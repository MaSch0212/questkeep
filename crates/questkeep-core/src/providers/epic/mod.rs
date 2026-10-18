//! Epic Games Store provider: finds installed games from the Epic Games Launcher's local data
//! (`LauncherInstalled.dat` and the `*.item` manifests). No network access and no Epic login.
//!
//! Signing in to an Epic account (`account`, optional) adds the owned-but-not-installed games,
//! playtime, covers, backgrounds and metadata from Epic's library and catalog services. Without
//! a session there is no network access and no covers or metadata: the manifests only carry a
//! `VaultThumbnailUrl` for a few legacy entries, and scraping the launcher's web cache is not
//! acceptable.
//!
//! On Linux the games come from Heroic Games Launcher / Legendary (`super::heroic`), with the same
//! `AppName` ids as on Windows, and launch through Heroic's `heroic://launch/...` URI.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use super::heroic::{self, HeroicPaths};
use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::auth::AccountAuth;
use crate::model::{GameMetadata, ImportedGame, PlayAction};

mod account;
mod web;

pub use account::{EpicAccount, REDIRECT_URL as CODE_REDIRECT_URL};

pub const EPIC_SOURCE: &str = "epic";

/// Reads installed games from an Epic launcher data directory (the `Epic` folder under
/// `%ProgramData%`, containing `UnrealEngineLauncher` and `EpicGamesLauncher`).
#[derive(Debug, Clone, Default)]
pub struct EpicProvider {
    data_root: Option<PathBuf>,
    /// Heroic/Legendary directories (Linux).
    heroic: Option<HeroicPaths>,
    /// Signed-in account, when the app supplies token storage.
    account: Option<Arc<EpicAccount>>,
}

impl EpicProvider {
    /// Uses the launcher data of this machine (Epic Games Launcher on Windows, Heroic/Legendary
    /// elsewhere).
    pub fn detect() -> Self {
        Self {
            data_root: default_data_root(),
            heroic: default_heroic(),
            account: None,
        }
    }

    /// Enables account sign-in, the owned-games list, covers and metadata.
    pub fn with_account(mut self, account: Arc<EpicAccount>) -> Self {
        self.account = Some(account);
        self
    }

    /// Uses the given launcher data directory.
    pub fn with_data_root(root: impl Into<PathBuf>) -> Self {
        Self {
            data_root: Some(root.into()),
            heroic: None,
            account: None,
        }
    }

    /// Reads Heroic/Legendary data from the given directories (any platform; tests and tools).
    pub fn with_heroic(paths: HeroicPaths) -> Self {
        Self {
            data_root: None,
            heroic: Some(paths),
            account: None,
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
        let Some(account) = &self.account else {
            return Vec::new();
        };
        match account.catalog_item(source_game_id) {
            Ok(Some(item)) => pick(&item).into_iter().map(ImageCandidate::Url).collect(),
            Ok(None) => Vec::new(),
            Err(err) => {
                tracing::debug!(game = source_game_id, %err, "Epic image lookup failed");
                Vec::new()
            }
        }
    }
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
}

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
