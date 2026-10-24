//! Epic Games Store provider: finds installed games from the Epic Games Launcher's local data
//! (`LauncherInstalled.dat` and the `*.item` manifests). No network access and no Epic login.
//!
//! Signing in to an Epic account (`account`, optional) adds the owned-but-not-installed games,
//! playtime and metadata from Epic's library and catalog services. Covers, backgrounds and icons
//! of installed games also work without an account: the manifests carry the catalog namespace
//! and item id, and `public` asks the catalog with an app-only token (`client_credentials`).
//! Scanning installed games itself never touches the network.
//!
//! On Linux the games come from Heroic Games Launcher / Legendary (`super::heroic`), with the same
//! `AppName` ids as on Windows, and launch through Heroic's `heroic://launch/...` URI.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Deserialize;

use super::heroic::{self, HeroicPaths};
use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::auth::{AccountAuth, system_clock};
use crate::model::{GameMetadata, ImportedGame, PlayAction};

mod account;
mod cache;
mod public;
mod web;

pub use account::{EpicAccount, REDIRECT_URL as CODE_REDIRECT_URL};

pub const EPIC_SOURCE: &str = "epic";

/// Catalog namespace and item id of an installed game.
type CatalogIds = (String, String);

/// Reads installed games from an Epic launcher data directory (the `Epic` folder under
/// `%ProgramData%`, containing `UnrealEngineLauncher` and `EpicGamesLauncher`).
#[derive(Debug, Clone, Default)]
pub struct EpicProvider {
    data_root: Option<PathBuf>,
    /// Heroic/Legendary directories (Linux).
    heroic: Option<HeroicPaths>,
    /// Signed-in account, when the app supplies token storage.
    account: Option<Arc<EpicAccount>>,
    /// Account-free catalog access for installed games.
    public: Arc<public::PublicCatalog>,
    /// Where catalog answers are cached on disk, when the app supplies a directory.
    cache_dir: Option<PathBuf>,
    /// Catalog ids by lowercase appName, read from the installed-game data on demand.
    ids: Arc<Mutex<HashMap<String, CatalogIds>>>,
}

impl EpicProvider {
    /// Uses the launcher data of this machine (Epic Games Launcher on Windows, Heroic/Legendary
    /// elsewhere).
    pub fn detect() -> Self {
        Self {
            data_root: default_data_root(),
            heroic: default_heroic(),
            ..Self::default()
        }
    }

    /// Replaces the account-free catalog access (tests: fake network).
    #[cfg(test)]
    fn with_public(mut self, public: public::PublicCatalog) -> Self {
        self.public = Arc::new(public);
        self
    }

    /// Enables account sign-in, the owned-games list, covers and metadata.
    pub fn with_account(mut self, account: Arc<EpicAccount>) -> Self {
        if let Some(dir) = &self.cache_dir {
            account.set_disk_cache(Arc::new(cache::CatalogDiskCache::for_account(
                dir,
                system_clock(),
            )));
        }
        self.account = Some(account);
        self
    }

    /// Keeps Epic catalog answers (covers, backgrounds, metadata) on disk below `dir`, usually
    /// the app's data directory, so they survive restarts (see `cache` for the expiry). Works in
    /// either order with [`with_account`](Self::with_account).
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        self.public
            .set_disk_cache(Arc::new(cache::CatalogDiskCache::new(&dir, system_clock())));
        if let Some(account) = &self.account {
            account.set_disk_cache(Arc::new(cache::CatalogDiskCache::for_account(
                &dir,
                system_clock(),
            )));
        }
        self.cache_dir = Some(dir);
        self
    }

    /// Uses the given launcher data directory.
    pub fn with_data_root(root: impl Into<PathBuf>) -> Self {
        Self {
            data_root: Some(root.into()),
            ..Self::default()
        }
    }

    /// Reads Heroic/Legendary data from the given directories (any platform; tests and tools).
    pub fn with_heroic(paths: HeroicPaths) -> Self {
        Self {
            heroic: Some(paths),
            ..Self::default()
        }
    }
}

#[cfg(windows)]
fn default_data_root() -> Option<PathBuf> {
    let program_data = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    Some(program_data.join("Epic"))
}

#[cfg(not(windows))]
fn default_data_root() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn default_heroic() -> Option<HeroicPaths> {
    None
}

#[cfg(not(windows))]
fn default_heroic() -> Option<HeroicPaths> {
    Some(HeroicPaths::detect())
}

/// `LauncherInstalled.dat`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct LauncherInstalled {
    #[serde(default)]
    installation_list: Vec<InstalledApp>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InstalledApp {
    #[serde(default)]
    install_location: Option<String>,
    #[serde(default)]
    app_name: Option<String>,
}

