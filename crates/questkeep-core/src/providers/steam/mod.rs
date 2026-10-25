//! Steam provider: finds installed games from the local `appmanifest_*.acf` files of every Steam
//! library folder (no network, no login). Owned games come from the Steam Web API, authorized by
//! a store sign-in (see [`account`]) or the user's own API key.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

mod account;
mod icon;
mod local;
mod scan;
pub mod vdf;
mod web;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::auth::AccountAuth;
use crate::model::{GameMetadata, ImportedGame, PlayAction};

pub use account::{CAPTURE_PREFIX, LOGIN_URL, SESSION_URL, SteamAccount};

pub const STEAM_SOURCE: &str = "steam";

/// Reads installed games from one or more Steam installation directories.
#[derive(Debug, Clone, Default)]
pub struct SteamProvider {
    roots: Vec<PathBuf>,
    /// Store sign-in for owned games, when the app supplies one.
    account: Option<Arc<SteamAccount>>,
}

impl SteamProvider {
    /// Uses the Steam installations found on this machine.
    pub fn detect() -> Self {
        Self::with_roots(crate::platform::steam_install_dirs())
    }

    /// Uses the given Steam root directories (the ones containing `steamapps`).
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            account: None,
        }
    }

    /// Enables the store sign-in for owned games.
    pub fn with_account(mut self, account: Arc<SteamAccount>) -> Self {
        self.account = Some(account);
        self
    }

    /// The Steam root directories this provider reads. The image cache looks into
    /// `<root>/appcache/librarycache`.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

impl LibraryProvider for SteamProvider {
    fn id(&self) -> &str {
        STEAM_SOURCE
    }

    fn name(&self) -> &str {
        "Steam"
    }

    fn icon_svg(&self) -> &'static str {
        crate::providers::icons::STEAM
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if self.roots.is_empty() {
            return Err(ProviderError::NotFound(
                "Steam installation not found".into(),
            ));
        }
        let mut games = scan::scan_roots(&self.roots);
        self.apply_local_stats(&mut games);
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    fn can_scan_owned(&self) -> bool {
        true
    }

    /// Owned games through the Steam Web API: with the store sign-in when signed in, else with
    /// `config` = `{ "apiKey": string, "steamId"?: string }` (without a `steamId` the most
    /// recent login of the local Steam client is used). A failing sign-in falls back to the key.
    fn scan_owned(&self, config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        if let Some(account) = self.account.as_deref().filter(|a| a.is_signed_in()) {
            match Self::owned_with_session(account) {
                Ok(games) => return Ok(games),
                Err(err) if config_text(config, "apiKey").is_some() => {
                    tracing::warn!(%err, "Steam sign-in failed, using the API key");
                }
                Err(err) => return Err(err),
            }
        }
        let (api_key, steam_id) = self.owned_credentials(config)?;
        web::fetch_owned(web::Credential::Key(&api_key), &steam_id).map_err(|e| match e {
            web::OwnedError::Rejected => {
                ProviderError::Network("The Steam Web API key was rejected.".into())
            }
            web::OwnedError::Other(err) => err,
        })
    }

    fn account(&self) -> Option<&dyn AccountAuth> {
        self.account.as_deref().map(|a| a as &dyn AccountAuth)
    }

    fn install_action(&self, app_id: &str) -> Option<PlayAction> {
        is_app_id(app_id).then(|| PlayAction::uri(format!("steam://install/{app_id}")))
    }

    fn can_uninstall(&self) -> bool {
        true
    }

    /// Steam's own uninstall dialog (`steam://uninstall/<appid>`), which asks for confirmation.
    fn uninstall_action(&self, app_id: &str) -> Option<PlayAction> {
        is_app_id(app_id).then(|| PlayAction::uri(format!("steam://uninstall/{app_id}")))
    }

    fn metadata(&self, app_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        web::fetch_metadata(app_id)
    }

    /// Steam's own library cache first (no network), then the store CDN. Cached file layouts:
    /// `librarycache/<id>_library_600x900.jpg` (older clients) and
    /// `librarycache/<id>/library_600x900.jpg`, and `librarycache/<id>/<hash>/library_600x900.jpg`
    /// or `.../<hash>/library_capsule.jpg` (newer clients). The CDN works for any app id, so
    /// owned games that are not installed get covers too.
    fn cover_candidates(&self, app_id: &str) -> Vec<ImageCandidate> {
        if !is_app_id(app_id) {
            return Vec::new();
        }
        // Newest clients store the portrait capsule as `library_capsule.jpg`.
        let mut candidates =
            self.cached_art(app_id, &["library_600x900.jpg", "library_capsule.jpg"]);
        candidates.extend(
            [
                format!(
                    "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_600x900_2x.jpg"
                ),
                format!(
                    "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_600x900.jpg"
                ),
                format!(
                    "https://steamcdn-a.akamaihd.net/steam/apps/{app_id}/library_600x900_2x.jpg"
                ),
            ]
            .map(ImageCandidate::Url),
        );
        candidates
    }

