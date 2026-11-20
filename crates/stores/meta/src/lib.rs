//! Meta Horizon Link provider (the Meta Quest PC app, formerly the Oculus app), Windows only.
//!
//! **Installed apps.** The app keeps its library folders under
//! `HKCU\Software\Oculus VR, LLC\Oculus\Libraries\{guid}` (`Path`, often a `\\?\Volume{...}`
//! path, and `OriginalPath`); the default one is `<install>\Software`, where `<install>` is the
//! `Base` value of `HKLM\SOFTWARE\(WOW6432Node\)Oculus VR, LLC\Oculus`. Each library has a manifest
//! per package and the package files in `Software\<canonicalName>` (see [`manifest`]). Games start
//! through their executable with the manifest's parameters, like the Meta app does (the VR
//! runtime is a service that starts the app when needed); sessions are tracked by install folder.
//! The Meta app has no way to hand it an uninstall, so Questkeep offers none.
//!
//! **Owned apps.** The Meta app caches the user's PC entitlements and the store details of the
//! apps for offline use (see [`catalog`]), so owned apps that are not installed are listed
//! without a sign-in, as of the last time the Meta app was online. Installing opens the app's page
//! in the Meta app (`oculus://view/app/<id>`), which has the Install button.
//!
//! **Art.** The Meta app downloads each library app's store art to
//! `<install>\CoreData\Software\StoreAssets\<canonicalName>_assets\` (square cover, wide cover,
//! icon); the cache also has their (expiring) download URLs.
//!
//! The library folder layout and the store art folder were first described by the Playnite
//! OculusLibrary extension (see THIRD_PARTY_NOTICES.md); the cache format was worked out from the
//! files the app writes.

/// The store's logo: a monochrome SVG drawn with `currentColor` (an original headset glyph).
pub const ICON: &str = include_str!("../icon.svg");

pub mod blob;
pub mod catalog;
pub mod manifest;
#[cfg(windows)]
mod registry;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use questkeep_store_api::model::{
    GameLink, GameMetadata, ImportedGame, PlayAction, Score, TrackingMode,
};
use questkeep_store_api::{ImageCandidate, LibraryProvider, ProviderError};

use catalog::Catalog;
use manifest::{Install, name_from_canonical, plausible_app_id};

pub const META_SOURCE: &str = "meta";
/// How long a read of the app's cache is reused (a sync reads it for both scans, then the image
/// fetcher asks per game).
const CATALOG_TTL: Duration = Duration::from_secs(60);

/// Where the Meta app keeps its data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Paths {
    /// Library folders, each with `Manifests` and `Software`.
    pub libraries: Vec<PathBuf>,
    /// The app's install folder (`C:\Program Files\Meta Horizon`).
    pub install_dir: Option<PathBuf>,
    /// `%APPDATA%\Oculus\Sessions`.
    pub sessions: Option<PathBuf>,
}

impl Paths {
    /// The paths of this PC (empty elsewhere).
    pub fn detect() -> Self {
        #[cfg(windows)]
        {
            let install_dir = registry::install_dir();
            let mut libraries: Vec<PathBuf> = registry::libraries()
                .into_iter()
                .filter_map(|(path, original)| {
                    manifest::resolve_library(&path, original.as_deref())
                })
                .collect();
            // The default library, which older installs don't always list.
            if let Some(default) = install_dir.as_ref().map(|d| d.join("Software"))
                && default.join("Manifests").is_dir()
            {
                libraries.push(default);
            }
            let mut unique: Vec<PathBuf> = Vec::new();
            for library in libraries {
                let same = |known: &PathBuf| {
                    known.to_string_lossy().to_lowercase()
                        == library.to_string_lossy().to_lowercase()
                };
                if !unique.iter().any(same) {
                    unique.push(library);
                }
            }
            Self {
                libraries: unique,
                install_dir,
                sessions: dirs::config_dir().map(|d| d.join("Oculus").join("Sessions")),
            }
        }
        #[cfg(not(windows))]
        Self::default()
    }

    fn core_data(&self) -> Option<PathBuf> {
        self.install_dir.as_ref().map(|d| d.join("CoreData"))
    }

    /// The downloaded store art of an app.
    fn store_assets(&self, canonical: &str) -> Option<PathBuf> {
        let dir = self
            .core_data()?
            .join("Software")
            .join("StoreAssets")
            .join(format!("{canonical}_assets"));
        dir.is_dir().then_some(dir)
    }
}