/// The fields of an `*.item` manifest that matter here.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    #[serde(default)]
    app_name: Option<String>,
    #[serde(default)]
    main_game_app_name: Option<String>,
    #[serde(default)]
    catalog_namespace: Option<String>,
    #[serde(default)]
    catalog_item_id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    install_location: Option<String>,
    #[serde(default)]
    app_categories: Option<Vec<String>>,
    #[serde(default)]
    compatible_apps: Option<Vec<String>>,
    #[serde(default)]
    technical_type: Option<String>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// DLC and add-ons (unless launchable), Unreal Engine plugins and engine installs.
fn is_not_a_game(m: &Manifest, app_name: &str) -> bool {
    let cats = m.app_categories.as_deref().unwrap_or(&[]);
    let launchable_addon = cats.iter().any(|c| c == "addons/launchable");
    if cats.iter().any(|c| c == "addons") && !launchable_addon {
        return true;
    }
    if let Some(main) = non_empty(&m.main_game_app_name)
        && !main.eq_ignore_ascii_case(app_name)
        && !launchable_addon
    {
        return true;
    }
    cats.iter().any(|c| c == "plugins" || c == "plugins/engine")
        || m.compatible_apps
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .any(|a| a.starts_with("UE_"))
        || m.technical_type
            .as_deref()
            .is_some_and(|t| t.contains("plugins/engine"))
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `com.epicgames.launcher://apps/<namespace>%3A<catalogItemId>%3A<appName>?action=launch&silent=true`
fn launch_uri(namespace: &str, item_id: &str, app_name: &str) -> String {
    format!(
        "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
        percent_encode(namespace),
        percent_encode(item_id),
        percent_encode(app_name)
    )
}

/// `com.epicgames.launcher://apps/<namespace>%3A<catalogItemId>%3A<appName>?action=install`
fn install_uri(namespace: &str, item_id: &str, app_name: &str) -> String {
    format!(
        "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=install",
        percent_encode(namespace),
        percent_encode(item_id),
        percent_encode(app_name)
    )
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

impl EpicProvider {
    fn catalog_images(
        &self,
        source_game_id: &str,
        pick: impl Fn(&web::CatalogItem) -> Vec<String>,
    ) -> Vec<ImageCandidate> {
        // The signed-in account first (its token, its library); the account-free lookup by the
        // installed manifest's ids covers signed-out users and games the account does not own.
        if let Some(account) = self.account.as_ref().filter(|a| a.is_signed_in()) {
            match account.catalog_item(source_game_id) {
                Ok(Some(item)) => {
                    let urls = pick(&item);
                    if !urls.is_empty() {
                        return urls.into_iter().map(ImageCandidate::Url).collect();
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::debug!(game = source_game_id, %err, "Epic image lookup failed");
                }
            }
        }
        let Some((namespace, item_id)) = self.catalog_ids(source_game_id) else {
            return Vec::new();
        };
        match self.public.item(&namespace, &item_id) {
            Ok(Some(item)) => pick(&item).into_iter().map(ImageCandidate::Url).collect(),
            Ok(None) => Vec::new(),
            Err(err) => {
                tracing::debug!(game = source_game_id, %err, "Epic public image lookup failed");
                Vec::new()
            }
        }
    }

    /// Namespace and catalog item id of an installed game.
    fn catalog_ids(&self, app_name: &str) -> Option<CatalogIds> {
        let key = app_name.to_lowercase();
        let cached = |this: &Self| {
            this.ids
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&key)
                .cloned()
        };
        if let Some(found) = cached(self) {
            return Some(found);
        }
        let fresh = if let Some(paths) = &self.heroic {
            legendary_ids(paths, app_name)
                .map(|ids| HashMap::from([(key.clone(), ids)]))
                .unwrap_or_default()
        } else if let Some(root) = &self.data_root {
            manifest_ids(root)
        } else {
            HashMap::new()
        };
        self.ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(fresh);
        cached(self)
    }
}

/// Namespace and item id of every `*.item` manifest under an Epic data root, by lowercase
/// appName.
fn manifest_ids(root: &Path) -> HashMap<String, CatalogIds> {
    let dir = root
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if !path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("item"))
        {
            continue;
        }
        let Ok(manifest) = read_json::<Manifest>(&path) else {
            continue;
        };
        if let (Some(app), Some(ns), Some(item)) = (
            non_empty(&manifest.app_name),
            non_empty(&manifest.catalog_namespace),
            non_empty(&manifest.catalog_item_id),
        ) {
            out.insert(app.to_lowercase(), (ns.to_string(), item.to_string()));
        }
    }
    out
}