    /// The small square icon from Steam's local library cache (see [`icon`]). There is no CDN
    /// fallback: the icon's file name is a hash that only the client knows.
    fn icon_candidates(&self, app_id: &str) -> Vec<ImageCandidate> {
        if !is_app_id(app_id) {
            return Vec::new();
        }
        self.roots
            .iter()
            .flat_map(|root| {
                icon::icon_candidates(&root.join("appcache").join("librarycache"), app_id)
            })
            .map(ImageCandidate::File)
            .collect()
    }

    /// Wide hero art: the local library cache, then the CDN; the small store header only last.
    fn background_candidates(&self, app_id: &str) -> Vec<ImageCandidate> {
        if !is_app_id(app_id) {
            return Vec::new();
        }
        let mut candidates = self.cached_art(app_id, &["library_hero.jpg"]);
        candidates.extend(
            [
                format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/library_hero.jpg"),
                format!(
                    "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_hero.jpg"
                ),
                format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/header.jpg"),
            ]
            .map(ImageCandidate::Url),
        );
        candidates
    }
}

impl SteamProvider {
    /// Local library cache files for `app_id`, in every layout Steam has used: the flat
    /// `<id>_<legacy>` file, `<id>/<name>`, and `<id>/<hash>/<name>`. `names` are tried in order
    /// within each layout step, e.g. `["library_600x900.jpg", "library_capsule.jpg"]`.
    fn cached_art(&self, app_id: &str, names: &[&str]) -> Vec<ImageCandidate> {
        let mut candidates = Vec::new();
        for root in &self.roots {
            let cache = root.join("appcache").join("librarycache");
            candidates.push(ImageCandidate::File(
                cache.join(format!("{app_id}_{}", names[0])),
            ));
            let app_dir = cache.join(app_id);
            let mut dirs = vec![app_dir.clone()];
            if let Ok(entries) = std::fs::read_dir(&app_dir) {
                let mut hashed: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect();
                hashed.sort();
                dirs.extend(hashed);
            }
            for name in names {
                for dir in &dirs {
                    candidates.push(ImageCandidate::File(dir.join(name)));
                }
            }
        }
        candidates
    }

    /// Fills playtime and last-played from the local Steam config of the most recent login.
    fn apply_local_stats(&self, games: &mut [ImportedGame]) {
        let stats = self.roots.iter().find_map(|root| {
            let stats = local::local_stats(root);
            (!stats.is_empty()).then_some(stats)
        });
        let Some(stats) = stats else { return };
        for game in games {
            if let Some(s) = stats.get(&game.source_game_id) {
                game.playtime_seconds = Some(s.playtime_seconds);
                game.last_played = s.last_played;
            }
        }
    }

    /// API key and SteamID64 for [`scan_owned`](LibraryProvider::scan_owned).
    fn owned_credentials(
        &self,
        config: &serde_json::Value,
    ) -> Result<(String, String), ProviderError> {
        let api_key = config_text(config, "apiKey").ok_or_else(|| {
            ProviderError::NotConfigured(
                "Sign in to Steam or add a Steam Web API key in Settings to import games you own but haven't installed."
                    .into(),
            )
        })?;
        let steam_id = config_text(config, "steamId")
            .or_else(|| {
                self.roots
                    .iter()
                    .find_map(|r| local::most_recent_steam_id(r))
            })
            .ok_or_else(|| {
                ProviderError::NotConfigured(
                    "Could not detect your Steam account. Enter your SteamID64 in Settings.".into(),
                )
            })?;
        Ok((api_key, steam_id))
    }
}