/// The client's executable from the command of its `oculus://` protocol handler
/// (`"C:\...\Support\oculus-client\Client.exe" "--" "--url" "%1"`), when the file exists.
fn client_from_command(command: &str) -> Option<PathBuf> {
    let (program, _) = questkeep_store_api::platform::split_command_line(command)?;
    let exe = PathBuf::from(program);
    exe.is_file().then_some(exe)
}

/// The Meta app's page of an app (store details, Install button).
fn app_page(app_id: &str) -> Option<PlayAction> {
    plausible_app_id(app_id).then(|| PlayAction::uri(format!("oculus://view/app/{app_id}")))
}

/// Genre ids such as `ROLE_PLAYING` as `Role Playing`.
fn genre_name(genre: &str) -> String {
    name_from_canonical(&genre.to_lowercase())
}

enum System {
    Live,
    /// Fixed paths and no registry, for tests and tools.
    Fixed(Paths),
}

pub struct MetaProvider {
    system: System,
    catalog: Mutex<Option<(Instant, Arc<Catalog>)>>,
}

impl MetaProvider {
    pub fn detect() -> Self {
        Self::new(System::Live)
    }

    /// Uses fixed folders instead of the ones of this PC (tests and tools).
    pub fn with_paths(paths: Paths) -> Self {
        Self::new(System::Fixed(paths))
    }

    fn new(system: System) -> Self {
        Self {
            system,
            catalog: Mutex::new(None),
        }
    }

    pub fn paths(&self) -> Paths {
        match &self.system {
            System::Live => Paths::detect(),
            System::Fixed(paths) => paths.clone(),
        }
    }

    /// The installed store apps of every library, each app once.
    fn installs(&self, paths: &Paths) -> Vec<Install> {
        let mut installs: Vec<Install> = Vec::new();
        for library in &paths.libraries {
            for install in manifest::installs(library) {
                if !installs
                    .iter()
                    .any(|i| i.manifest.app_id == install.manifest.app_id)
                {
                    installs.push(install);
                }
            }
        }
        installs
    }

    /// The app's cache, read at most once a minute.
    pub fn catalog(&self) -> Result<Arc<Catalog>, ProviderError> {
        let mut cached = self.catalog.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, catalog)) = cached.as_ref()
            && at.elapsed() < CATALOG_TTL
        {
            return Ok(catalog.clone());
        }
        let sessions = self.paths().sessions.ok_or_else(|| {
            ProviderError::NotConfigured("The Meta Horizon Link app has no data on this PC.".into())
        })?;
        let catalog = Arc::new(catalog::read(&sessions)?);
        *cached = Some((Instant::now(), catalog.clone()));
        Ok(catalog)
    }

    /// The canonical name of an app (the name of its folders), from the cache, an install or its
    /// store art bundle.
    fn canonical_name(&self, paths: &Paths, app_id: &str) -> Option<String> {
        if let Some(name) = self
            .catalog()
            .ok()
            .and_then(|c| c.apps.get(app_id).and_then(|a| a.canonical_name.clone()))
        {
            return Some(name);
        }
        if let Some(install) = self
            .installs(paths)
            .into_iter()
            .find(|i| i.manifest.app_id.as_deref() == Some(app_id))
        {
            return Some(install.manifest.canonical_name);
        }
        let core_data = paths.core_data()?;
        manifest::asset_bundles(&core_data)
            .into_iter()
            .find(|(id, _)| id == app_id)
            .map(|(_, canonical)| canonical)
    }

    /// Local art files first (when they exist), then the cached URL.
    fn images(
        &self,
        app_id: &str,
        files: &[&str],
        url: fn(&catalog::Images) -> Option<&String>,
    ) -> Vec<ImageCandidate> {
        if !plausible_app_id(app_id) {
            return Vec::new();
        }
        let paths = self.paths();
        let mut out: Vec<ImageCandidate> = Vec::new();
        if let Some(dir) = self
            .canonical_name(&paths, app_id)
            .and_then(|c| paths.store_assets(&c))
        {
            out.extend(
                files
                    .iter()
                    .map(|f| dir.join(f))
                    .filter(|p| p.is_file())
                    .map(ImageCandidate::File),
            );
        }
        if let Some(link) = self
            .catalog()
            .ok()
            .and_then(|c| c.apps.get(app_id).and_then(|a| url(&a.images).cloned()))
        {
            out.push(ImageCandidate::Url(link));
        }
        out
    }
}

impl LibraryProvider for MetaProvider {
    fn id(&self) -> &str {
        META_SOURCE
    }

    fn name(&self) -> &str {
        "Meta Horizon Link"
    }