/// Namespace and item id from a Legendary `metadata/<appName>.json`: `metadata.namespace` and
/// `metadata.id`, or the `catalog_item_id`/`namespace` of an `asset_infos` platform entry.
fn parse_legendary_ids(json: &str) -> Option<CatalogIds> {
    let value: serde_json::Value =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()?;
    let text = |v: Option<&serde_json::Value>| {
        v.and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let meta = value.get("metadata");
    if let (Some(ns), Some(id)) = (
        text(meta.and_then(|m| m.get("namespace"))),
        text(meta.and_then(|m| m.get("id"))),
    ) {
        return Some((ns, id));
    }
    let infos = value.get("asset_infos").and_then(|a| a.as_object());
    let entries = infos
        .into_iter()
        .flat_map(|m| m.values())
        .chain(value.get("asset_info"));
    for info in entries {
        if let (Some(ns), Some(id)) = (
            text(info.get("namespace")),
            text(info.get("catalog_item_id")),
        ) {
            return Some((ns, id));
        }
    }
    None
}

fn legendary_ids(paths: &HeroicPaths, app_name: &str) -> Option<CatalogIds> {
    if app_name.is_empty() || app_name.contains(['/', '\\']) || app_name.contains("..") {
        return None;
    }
    paths.legendary.iter().find_map(|source| {
        let file = source.dir.join("metadata").join(format!("{app_name}.json"));
        parse_legendary_ids(&std::fs::read_to_string(file).ok()?)
    })
}

impl LibraryProvider for EpicProvider {
    fn id(&self) -> &str {
        EPIC_SOURCE
    }

    fn name(&self) -> &str {
        "Epic Games"
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if let Some(paths) = &self.heroic {
            return heroic::legendary_games(paths);
        }
        let Some(root) = &self.data_root else {
            return Err(ProviderError::NotFound(
                "Epic Games needs the Epic Games Launcher (Windows) or Heroic/Legendary (Linux)"
                    .into(),
            ));
        };
        let manifests_dir = root
            .join("EpicGamesLauncher")
            .join("Data")
            .join("Manifests");
        if !root.is_dir() || !manifests_dir.is_dir() {
            return Err(ProviderError::NotFound(
                "Epic Games Launcher data not found (is the launcher installed?)".into(),
            ));
        }

        // The install list can be missing or stale; it is only used to resolve conflicts.
        let list_path = root
            .join("UnrealEngineLauncher")
            .join("LauncherInstalled.dat");
        let list: LauncherInstalled = if list_path.is_file() {
            read_json(&list_path).unwrap_or_else(|e| {
                tracing::warn!("could not parse {}: {e}", list_path.display());
                LauncherInstalled::default()
            })
        } else {
            LauncherInstalled::default()
        };

        let entries = std::fs::read_dir(&manifests_dir).map_err(|source| ProviderError::Io {
            path: manifests_dir.display().to_string(),
            source,
        })?;
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("item"))
            })
            .collect();
        paths.sort();

        let mut games: Vec<ImportedGame> = Vec::new();
        for path in paths {
            let manifest: Manifest = match read_json(&path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("skipping Epic manifest {}: {e}", path.display());
                    continue;
                }
            };
            let Some(app_name) = non_empty(&manifest.app_name) else {
                tracing::warn!("skipping Epic manifest without AppName: {}", path.display());
                continue;
            };
            if is_not_a_game(&manifest, app_name) {
                continue;
            }
            let listed = list
                .installation_list
                .iter()
                .find(|a| non_empty(&a.app_name).is_some_and(|n| n.eq_ignore_ascii_case(app_name)));
            let listed_dir = listed.and_then(|a| non_empty(&a.install_location));
            // The manifest location can be stale after a move; the install list is preferred then.
            let dir = [non_empty(&manifest.install_location), listed_dir]
                .into_iter()
                .flatten()
                .find(|d| Path::new(d).is_dir());
            let Some(dir) = dir else {
                tracing::warn!("Epic game {app_name} has no existing install folder, skipped");
                continue;
            };
            let name = non_empty(&manifest.display_name)
                .map(str::to_string)
                .or_else(|| {
                    Path::new(dir)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| app_name.to_string());

            let mut game = ImportedGame::installed(app_name, name);
            game.install_dir = Some(dir.to_string());
            if let (Some(ns), Some(item)) = (
                non_empty(&manifest.catalog_namespace),
                non_empty(&manifest.catalog_item_id),
            ) {
                game.play_action = Some(PlayAction::uri(launch_uri(ns, item, app_name)));
            } else {
                tracing::warn!("Epic game {app_name} has no catalog ids, cannot launch it");
            }

            // Several manifests for one game: the one matching the install list wins.
            if let Some(existing) = games
                .iter_mut()
                .find(|g| g.source_game_id.eq_ignore_ascii_case(app_name))
            {
                if listed_dir.is_some_and(|l| same_path(l, dir)) {
                    *existing = game;
                }
            } else {
                games.push(game);
            }
        }
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    fn can_scan_owned(&self) -> bool {
        self.account.is_some()
    }

    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        match &self.account {
            Some(account) => account.scan_owned(),
            None => Err(ProviderError::Unsupported),
        }
    }

    fn account(&self) -> Option<&dyn AccountAuth> {
        self.account.as_deref().map(|a| a as &dyn AccountAuth)
    }

    /// Description, developer and release date from the catalog (needs a signed-in account;
    /// signed out is an error rather than "no data", so the lookup is retried after sign-in).
    fn metadata(&self, source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        match &self.account {
            Some(account) => Ok(account
                .catalog_item(source_game_id)?
                .map(|item| item.metadata())),
            None => Ok(None),
        }
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.catalog_images(source_game_id, |item| item.cover_urls())
    }

    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.catalog_images(source_game_id, |item| item.background_urls())
    }

    fn icon_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.catalog_images(source_game_id, |item| item.icon_urls())
    }

    /// Asks the launcher to install the game. Current launchers only act on the full
    /// `<namespace>:<catalogItemId>:<appName>` id (the bare appName just opens the launcher), so
    /// the signed-in library supplies the ids; without them the bare form is the fallback.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        // Heroic's protocol handler has no install command (it only offers to install when
        // launching an uninstalled game), so there is nothing to open.
        if self.heroic.is_some() {
            return None;
        }
        let record = self
            .account
            .as_ref()
            .and_then(|a| a.library_record(source_game_id));
        Some(PlayAction::uri(match record {
            Some(r) => install_uri(&r.namespace, &r.catalog_item_id, &r.app_name),
            None => format!(
                "com.epicgames.launcher://apps/{}?action=install",
                percent_encode(source_game_id)
            ),
        }))
    }

    /// Windows: the Epic launcher. Linux: only possible for games of a standalone Legendary
    /// (with `legendary` on `PATH`); Heroic has no uninstall protocol.
    fn can_uninstall(&self) -> bool {
        match &self.heroic {
            None => true,
            Some(paths) => {
                paths.legendary_bin.is_some() && paths.legendary.iter().any(|s| !s.via_heroic)
            }
        }
    }

    /// The Epic launcher has no per-game uninstall URI (searched the launcher's files and the web;
    /// nothing documented or known to work, 2026-10-25), so on Windows this opens its library,
    /// where the game's menu has Uninstall (Playnite does the same). Games installed by a
    /// standalone Legendary are uninstalled per game with `legendary uninstall <app> -y`
    /// (Legendary's own command; the UI asks first). Heroic-managed games have no action.
    fn uninstall_action(&self, source_game_id: &str) -> Option<PlayAction> {
        match &self.heroic {
            None => Some(PlayAction::uri(EPIC_LIBRARY_URI)),
            Some(paths) => legendary_uninstall_action(paths, source_game_id),
        }
    }
}

