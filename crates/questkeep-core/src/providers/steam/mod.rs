//! Steam provider: finds installed games from the local `appmanifest_*.acf` files of every Steam
//! library folder. No network access and no Steam login are needed.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

mod scan;
pub mod vdf;

use std::path::{Path, PathBuf};

use super::{CoverCandidate, LibraryProvider, ProviderError};
use crate::model::ImportedGame;

pub const STEAM_SOURCE: &str = "steam";

/// Reads installed games from one or more Steam installation directories.
#[derive(Debug, Clone, Default)]
pub struct SteamProvider {
    roots: Vec<PathBuf>,
}

impl SteamProvider {
    /// Uses the Steam installations found on this machine.
    pub fn detect() -> Self {
        Self::with_roots(crate::platform::steam_install_dirs())
    }

    /// Uses the given Steam root directories (the ones containing `steamapps`).
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots }
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

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if self.roots.is_empty() {
            return Err(ProviderError::NotFound(
                "Steam installation not found".into(),
            ));
        }
        let mut games = scan::scan_roots(&self.roots);
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    /// Steam's own library cache first (no network), then the store CDN. Cached file layouts:
    /// `librarycache/<id>_library_600x900.jpg` (older clients) and
    /// `librarycache/<id>/library_600x900.jpg`, and `librarycache/<id>/<hash>/library_600x900.jpg`
    /// or `.../<hash>/library_capsule.jpg` (newer clients).
    fn cover_candidates(&self, app_id: &str) -> Vec<CoverCandidate> {
        if app_id.is_empty() || !app_id.chars().all(|c| c.is_ascii_digit()) {
            return Vec::new();
        }
        let mut candidates = Vec::new();
        for root in &self.roots {
            let cache = root.join("appcache").join("librarycache");
            candidates.push(CoverCandidate::File(
                cache.join(format!("{app_id}_library_600x900.jpg")),
            ));
            let app_dir = cache.join(app_id);
            let mut dirs = vec![app_dir.clone()];
            if let Ok(entries) = std::fs::read_dir(&app_dir) {
                dirs.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
            // Newest clients store the portrait capsule as `library_capsule.jpg`.
            for name in ["library_600x900.jpg", "library_capsule.jpg"] {
                for dir in &dirs {
                    candidates.push(CoverCandidate::File(dir.join(name)));
                }
            }
        }
        for url in [
            format!(
                "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_600x900_2x.jpg"
            ),
            format!(
                "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{app_id}/library_600x900.jpg"
            ),
            format!("https://steamcdn-a.akamaihd.net/steam/apps/{app_id}/library_600x900_2x.jpg"),
        ] {
            candidates.push(CoverCandidate::Url(url));
        }
        candidates
    }
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
            source_game_id: id.into(),
            name: name.into(),
            install_dir: Some(
                library
                    .join("steamapps")
                    .join("common")
                    .join(dir)
                    .to_string_lossy()
                    .into_owned(),
            ),
            play_action: Some(PlayAction::uri(format!("steam://rungameid/{id}"))),
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
    fn cover_candidates_prefer_local_cache_then_cdn() {
        let tmp = tempfile::tempdir().unwrap();
        let hashed = tmp.path().join("appcache/librarycache/123/abc");
        fs::create_dir_all(&hashed).unwrap();
        let provider = SteamProvider::with_roots(vec![tmp.path().to_path_buf()]);
        let candidates = provider.cover_candidates("123");
        let capsule = CoverCandidate::File(hashed.join("library_capsule.jpg"));
        let capsule_pos = candidates.iter().position(|c| *c == capsule).unwrap();
        let first_url = candidates
            .iter()
            .position(|c| matches!(c, CoverCandidate::Url(_)))
            .unwrap();
        assert!(capsule_pos < first_url);
        assert!(matches!(
            &candidates[0],
            CoverCandidate::File(p) if p.ends_with("123_library_600x900.jpg")
        ));
        assert!(provider.cover_candidates("../etc").is_empty());
    }
}