    fn shop_url(&self) -> Option<&'static str> {
        Some("https://www.meta.com/experiences/")
    }

    fn icon_svg(&self) -> &'static str {
        crate::ICON
    }

    fn can_scan(&self) -> bool {
        cfg!(windows) || matches!(self.system, System::Fixed(_))
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if !self.can_scan() {
            return Err(ProviderError::NotFound(
                "Meta Horizon Link is only available on Windows".into(),
            ));
        }
        let paths = self.paths();
        // Names and details are optional here; the cache may be missing or unreadable.
        let catalog = self.catalog().ok();
        let mut games: Vec<ImportedGame> = self
            .installs(&paths)
            .into_iter()
            .filter_map(|install| {
                let id = install.manifest.app_id.clone()?;
                let app = catalog.as_ref().and_then(|c| c.apps.get(&id));
                let name = app
                    .and_then(|a| a.name.clone())
                    .unwrap_or_else(|| name_from_canonical(&install.manifest.canonical_name));
                let dir = install.dir.to_string_lossy().into_owned();
                let mut action = PlayAction::file(install.exe.to_string_lossy());
                action.args = install
                    .manifest
                    .launch_parameters
                    .as_deref()
                    .map(str::trim)
                    .filter(|a| !a.is_empty())
                    .map(str::to_string);
                action.working_dir = install
                    .exe
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned());
                // Some games restart themselves or start a helper; follow the folder instead.
                action.tracking = TrackingMode::Directory;
                action.tracking_path = Some(dir.clone());
                Some(ImportedGame {
                    install_dir: Some(dir),
                    play_action: Some(action),
                    playtime_seconds: catalog.as_ref().and_then(|c| c.playtime.get(&id).copied()),
                    app: app.and_then(catalog::App::is_app),
                    ..ImportedGame::installed(id, name)
                })
            })
            .collect();
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    fn can_scan_owned(&self) -> bool {
        self.can_scan()
    }

    /// The owned PC apps from the Meta app's cache. When the cache lists no entitlements (its
    /// format changed, say), the apps whose store art the Meta app downloaded stand in.
    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        if !self.can_scan() {
            return Err(ProviderError::Unsupported);
        }
        let paths = self.paths();
        let catalog = self.catalog();
        let bundles = paths
            .core_data()
            .map(|d| manifest::asset_bundles(&d))
            .unwrap_or_default();
        let ids: Vec<String> = match &catalog {
            Ok(c) if !c.entitlements.is_empty() => c.owned(),
            _ => bundles.iter().map(|(id, _)| id.clone()).collect(),
        };
        if ids.is_empty()
            && let Err(err) = catalog
        {
            return Err(err);
        }
        let catalog = catalog.ok();
        let mut games: Vec<ImportedGame> = ids
            .into_iter()
            .filter_map(|id| {
                let app = catalog.as_ref().and_then(|c| c.apps.get(&id));
                let name = app.and_then(|a| a.name.clone()).or_else(|| {
                    let canonical = app.and_then(|a| a.canonical_name.clone()).or_else(|| {
                        bundles
                            .iter()
                            .find(|(b, _)| *b == id)
                            .map(|(_, c)| c.clone())
                    })?;
                    Some(name_from_canonical(&canonical))
                })?;
                let mut game = ImportedGame::installed(id.clone(), name);
                game.installed = false;
                game.app = app.and_then(catalog::App::is_app);
                game.playtime_seconds = catalog.as_ref().and_then(|c| c.playtime.get(&id).copied());
                Some(game)
            })
            .collect();
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    /// Opens the app's page in the Meta app, which offers the install.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        cfg!(windows).then(|| app_page(source_game_id)).flatten()
    }

    fn has_launcher_pages(&self) -> bool {
        self.launcher().is_some()
    }

    fn launcher_page(&self, source_game_id: &str) -> Option<PlayAction> {
        app_page(source_game_id)
    }

    /// Starts the Meta app, found through the command of its `oculus://` protocol handler, else
    /// in its install folder.
    fn launcher(&self) -> Option<PlayAction> {
        #[cfg(windows)]
        if matches!(self.system, System::Live) {
            let exe = questkeep_store_api::platform::registry_value(
                r"HKEY_CLASSES_ROOT\oculus\shell\open\command\",
            )
            .and_then(|command| client_from_command(&command))
            .or_else(|| {
                let exe = registry::install_dir()?
                    .join("Support")
                    .join("oculus-client")
                    .join("Client.exe");
                exe.is_file().then_some(exe)
            })?;
            return Some(PlayAction::file(exe.to_string_lossy()));
        }
        None
    }

    // No `launcher_process`: the client runs as `Client.exe`, a name too generic to close by
    // name without hitting other programs (`OculusClient.exe` next to it is an empty stub).

    fn metadata(&self, source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        if !plausible_app_id(source_game_id) {
            return Ok(None);
        }
        let catalog = match self.catalog() {
            Ok(c) => c,
            Err(ProviderError::NotConfigured(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        let Some(app) = catalog.apps.get(source_game_id) else {
            return Ok(None);
        };
        Ok(Some(GameMetadata {
            description: app.description.clone(),
            developers: app.developer.iter().cloned().collect(),
            genres: app.genres.iter().map(|g| genre_name(g)).collect(),
            features: vec!["VR".into()],
            platforms: app
                .is_pc()
                .then(|| "Windows".to_string())
                .into_iter()
                .collect(),
            community_score: app.rating.map(|r| Score {
                value: (r * 20.0).round().clamp(0.0, 100.0) as u8,
                source: "Meta Horizon Store".into(),
            }),
            links: vec![GameLink::new(
                "Meta Horizon Store",
                format!("https://www.meta.com/experiences/{source_game_id}/"),
            )],
            ..GameMetadata::default()
        }))
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.images(source_game_id, &["cover_square_image.jpg"], |i| {
            i.cover_square.as_ref()
        })
    }

    /// The store's covers are square.
    fn covers_are_low_quality(&self, _source_game_id: &str) -> bool {
        true
    }

    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.images(
            source_game_id,
            &[
                "cover_landscape_image_large.png",
                "cover_landscape_image.jpg",
            ],
            |i| i.cover_landscape.as_ref().or(i.small_landscape.as_ref()),
        )
    }

    fn icon_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.images(source_game_id, &["icon_image.jpg"], |i| i.icon.as_ref())
    }
}