/// `legendary uninstall <app> -y` when `app_name` is installed in a standalone Legendary
/// directory (not one Heroic manages).
fn legendary_uninstall_action(paths: &HeroicPaths, app_name: &str) -> Option<PlayAction> {
    let bin = paths.legendary_bin.as_ref()?;
    // The name ends up on a command line, so only plain Legendary ids are accepted.
    let plain = !app_name.is_empty()
        && app_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !app_name.starts_with('-');
    if !plain {
        return None;
    }
    let installed_in = |via_heroic: bool| {
        paths
            .legendary
            .iter()
            .filter(|s| s.via_heroic == via_heroic)
            .any(|source| {
                std::fs::read_to_string(source.dir.join("installed.json"))
                    .ok()
                    .and_then(|text| heroic::parse_legendary_installed(&text).ok())
                    .is_some_and(|list| {
                        list.iter()
                            .any(|i| i.app_name.eq_ignore_ascii_case(app_name))
                    })
            })
    };
    // A game that Heroic also manages must be uninstalled in Heroic, which keeps its own state.
    if !installed_in(false) || installed_in(true) {
        return None;
    }
    let mut action = PlayAction::file(bin.display().to_string());
    action.args = Some(format!("uninstall {app_name} -y"));
    Some(action)
}

/// Opens the Epic Games Launcher on its library page.
const EPIC_LIBRARY_URI: &str = "com.epicgames.launcher://store/library";

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::ActionKind;

    struct Env {
        tmp: tempfile::TempDir,
    }

    impl Env {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            fs::create_dir_all(tmp.path().join("EpicGamesLauncher/Data/Manifests")).unwrap();
            fs::create_dir_all(tmp.path().join("UnrealEngineLauncher")).unwrap();
            Self { tmp }
        }
        fn root(&self) -> &Path {
            self.tmp.path()
        }
        fn game_dir(&self, name: &str) -> String {
            let d = self.root().join("games").join(name);
            fs::create_dir_all(&d).unwrap();
            d.to_string_lossy().into_owned()
        }
        fn manifest(&self, file: &str, json: &str) {
            fs::write(
                self.root()
                    .join("EpicGamesLauncher/Data/Manifests")
                    .join(file),
                json,
            )
            .unwrap();
        }
        fn install_list(&self, json: &str) {
            fs::write(
                self.root()
                    .join("UnrealEngineLauncher/LauncherInstalled.dat"),
                json,
            )
            .unwrap();
        }
        fn scan(&self) -> Vec<ImportedGame> {
            EpicProvider::with_data_root(self.root())
                .scan_installed()
                .unwrap()
        }
    }

    /// A manifest JSON; `extra` is a JSON object whose fields are merged in.
    fn item(app: &str, name: &str, dir: &str, extra: serde_json::Value) -> String {
        let mut v = serde_json::json!({
            "FormatVersion": 0,
            "AppName": app,
            "DisplayName": name,
            "CatalogNamespace": "ns1",
            "CatalogItemId": "item1",
            "InstallLocation": dir,
            "InstallSize": 123,
        });
        if let (Some(base), Some(add)) = (v.as_object_mut(), extra.as_object()) {
            base.extend(add.clone());
        }
        v.to_string()
    }

    fn none() -> serde_json::Value {
        serde_json::json!({})
    }

    #[test]
    fn parses_installed_game() {
        let env = Env::new();
        let dir = env.game_dir("Fortnite");
        env.manifest("a.item", &item("Sugar", "Sugar Game", &dir, none()));
        let games = env.scan();
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert_eq!(g.source_game_id, "Sugar");
        assert_eq!(g.name, "Sugar Game");
        assert_eq!(g.install_dir.as_deref(), Some(dir.as_str()));
        let action = g.play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(
            action.target,
            "com.epicgames.launcher://apps/ns1%3Aitem1%3ASugar?action=launch&silent=true"
        );
    }

    #[test]
    fn skips_dlc_plugins_and_missing_folders() {
        let env = Env::new();
        let dir = env.game_dir("G");
        env.manifest("game.item", &item("Main", "Main", &dir, none()));
        env.manifest(
            "dlc.item",
            &item(
                "Dlc",
                "Dlc",
                &dir,
                serde_json::json!({"AppCategories": ["addons", "games"]}),
            ),
        );
        env.manifest(
            "dlc2.item",
            &item(
                "Dlc2",
                "Dlc2",
                &dir,
                serde_json::json!({"MainGameAppName": "Main"}),
            ),
        );
        env.manifest(
            "plug.item",
            &item(
                "Plug",
                "Plug",
                &dir,
                serde_json::json!({"AppCategories": ["plugins/engine"]}),
            ),
        );
        env.manifest(
            "ue.item",
            &item(
                "Ue",
                "Ue",
                &dir,
                serde_json::json!({"CompatibleApps": ["UE_5.3"]}),
            ),
        );
        env.manifest(
            "gone.item",
            &item("Gone", "Gone", "C:/definitely/not/here", none()),
        );
        env.manifest(
            "launchable.item",
            &item(
                "Launch",
                "Launch",
                &dir,
                serde_json::json!({
                    "AppCategories": ["addons", "addons/launchable"],
                    "MainGameAppName": "Main"
                }),
            ),
        );
        let ids: Vec<_> = env.scan().into_iter().map(|g| g.source_game_id).collect();
        assert_eq!(ids, vec!["Launch", "Main"]);
    }

    #[test]
    fn falls_back_to_install_list_location() {
        let env = Env::new();
        let real = env.game_dir("Moved");
        env.manifest("a.item", &item("Moved", "Moved", "C:/old/place", none()));
        env.install_list(
            &serde_json::json!({"InstallationList":[{"AppName":"moved","InstallLocation":real}]})
                .to_string(),
        );
        let games = env.scan();
        assert_eq!(games[0].install_dir.as_deref(), Some(real.as_str()));
    }

    #[test]
    fn duplicate_manifests_prefer_install_list_match() {
        let env = Env::new();
        let a = env.game_dir("A");
        let b = env.game_dir("B");
        env.manifest("1.item", &item("Dup", "Dup", &a, none()));
        env.manifest("2.item", &item("Dup", "Dup", &b, none()));
        env.install_list(
            &serde_json::json!({"InstallationList":[{"AppName":"Dup","InstallLocation":b}]})
                .to_string(),
        );
        let games = env.scan();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].install_dir.as_deref(), Some(b.as_str()));
    }

    #[test]
    fn malformed_files_do_not_panic() {
        let env = Env::new();
        let dir = env.game_dir("Ok");
        env.manifest("ok.item", &item("Ok", "Ok", &dir, none()));
        env.manifest("empty.item", "");
        env.manifest("broken.item", "{ not json");
        env.manifest("noname.item", "{}");
        env.manifest("wrongtype.item", r#"{"AppName": 5}"#);
        env.manifest("readme.txt", "ignored");
        env.install_list("garbage");
        let games = env.scan();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].source_game_id, "Ok");
    }

    #[test]
    fn missing_launcher_data_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let err = EpicProvider::with_data_root(tmp.path().join("nope"))
            .scan_installed()
            .unwrap_err();
        assert!(matches!(err, ProviderError::NotFound(_)));
        let err = EpicProvider::default().scan_installed().unwrap_err();
        assert!(matches!(err, ProviderError::NotFound(_)));
    }

    #[test]
    fn heroic_mode_scans_legendary_and_has_no_install_action() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("g/Sugar");
        fs::create_dir_all(&game).unwrap();
        let dir = tmp.path().join(".config/heroic/legendaryConfig/legendary");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("installed.json"),
            serde_json::json!({"Sugar": {"app_name": "Sugar", "title": "Sugar Game",
                "install_path": game, "is_dlc": false, "platform": "Windows"}})
            .to_string(),
        )
        .unwrap();
        let p = EpicProvider::with_heroic(HeroicPaths::discover(tmp.path(), None, None));
        let games = p.scan_installed().unwrap();
        assert_eq!(games[0].source_game_id, "Sugar");
        assert_eq!(
            games[0].play_action.as_ref().unwrap().target,
            "heroic://launch/legendary/Sugar"
        );
        assert!(p.install_action("Sugar").is_none());
        let empty = tempfile::tempdir().unwrap();
        let none = EpicProvider::with_heroic(HeroicPaths::discover(empty.path(), None, None));
        assert!(matches!(
            none.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
    }

    const APP_TOKEN: &str = include_str!("../../../tests/fixtures/epic/app_token.json");
    const CATALOG: &str = include_str!("../../../tests/fixtures/epic/catalog_bulk.json");
    const WIDE_ONLY: &str = include_str!("../../../tests/fixtures/epic/catalog_wide_only.json");

    /// A provider over `env` whose public catalog talks to a scripted Epic.
    fn provider_with_fake_epic(env: &Env) -> (EpicProvider, Arc<crate::auth::testing::FakeHttp>) {
        use crate::auth::testing::{FakeHttp, response};
        let http = Arc::new(FakeHttp::new(|req| {
            if req.url.contains("/oauth/token") {
                response(200, APP_TOKEN)
            } else if req.url.contains("/ns-wide/") {
                response(200, WIDE_ONLY)
            } else if req.url.contains("/ns-sugar/") {
                response(200, CATALOG)
            } else {
                response(404, "")
            }
        }));
        let provider = EpicProvider::with_data_root(env.root())
            .with_public(public::PublicCatalog::with(http.clone(), Arc::new(|| 10)));
        (provider, http)
    }

    fn urls(candidates: Vec<ImageCandidate>) -> Vec<String> {
        candidates
            .into_iter()
            .filter_map(|c| match c {
                ImageCandidate::Url(u) => Some(u),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn manifests_give_catalog_ids_for_account_free_covers() {
        let env = Env::new();
        let dir = env.game_dir("Sugar");
        env.manifest(
            "a.item",
            &item(
                "Sugar",
                "Sugar Game",
                &dir,
                serde_json::json!({"CatalogNamespace": "ns-sugar", "CatalogItemId": "sugar-item"}),
            ),
        );
        let (provider, http) = provider_with_fake_epic(&env);
        assert_eq!(
            provider.catalog_ids("sugar"),
            Some(("ns-sugar".to_string(), "sugar-item".to_string()))
        );
        // No account: tall art first, wide art as background, thumbnail as icon.
        assert_eq!(
            urls(provider.cover_candidates("Sugar")),
            [
                "https://cdn.example/sugar-tall.jpg",
                "https://cdn.example/sugar-offer-tall.jpg"
            ]
        );
        assert_eq!(
            urls(provider.background_candidates("Sugar")),
            [
                "https://cdn.example/sugar-wide.jpg",
                "https://cdn.example/sugar-offer-wide.jpg"
            ]
        );
        assert_eq!(
            urls(provider.icon_candidates("Sugar")),
            ["https://cdn.example/sugar-thumb.jpg"]
        );
        // One token and one catalog request for all three lookups.
        assert_eq!(http.urls().len(), 2);
        assert!(provider.catalog_ids("Unknown").is_none());
        assert!(provider.cover_candidates("Unknown").is_empty());
    }

    #[test]
    fn wide_only_art_gives_no_cover_but_a_background() {
        let env = Env::new();
        let dir = env.game_dir("Wide");
        env.manifest(
            "w.item",
            &item(
                "Wide",
                "Wide",
                &dir,
                serde_json::json!({"CatalogNamespace": "ns-wide", "CatalogItemId": "wide-item"}),
            ),
        );
        let (provider, _) = provider_with_fake_epic(&env);
        assert!(provider.cover_candidates("Wide").is_empty());
        assert_eq!(
            urls(provider.background_candidates("Wide")),
            [
                "https://cdn.example/wide-box.jpg",
                "https://cdn.example/wide-offer.jpg"
            ]
        );
    }

    #[test]
    fn manifests_without_ids_make_no_requests() {
        let env = Env::new();
        let dir = env.game_dir("NoIds");
        env.manifest(
            "n.item",
            &item(
                "NoIds",
                "NoIds",
                &dir,
                serde_json::json!({"CatalogNamespace": "", "CatalogItemId": null}),
            ),
        );
        let (provider, http) = provider_with_fake_epic(&env);
        assert!(provider.cover_candidates("NoIds").is_empty());
        assert!(http.urls().is_empty());
    }

    #[test]
    fn network_failure_gives_no_candidates() {
        use crate::auth::testing::FakeHttp;
        let env = Env::new();
        let dir = env.game_dir("Sugar");
        env.manifest(
            "a.item",
            &item(
                "Sugar",
                "Sugar",
                &dir,
                serde_json::json!({"CatalogNamespace": "ns-sugar", "CatalogItemId": "sugar-item"}),
            ),
        );
        let provider =
            EpicProvider::with_data_root(env.root()).with_public(public::PublicCatalog::with(
                Arc::new(FakeHttp::new(|_| Err("offline".into()))),
                Arc::new(|| 10),
            ));
        assert!(provider.cover_candidates("Sugar").is_empty());
    }

    #[test]
    fn legendary_metadata_gives_catalog_ids() {
        let by_metadata = r#"{"app_name":"Sugar","metadata":{"id":"item-1","namespace":"ns-1"}}"#;
        assert_eq!(
            parse_legendary_ids(by_metadata),
            Some(("ns-1".into(), "item-1".into()))
        );
        let by_assets =
            r#"{"asset_infos":{"Windows":{"namespace":"ns-2","catalog_item_id":"item-2"}}}"#;
        assert_eq!(
            parse_legendary_ids(by_assets),
            Some(("ns-2".into(), "item-2".into()))
        );
        assert_eq!(parse_legendary_ids("{}"), None);
        assert_eq!(parse_legendary_ids("nope"), None);

        let tmp = tempfile::tempdir().unwrap();
        let meta = tmp
            .path()
            .join(".config/heroic/legendaryConfig/legendary/metadata");
        fs::create_dir_all(&meta).unwrap();
        fs::write(meta.join("Sugar.json"), by_metadata).unwrap();
        let paths = HeroicPaths::discover(tmp.path(), None, None);
        let provider = EpicProvider::with_heroic(paths.clone());
        assert_eq!(
            provider.catalog_ids("Sugar"),
            Some(("ns-1".into(), "item-1".into()))
        );
        assert!(legendary_ids(&paths, "../Sugar").is_none());
        assert!(legendary_ids(&paths, "Missing").is_none());
    }

    #[test]
    fn uninstall_opens_the_launcher_library() {
        let p = EpicProvider::default();
        assert!(p.can_uninstall());
        let action = p.uninstall_action("Sugar").unwrap();
        assert_eq!(action.target, "com.epicgames.launcher://store/library");
        assert_eq!(action.kind, crate::model::ActionKind::Uri);
        let tmp = tempfile::tempdir().unwrap();
        let heroic = EpicProvider::with_heroic(HeroicPaths::discover(tmp.path(), None, None));
        assert!(!heroic.can_uninstall());
        assert!(heroic.uninstall_action("Sugar").is_none());
    }

    /// A home with Heroic's Legendary (game `H`, also listed standalone as `Both`) and a
    /// standalone Legendary (games `S`, `Both`, `-flag`), plus a `legendary` executable if asked.
    fn legendary_home(with_bin: bool) -> (tempfile::TempDir, HeroicPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let installed = |ids: &[&str]| {
            let entries: serde_json::Map<String, serde_json::Value> = ids
                .iter()
                .map(|id| {
                    let dir = home.join("games").join(id.trim_start_matches('-'));
                    fs::create_dir_all(&dir).unwrap();
                    (
                        (*id).to_string(),
                        serde_json::json!({"app_name": id, "title": id, "install_path": dir,
                            "is_dlc": false, "platform": "Windows"}),
                    )
                })
                .collect();
            serde_json::Value::Object(entries).to_string()
        };
        let put = |path: &str, text: String| {
            let file = home.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        };
        put(
            ".config/heroic/legendaryConfig/legendary/installed.json",
            installed(&["H", "Both"]),
        );
        put(
            ".config/legendary/installed.json",
            installed(&["S", "Both", "-flag"]),
        );
        let path = if with_bin {
            put("bin/legendary", String::new());
            Some(std::env::join_paths([home.join("bin")]).unwrap())
        } else {
            None
        };
        let paths = HeroicPaths::discover(home, None, path.as_deref());
        (tmp, paths)
    }

    #[test]
    fn standalone_legendary_games_uninstall_through_legendary() {
        let (_tmp, paths) = legendary_home(true);
        let bin = paths.legendary_bin.clone().unwrap();
        let p = EpicProvider::with_heroic(paths);
        assert!(p.can_uninstall());
        let action = p.uninstall_action("S").unwrap();
        assert_eq!(action.kind, crate::model::ActionKind::File);
        assert_eq!(action.target, bin.display().to_string());
        assert_eq!(action.args.as_deref(), Some("uninstall S -y"));
        // Case-insensitive like the rest of the Epic ids.
        assert!(p.uninstall_action("s").is_some());
    }

    #[test]
    fn heroic_managed_and_unknown_games_have_no_uninstall_action() {
        let (_tmp, paths) = legendary_home(true);
        let p = EpicProvider::with_heroic(paths);
        assert!(p.uninstall_action("H").is_none(), "managed by Heroic");
        assert!(p.uninstall_action("Both").is_none(), "Heroic wins");
        assert!(p.uninstall_action("Missing").is_none());
        assert!(
            p.uninstall_action("-flag").is_none(),
            "would read as an option"
        );
        assert!(p.uninstall_action("a b").is_none());
        assert!(p.uninstall_action("").is_none());
    }

    #[test]
    fn standalone_legendary_needs_the_executable_to_uninstall() {
        let (_tmp, paths) = legendary_home(false);
        let p = EpicProvider::with_heroic(paths);
        assert!(!p.can_uninstall());
        assert!(p.uninstall_action("S").is_none());
        // Heroic alone (no standalone directory) cannot uninstall either.
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".config/heroic/legendaryConfig/legendary")).unwrap();
        let heroic_only = HeroicPaths {
            legendary_bin: Some(tmp.path().join("legendary")),
            ..HeroicPaths::discover(tmp.path(), None, None)
        };
        assert!(!EpicProvider::with_heroic(heroic_only).can_uninstall());
    }

    #[test]
    fn cache_dir_applies_in_either_order() {
        use crate::auth::MemoryTokenStore;
        let tmp = tempfile::tempdir().unwrap();
        let account = || Arc::new(EpicAccount::new(Arc::new(MemoryTokenStore::new())));
        // Smoke test: both orders build, and the folder is only created by writes.
        let a = EpicProvider::default()
            .with_cache_dir(tmp.path())
            .with_account(account());
        let b = EpicProvider::default()
            .with_account(account())
            .with_cache_dir(tmp.path());
        assert_eq!(a.cache_dir, b.cache_dir);
        assert!(!tmp.path().join("epic-catalog").exists());
        assert!(a.account.is_some() && b.account.is_some());
    }

    #[test]
    fn catalog_images_are_cached_on_disk_across_providers() {
        let env = Env::new();
        let dir = env.game_dir("Sugar");
        env.manifest(
            "a.item",
            &item(
                "Sugar",
                "Sugar Game",
                &dir,
                serde_json::json!({"CatalogNamespace": "ns-sugar", "CatalogItemId": "sugar-item"}),
            ),
        );
        let cache = tempfile::tempdir().unwrap();
        let (first, http) = provider_with_fake_epic(&env);
        let first = first.with_cache_dir(cache.path());
        assert!(!first.cover_candidates("Sugar").is_empty());
        let calls = http.urls().len();
        assert!(calls > 0);

        // "Restart": a new provider with no network at all still finds the covers.
        let second = EpicProvider::with_data_root(env.root())
            .with_public(public::PublicCatalog::with(
                Arc::new(crate::auth::testing::FakeHttp::new(|_| {
                    Err("offline".into())
                })),
                Arc::new(|| 10),
            ))
            .with_cache_dir(cache.path());
        assert!(!second.cover_candidates("Sugar").is_empty());
    }

    #[test]
    fn install_action_uri() {
        let action = EpicProvider::default().install_action("Sugar").unwrap();
        assert_eq!(
            action.target,
            "com.epicgames.launcher://apps/Sugar?action=install"
        );
        assert!(!EpicProvider::default().can_scan_owned());
        assert_eq!(
            install_uri("ns 1", "item1", "Sugar"),
            "com.epicgames.launcher://apps/ns%201%3Aitem1%3ASugar?action=install"
        );
    }
}