impl SteamProvider {
    /// Owned games with the store session's token; a rejected token is renewed once.
    fn owned_with_session(account: &SteamAccount) -> Result<Vec<ImportedGame>, ProviderError> {
        let (steam_id, token) = account.web_token(false)?;
        match web::fetch_owned(web::Credential::Token(&token), &steam_id) {
            Err(web::OwnedError::Rejected) => {
                let (steam_id, token) = account.web_token(true)?;
                web::fetch_owned(web::Credential::Token(&token), &steam_id).map_err(|e| match e {
                    web::OwnedError::Rejected => ProviderError::NotConfigured(
                        "Steam did not accept the sign-in. Sign in to Steam again in Settings."
                            .into(),
                    ),
                    web::OwnedError::Other(err) => err,
                })
            }
            Err(web::OwnedError::Other(err)) => Err(err),
            Ok(games) => Ok(games),
        }
    }
}

/// A trimmed, non-empty string setting.
fn config_text(config: &serde_json::Value, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn is_app_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}
/// Library folders (including the root itself) of one Steam installation.
pub fn library_folders(root: &Path) -> Vec<PathBuf> {
    scan::library_folders(root)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::{ActionKind, PlayAction};

    const LIBRARYFOLDERS: &str = include_str!("../../../tests/fixtures/steam/libraryfolders.vdf");
    const LIBRARYFOLDERS_OLD: &str =
        include_str!("../../../tests/fixtures/steam/libraryfolders_old.vdf");

    /// Fixture manifests by app id.
    fn manifest(id: &str) -> &'static str {
        match id {
            "620" => include_str!("../../../tests/fixtures/steam/appmanifest_620.acf"),
            "1245620" => include_str!("../../../tests/fixtures/steam/appmanifest_1245620.acf"),
            "105600" => include_str!("../../../tests/fixtures/steam/appmanifest_105600.acf"),
            "228980" => include_str!("../../../tests/fixtures/steam/appmanifest_228980.acf"),
            "2348590" => include_str!("../../../tests/fixtures/steam/appmanifest_2348590.acf"),
            "1145350" => include_str!("../../../tests/fixtures/steam/appmanifest_1145350.acf"),
            "413150" => include_str!("../../../tests/fixtures/steam/appmanifest_413150.acf"),
            "1091500" => include_str!("../../../tests/fixtures/steam/appmanifest_1091500.acf"),
            "999999" => include_str!("../../../tests/fixtures/steam/appmanifest_999999.acf"),
            other => panic!("no fixture for {other}"),
        }
    }

    struct Layout {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        lib2: PathBuf,
    }

    fn add_manifest(library: &Path, id: &str) {
        let steamapps = library.join("steamapps");
        fs::create_dir_all(&steamapps).unwrap();
        fs::write(
            steamapps.join(format!("appmanifest_{id}.acf")),
            manifest(id),
        )
        .unwrap();
    }

    fn add_dir(library: &Path, sub: &str, name: &str) {
        fs::create_dir_all(library.join("steamapps").join(sub).join(name)).unwrap();
    }

    fn substitute(template: &str, root: &Path, lib2: &Path, missing: &Path) -> String {
        let escape = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
        template
            .replace("{{ROOT}}", &escape(root))
            .replace("{{LIB2}}", &escape(lib2))
            .replace("{{MISSING}}", &escape(missing))
    }

    /// Steam root with the fixtures below plus a second library folder:
    /// root: 620, 228980, 2348590, 1145350 (soundtrack), 413150, 1091500, 999999 (malformed);
    /// lib2: 620 (duplicate), 1245620 (update pending), 105600 (partial download).
    fn layout(libraryfolders: &str) -> Layout {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Steam");
        let lib2 = tmp.path().join("SteamLibrary");
        let missing = tmp.path().join("gone");

        for id in [
            "620", "228980", "2348590", "1145350", "413150", "1091500", "999999",
        ] {
            add_manifest(&root, id);
        }
        add_dir(&root, "common", "Portal 2");
        add_dir(&root, "common", "Steamworks Shared");
        add_dir(&root, "common", "Proton 8.0");
        add_dir(&root, "music", "Hades Soundtrack");
        add_dir(&root, "common", "Stardew Valley");
        add_dir(&root, "common", "Cyberpunk 2077");
        // Leftover temp file of a Steam update must be ignored.
        fs::write(root.join("steamapps/appmanifest_620.acf.tmp"), "garbage").unwrap();

        for id in ["620", "1245620", "105600"] {
            add_manifest(&lib2, id);
        }
        add_dir(&lib2, "common", "Portal 2");
        add_dir(&lib2, "common", "ELDEN RING");
        add_dir(&lib2, "common", "Terraria");

        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            substitute(libraryfolders, &root, &lib2, &missing),
        )
        .unwrap();
        Layout {
            _tmp: tmp,
            root,
            lib2,
        }
    }

    fn expected(library: &Path, id: &str, name: &str, dir: &str) -> ImportedGame {
        ImportedGame {
            install_dir: Some(
                library
                    .join("steamapps")
                    .join("common")
                    .join(dir)
                    .to_string_lossy()
                    .into_owned(),
            ),
            play_action: Some(PlayAction::uri(format!("steam://rungameid/{id}"))),
            ..ImportedGame::installed(id, name)
        }
    }

    fn expected_games(l: &Layout) -> Vec<ImportedGame> {
        vec![
            expected(&l.root, "1091500", "Cyberpunk 2077", "Cyberpunk 2077"),
            expected(&l.lib2, "1245620", "ELDEN RING", "ELDEN RING"),
            expected(&l.root, "620", "Portal 2", "Portal 2"),
            expected(&l.root, "413150", "Stardew Valley", "Stardew Valley"),
        ]
    }

    #[test]
    fn scans_new_format_libraries_with_all_skip_rules() {
        let l = layout(LIBRARYFOLDERS);
        let games = SteamProvider::with_roots(vec![l.root.clone()])
            .scan_installed()
            .unwrap();
        assert_eq!(games, expected_games(&l));
    }

    #[test]
    fn scans_old_format_libraries() {
        let l = layout(LIBRARYFOLDERS_OLD);
        let games = SteamProvider::with_roots(vec![l.root.clone()])
            .scan_installed()
            .unwrap();
        assert_eq!(games, expected_games(&l));
    }

    #[test]
    fn library_folders_dedupe_and_drop_missing() {
        let l = layout(LIBRARYFOLDERS);
        // The listed root path does not exist in the temp layout, lib2 does.
        assert_eq!(
            library_folders(&l.root),
            vec![l.root.clone(), l.lib2.clone()]
        );

        // Listing the same library twice must not duplicate it.
        let doubled = format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" \"{}\" }}",
            "{{LIB2}}", "{{LIB2}}"
        );
        let l = layout(&doubled);
        assert_eq!(
            library_folders(&l.root),
            vec![l.root.clone(), l.lib2.clone()]
        );
    }

    #[test]
    fn falls_back_to_config_libraryfolders() {
        let l = layout(LIBRARYFOLDERS);
        let config = l.root.join("config");
        fs::create_dir_all(&config).unwrap();
        fs::rename(
            l.root.join("steamapps/libraryfolders.vdf"),
            config.join("libraryfolders.vdf"),
        )
        .unwrap();
        assert_eq!(
            library_folders(&l.root),
            vec![l.root.clone(), l.lib2.clone()]
        );
    }

    #[test]
    fn unreadable_libraryfolders_falls_back_to_root_only() {
        let l = layout("\"libraryfolders\" { \"0\" {");
        let games = SteamProvider::with_roots(vec![l.root.clone()])
            .scan_installed()
            .unwrap();
        let ids: Vec<_> = games.iter().map(|g| g.source_game_id.as_str()).collect();
        assert_eq!(ids, ["1091500", "620", "413150"]);
    }

    #[test]
    fn dedupes_apps_across_roots_first_wins() {
        let l = layout(LIBRARYFOLDERS);
        // lib2 doubles as a second root; its Portal 2 copy must not replace the root's.
        let games = SteamProvider::with_roots(vec![l.root.clone(), l.lib2.clone()])
            .scan_installed()
            .unwrap();
        assert_eq!(games, expected_games(&l));
    }

    #[test]
    fn manifest_without_install_dir_on_disk_is_skipped() {
        let l = layout(LIBRARYFOLDERS);
        fs::remove_dir_all(l.root.join("steamapps/common/Portal 2")).unwrap();
        fs::remove_dir_all(l.lib2.join("steamapps/common/Portal 2")).unwrap();
        let games = SteamProvider::with_roots(vec![l.root.clone()])
            .scan_installed()
            .unwrap();
        assert!(games.iter().all(|g| g.source_game_id != "620"));
        assert_eq!(games.len(), 3);
    }

    #[test]
    fn tool_app_ids_are_skipped() {
        let l = layout(LIBRARYFOLDERS);
        let sniper = manifest("2348590")
            .replace("2348590", "1628350")
            .replace("Proton 8.0", "Some Tool");
        fs::write(l.root.join("steamapps/appmanifest_1628350.acf"), sniper).unwrap();
        add_dir(&l.root, "common", "Some Tool");
        let games = SteamProvider::with_roots(vec![l.root.clone()])
            .scan_installed()
            .unwrap();
        assert!(games.iter().all(|g| g.source_game_id != "1628350"));
    }

    #[test]
    fn missing_steam_is_not_found() {
        let err = SteamProvider::with_roots(vec![])
            .scan_installed()
            .unwrap_err();
        assert!(matches!(&err, ProviderError::NotFound(m) if m == "Steam installation not found"));
    }

    #[test]
    fn provider_metadata() {
        let p = SteamProvider::with_roots(vec![PathBuf::from("x")]);
        assert_eq!(p.id(), STEAM_SOURCE);
        assert_eq!(p.name(), "Steam");
        assert_eq!(p.roots(), [PathBuf::from("x")]);
        let game = &expected_games(&layout(LIBRARYFOLDERS))[0];
        assert_eq!(game.play_action.as_ref().unwrap().kind, ActionKind::Uri);
    }

    #[test]
    fn icon_candidates_come_from_the_local_cache_only() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("appcache/librarycache/123");
        fs::create_dir_all(&app).unwrap();
        let hash = "0123456789abcdef0123456789abcdef01234567";
        image::RgbImage::new(32, 32)
            .save_with_format(app.join(format!("{hash}.jpg")), image::ImageFormat::Jpeg)
            .unwrap();
        let provider = SteamProvider::with_roots(vec![tmp.path().to_path_buf()]);
        let candidates = provider.icon_candidates("123");
        assert!(
            candidates
                .iter()
                .all(|c| matches!(c, ImageCandidate::File(_)))
        );
        assert!(candidates.contains(&ImageCandidate::File(app.join(format!("{hash}.jpg")))));
        assert!(provider.icon_candidates("../etc").is_empty());
        assert!(
            SteamProvider::with_roots(vec![])
                .icon_candidates("123")
                .is_empty()
        );
    }

    #[test]
    fn cover_candidates_prefer_local_cache_then_cdn() {
        let tmp = tempfile::tempdir().unwrap();
        let hashed = tmp.path().join("appcache/librarycache/123/abc");
        fs::create_dir_all(&hashed).unwrap();
        let provider = SteamProvider::with_roots(vec![tmp.path().to_path_buf()]);
        let candidates = provider.cover_candidates("123");
        let capsule = ImageCandidate::File(hashed.join("library_capsule.jpg"));
        let capsule_pos = candidates.iter().position(|c| *c == capsule).unwrap();
        let first_url = candidates
            .iter()
            .position(|c| matches!(c, ImageCandidate::Url(_)))
            .unwrap();
        assert!(capsule_pos < first_url);
        assert!(matches!(
            &candidates[0],
            ImageCandidate::File(p) if p.ends_with("123_library_600x900.jpg")
        ));
        assert!(provider.cover_candidates("../etc").is_empty());
    }

    #[test]
    fn installed_games_get_local_playtime() {
        let l = layout(LIBRARYFOLDERS);
        let root = &l.root;
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(
            root.join("config/loginusers.vdf"),
            r#""users" {
                "76561197960265730" { "MostRecent" "0" "Timestamp" "5" }
                "76561197960265729" { "MostRecent" "1" "Timestamp" "1" }
            }"#,
        )
        .unwrap();
        let config = root.join("userdata/1/config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("localconfig.vdf"),
            r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" { "Apps" {
                "620" { "Playtime" "30" "LastPlayed" "1700000000" }
                "99" { "Playtime" "5" }
            } } } } }"#,
        )
        .unwrap();
        let games = SteamProvider::with_roots(vec![root.clone()])
            .scan_installed()
            .unwrap();
        let portal = games.iter().find(|g| g.source_game_id == "620").unwrap();
        assert_eq!(portal.playtime_seconds, Some(1800));
        assert_eq!(portal.last_played.unwrap().timestamp(), 1_700_000_000);
        let other = games.iter().find(|g| g.source_game_id == "413150").unwrap();
        assert_eq!(other.playtime_seconds, None);
        assert_eq!(games.len(), 4);
    }

    #[test]
    fn owned_scan_requires_configuration() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = SteamProvider::with_roots(vec![tmp.path().to_path_buf()]);
        assert!(provider.can_scan_owned());
        for config in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({ "apiKey": "   " }),
        ] {
            let err = provider.scan_owned(&config).unwrap_err();
            assert!(
                matches!(&err, ProviderError::NotConfigured(m) if m.contains("Web API key")),
                "{err}"
            );
        }
        // Key but no Steam id and nothing to detect it from.
        let err = provider
            .scan_owned(&serde_json::json!({ "apiKey": "k" }))
            .unwrap_err();
        assert!(matches!(&err, ProviderError::NotConfigured(m) if m.contains("SteamID64")));
    }

    #[test]
    fn owned_credentials_prefer_config_then_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("config")).unwrap();
        fs::write(
            tmp.path().join("config/loginusers.vdf"),
            r#""users" { "76561197960265729" { "MostRecent" "1" } }"#,
        )
        .unwrap();
        let provider = SteamProvider::with_roots(vec![tmp.path().to_path_buf()]);
        let detected = provider
            .owned_credentials(&serde_json::json!({ "apiKey": " k " }))
            .unwrap();
        assert_eq!(detected, ("k".to_string(), "76561197960265729".to_string()));
        let explicit = provider
            .owned_credentials(&serde_json::json!({ "apiKey": "k", "steamId": "42" }))
            .unwrap();
        assert_eq!(explicit.1, "42");
    }

    #[test]
    fn uninstall_action_is_a_steam_uri() {
        let p = SteamProvider::default();
        assert!(p.can_uninstall());
        assert_eq!(
            p.uninstall_action("440"),
            Some(PlayAction::uri("steam://uninstall/440"))
        );
        assert_eq!(p.uninstall_action("../x"), None);
        assert_eq!(p.uninstall_action(""), None);
    }

    #[test]
    fn install_action_is_a_steam_uri() {
        let p = SteamProvider::default();
        assert_eq!(
            p.install_action("440"),
            Some(PlayAction::uri("steam://install/440"))
        );
        assert_eq!(p.install_action("../x"), None);
        assert_eq!(p.install_action(""), None);
    }

    #[test]
    fn background_candidates_prefer_local_cache_then_cdn_with_header_last() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("appcache/librarycache");
        let hashed = cache.join("123/abc");
        fs::create_dir_all(&hashed).unwrap();
        let provider = SteamProvider::with_roots(vec![tmp.path().to_path_buf()]);
        let c = provider.background_candidates("123");

        let files: Vec<&ImageCandidate> = c
            .iter()
            .filter(|c| matches!(c, ImageCandidate::File(_)))
            .collect();
        assert_eq!(
            files,
            [
                &ImageCandidate::File(cache.join("123_library_hero.jpg")),
                &ImageCandidate::File(cache.join("123").join("library_hero.jpg")),
                &ImageCandidate::File(hashed.join("library_hero.jpg")),
            ]
        );
        let first_url = c
            .iter()
            .position(|c| matches!(c, ImageCandidate::Url(_)))
            .unwrap();
        assert_eq!(first_url, 3, "files come before any URL");
        assert!(matches!(
            &c[first_url],
            ImageCandidate::Url(u) if u == "https://cdn.cloudflare.steamstatic.com/steam/apps/123/library_hero.jpg"
        ));
        assert!(matches!(
            c.last().unwrap(),
            ImageCandidate::Url(u) if u.ends_with("/123/header.jpg")
        ));
        assert!(c.iter().all(|c| !matches!(
            c,
            ImageCandidate::Url(u) if u.contains("page_bg_generated")
        )));
        assert!(provider.background_candidates("x1").is_empty());
    }

    #[test]
    fn covers_work_for_apps_that_are_not_installed() {
        let provider = SteamProvider::with_roots(vec![]);
        let c = provider.cover_candidates("730");
        assert!(matches!(
            &c[0],
            ImageCandidate::Url(u) if u.contains("/730/library_600x900_2x.jpg")
        ));
    }

    /// Prints this machine's installed games with local playtime. No network.
    /// Run with `cargo test -p questkeep-core -- --ignored --nocapture steam_local_playtime`.
    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn steam_local_playtime_sanity_check() {
        let provider = SteamProvider::detect();
        let games = provider.scan_installed().unwrap();
        let with_playtime = games
            .iter()
            .filter(|g| g.playtime_seconds.is_some_and(|s| s > 0))
            .count();
        for g in &games {
            println!(
                "{:>10}  {:<40} {:>6} min  last played: {}",
                g.source_game_id,
                g.name,
                g.playtime_seconds.unwrap_or(0) / 60,
                g.last_played
                    .map_or("-".to_string(), |t| t.format("%Y-%m-%d").to_string())
            );
        }
        println!("installed: {}, with playtime: {with_playtime}", games.len());
    }
}