/// Where a test or tool finds the files of a made-up Meta install (`fixtures/`).
#[cfg(test)]
fn fixture_paths(sessions: &std::path::Path) -> Paths {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    Paths {
        libraries: vec![fixtures.join("library"), fixtures.join("missing-library")],
        install_dir: Some(fixtures),
        sessions: Some(sessions.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {
    use super::catalog::testing::*;
    use super::*;
    use std::path::Path;

    const USER: &str = "1000000000001";

    fn provider_with_cache(tmp: &Path) -> MetaProvider {
        let sessions = tmp.join("Sessions");
        write_db(
            &sessions.join("_oaf").join("data.sqlite"),
            &[
                offline_data(USER),
                pc_app(
                    "2000000000000002",
                    "Moon Garden™",
                    "moonlit-studio-moon-garden",
                ),
                entitlement(USER, "2000000000000002"),
                entitlement(USER, "2000000000000003"),
                entitlement(USER, "2000000000000005"),
                playtime(USER, "2000000000000002", 5400),
            ],
        );
        write_db(
            &sessions.join(format!("{USER}-7")).join("data.sqlite"),
            &[short_app(
                "2000000000000005",
                "Pocket Quest",
                "ANDROID_6DOF",
            )],
        );
        MetaProvider::with_paths(fixture_paths(&sessions))
    }

    #[test]
    fn finds_the_client_in_a_protocol_command() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("Client.exe");
        let command = format!("\"{}\" \"--\" \"--url\" \"%1\"", exe.display());
        assert_eq!(client_from_command(&command), None);
        std::fs::write(&exe, "x").unwrap();
        assert_eq!(client_from_command(&command), Some(exe));
        assert_eq!(client_from_command(""), None);
    }

    #[test]
    fn scans_installed_apps_with_names_from_the_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = provider_with_cache(tmp.path());
        let games = provider.scan_installed().unwrap();
        let ids: Vec<_> = games
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                ("2000000000000002", "Moon Garden"),
                ("2000000000000004", "Tiny Forge Sky Loop")
            ]
        );
        let moon = &games[0];
        let library = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("library")
            .join("Software");
        let dir = library.join("moonlit-studio-moon-garden");
        assert_eq!(moon.install_dir.as_deref(), Some(&*dir.to_string_lossy()));
        assert_eq!(moon.playtime_seconds, Some(5400));
        assert_eq!(moon.app, Some(false));
        let action = moon.play_action.as_ref().unwrap();
        assert_eq!(Path::new(&action.target), dir.join("MoonGarden.exe"));
        assert_eq!(action.args.as_deref(), Some("-vrmode oculus"));
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(action.tracking_path, moon.install_dir);
        assert!(games[1].play_action.as_ref().unwrap().args.is_none());
        assert_eq!(games[1].app, None);
        assert!(!provider.can_uninstall());
    }

    #[test]
    fn scans_installed_apps_without_a_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = MetaProvider::with_paths(fixture_paths(&tmp.path().join("none")));
        let names: Vec<_> = provider
            .scan_installed()
            .unwrap()
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(names, ["Moonlit Studio Moon Garden", "Tiny Forge Sky Loop"]);
    }

    #[test]
    fn scans_owned_pc_apps() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = provider_with_cache(tmp.path());
        let owned = provider.scan_owned(&serde_json::Value::Null).unwrap();
        let ids: Vec<_> = owned
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str(), g.installed))
            .collect();
        // 2000000000000003 has no app object: its name comes from its store art bundle.
        // 2000000000000005 is a Quest app.
        assert_eq!(
            ids,
            [
                ("2000000000000002", "Moon Garden", false),
                ("2000000000000003", "Starfall Arcade", false)
            ]
        );
        assert_eq!(
            provider
                .install_action("2000000000000003")
                .map(|a| a.target),
            cfg!(windows).then(|| "oculus://view/app/2000000000000003".to_string())
        );
        assert!(provider.install_action("1&x=2").is_none());
        assert_eq!(
            provider.launcher_page("2000000000000002").unwrap().target,
            "oculus://view/app/2000000000000002"
        );
    }

    #[test]
    fn owned_apps_fall_back_to_store_art_bundles() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = MetaProvider::with_paths(fixture_paths(&tmp.path().join("none")));
        let owned: Vec<_> = provider
            .scan_owned(&serde_json::Value::Null)
            .unwrap()
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(owned, ["Moonlit Studio Moon Garden", "Starfall Arcade"]);

        // Without a cache or store art there is nothing to read.
        let empty = MetaProvider::with_paths(Paths {
            sessions: Some(tmp.path().join("none")),
            ..Paths::default()
        });
        assert!(matches!(
            empty.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::NotConfigured(_))
        ));
    }

    #[test]
    fn offers_local_art_before_cached_urls() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = provider_with_cache(tmp.path());
        let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/CoreData/Software/StoreAssets/moonlit-studio-moon-garden_assets");
        assert_eq!(
            provider.cover_candidates("2000000000000002"),
            [
                ImageCandidate::File(assets.join("cover_square_image.jpg")),
                ImageCandidate::Url("https://cdn.invalid/2000000000000002/square.png".into())
            ]
        );
        assert!(provider.covers_are_low_quality("2000000000000002"));
        assert_eq!(
            provider.background_candidates("2000000000000002")[..2],
            [
                ImageCandidate::File(assets.join("cover_landscape_image_large.png")),
                ImageCandidate::File(assets.join("cover_landscape_image.jpg")),
            ]
        );
        assert_eq!(
            provider.icon_candidates("2000000000000002")[0],
            ImageCandidate::File(assets.join("icon_image.jpg"))
        );
        // No art for an app the cache doesn't know and that has no art folder.
        assert!(provider.cover_candidates("2000000000000004").is_empty());
        assert!(provider.cover_candidates("../x").is_empty());
    }

    #[test]
    fn reads_metadata_from_the_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = provider_with_cache(tmp.path());
        let meta = provider.metadata("2000000000000002").unwrap().unwrap();
        assert_eq!(
            meta.description.as_deref(),
            Some("Grow a garden on the moon.")
        );
        assert_eq!(meta.developers, ["Moonlit Studio"]);
        assert_eq!(meta.genres, ["Adventure", "Casual"]);
        assert_eq!(meta.platforms, ["Windows"]);
        let score = meta.community_score.unwrap();
        assert_eq!(
            (score.value, score.source.as_str()),
            (90, "Meta Horizon Store")
        );
        assert_eq!(
            meta.links[0].url,
            "https://www.meta.com/experiences/2000000000000002/"
        );
        assert!(provider.metadata("2000000000000003").unwrap().is_none());
        let none = MetaProvider::with_paths(fixture_paths(&tmp.path().join("none")));
        assert!(none.metadata("2000000000000002").unwrap().is_none());
    }

    #[test]
    fn launcher_is_none_without_the_live_system() {
        let provider = MetaProvider::with_paths(Paths::default());
        assert!(provider.launcher().is_none());
        assert!(provider.launcher_process().is_none());
        assert!(!provider.has_launcher_pages());
    }

    #[test]
    fn genre_names_are_readable() {
        assert_eq!(genre_name("ROLE_PLAYING"), "Role Playing");
        assert_eq!(genre_name("ACTION"), "Action");
    }
}
