//! Turns installed AppX/MSIX packages into games. Pure functions over [`AppxPackage`] values and
//! the file system, so everything is testable with fixture folders.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::fs;
use std::path::{Path, PathBuf};

use super::manifest::{self, AppxManifest, GameConfig};
use crate::model::{ImportedGame, PlayAction, TrackingMode};
use crate::platform::AppxPackage;

const GAME_CONFIG: &str = "MicrosoftGame.config";
const APPX_MANIFEST: &str = "AppxManifest.xml";
/// Application id used by GDK titles; also the fallback when the registry lists none.
const DEFAULT_APP_ID: &str = "Game";

/// A detected game plus the files its cover may come from.
#[derive(Debug, Clone, PartialEq)]
pub struct GameEntry {
    pub game: ImportedGame,
    /// Declared logo paths (not yet resolved against scale-qualified file names), best first.
    pub logos: Vec<PathBuf>,
}

/// Splits `<Name>_<Version>_<Arch>_<ResourceId>_<PublisherHash>`; returns the package family name
/// (`<Name>_<PublisherHash>`) and whether it is a resource package (non-empty ResourceId).
pub fn parse_full_name(full_name: &str) -> Option<(String, bool)> {
    let parts: Vec<&str> = full_name.split('_').collect();
    if parts.len() != 5 || parts[0].is_empty() || parts[4].is_empty() {
        return None;
    }
    Some((format!("{}_{}", parts[0], parts[4]), !parts[3].is_empty()))
}

fn read_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    // Manifests are UTF-8, config files sometimes carry a BOM.
    let text = String::from_utf8_lossy(&bytes);
    Some(text.trim_start_matches('\u{feff}').to_string())
}

/// Folder holding `MicrosoftGame.config`: the package root (GDK installs register the `Content`
/// folder as root) or its `Content` sub-folder.
fn config_dir(root: &Path) -> Option<PathBuf> {
    [root.to_path_buf(), root.join("Content")]
        .into_iter()
        .find(|dir| dir.join(GAME_CONFIG).is_file())
}

/// Reads one package; `None` when it is not an installed game.
pub fn game_from_package(pkg: &AppxPackage) -> Option<GameEntry> {
    let (family, is_resource) = parse_full_name(&pkg.full_name)?;
    if is_resource || !pkg.root.is_dir() {
        return None;
    }
    // Game filter: GDK titles ship a MicrosoftGame.config.
    let content_dir = config_dir(&pkg.root)?;
    let config: GameConfig = read_text(&content_dir.join(GAME_CONFIG))
        .map(|xml| manifest::parse_game_config(&xml))
        .unwrap_or_default();
    let appx: AppxManifest = read_text(&pkg.root.join(APPX_MANIFEST))
        .map(|xml| manifest::parse_appx_manifest(&xml))
        .unwrap_or_default();
    if appx.is_framework_or_resource {
        return None;
    }

    let app_id = [&pkg.app_ids, &appx.app_ids]
        .into_iter()
        .find(|ids| !ids.is_empty())
        .map(|ids| {
            ids.iter()
                .find(|id| id.eq_ignore_ascii_case(DEFAULT_APP_ID))
                .unwrap_or(&ids[0])
                .clone()
        })
        .unwrap_or_else(|| DEFAULT_APP_ID.to_string());

    let name = appx
        .display_name
        .clone()
        .or(config.display_name.clone())
        .or(appx.visual_display_name.clone())
        .or(appx.identity_name.clone())
        .or(config.identity_name.clone())
        .or_else(|| pkg.full_name.split('_').next().map(str::to_string))?;

    let install_dir = content_dir.to_string_lossy().into_owned();
    let mut game = ImportedGame::installed(family.clone(), name);
    game.install_dir = Some(install_dir.clone());
    game.play_action = Some(PlayAction {
        tracking: TrackingMode::Directory,
        tracking_path: Some(install_dir),
        ..PlayAction::uri(format!(r"shell:AppsFolder\{family}!{app_id}"))
    });

    let mut logos: Vec<PathBuf> = config
        .logos
        .iter()
        .map(|rel| content_dir.join(normalize_rel(rel)))
        .collect();
    logos.extend(
        appx.logos
            .iter()
            .map(|rel| pkg.root.join(normalize_rel(rel))),
    );
    Some(GameEntry { game, logos })
}

/// Declared paths use backslashes; make them safe relative paths (no drive or `..` escapes).
fn normalize_rel(rel: &str) -> PathBuf {
    rel.split(['\\', '/'])
        .filter(|s| !s.is_empty() && *s != "." && *s != ".." && !s.contains(':'))
        .collect()
}

/// All games among `packages`, one per family name, sorted by name.
pub fn scan_packages(packages: &[AppxPackage]) -> Vec<GameEntry> {
    let mut entries: Vec<GameEntry> = Vec::new();
    for pkg in packages {
        let Some(entry) = game_from_package(pkg) else {
            continue;
        };
        // Several versions of one package can be registered; keep the first.
        if entries
            .iter()
            .all(|e| e.game.source_game_id != entry.game.source_game_id)
        {
            entries.push(entry);
        }
    }
    entries.sort_by_cached_key(|e| (e.game.name.to_lowercase(), e.game.source_game_id.clone()));
    entries
}

/// Resolves a declared logo path to the best existing file: `Logo.png`, `Logo.scale-200.png` and
/// `Logo.targetsize-256.png` style variants are all considered and the largest wins.
pub fn resolve_logo(declared: &Path) -> Option<PathBuf> {
    let dir = declared.parent()?;
    let stem = declared.file_stem()?.to_string_lossy().to_lowercase();
    let ext = declared.extension()?.to_string_lossy().to_lowercase();
    let mut best: Option<(u32, PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if file_ext != ext {
            continue;
        }
        let Some(file_stem) = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()) else {
            continue;
        };
        let score = if file_stem == stem {
            Some(100)
        } else {
            file_stem
                .strip_prefix(&stem)
                .and_then(|rest| rest.strip_prefix('.'))
                .and_then(variant_size)
        };
        if let Some(score) = score
            && best.as_ref().is_none_or(|(s, _)| score > *s)
        {
            best = Some((score, path));
        }
    }
    best.map(|(_, p)| p)
}

/// Size of a `scale-200` / `targetsize-256` (optionally `_altform-unplated`) qualifier.
fn variant_size(qualifier: &str) -> Option<u32> {
    let rest = qualifier
        .strip_prefix("scale-")
        .or_else(|| qualifier.strip_prefix("targetsize-"))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    // Skip theme/contrast variants such as `scale-200_contrast-white`, which look wrong on cards.
    let tail = &rest[digits.len()..];
    if !(tail.is_empty() || tail.starts_with("_altform-unplated")) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ActionKind;

    const FULL: &str = "Contoso.Racer_1.2.3.0_x64__8wekyb3d8bbwe";

    fn package(root: &Path, full_name: &str, app_ids: &[&str]) -> AppxPackage {
        AppxPackage {
            full_name: full_name.into(),
            root: root.to_path_buf(),
            app_ids: app_ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn gdk_game(root: &Path, config: &str, manifest: Option<&str>) {
        fs::create_dir_all(root.join("Assets")).unwrap();
        fs::write(root.join(GAME_CONFIG), config).unwrap();
        if let Some(m) = manifest {
            fs::write(root.join(APPX_MANIFEST), m).unwrap();
        }
    }

    const CONFIG: &str = r#"<Game><Identity Name="Contoso.Racer"/>
        <ShellVisuals DefaultDisplayName="Racer" Square150x150Logo="Assets\Logo.png"/></Game>"#;

    #[test]
    fn derives_family_names() {
        assert_eq!(
            parse_full_name(FULL),
            Some(("Contoso.Racer_8wekyb3d8bbwe".into(), false))
        );
        assert_eq!(
            parse_full_name("Contoso.Racer_1.0.0.0_neutral_split.language-de_8wekyb3d8bbwe"),
            Some(("Contoso.Racer_8wekyb3d8bbwe".into(), true))
        );
        for bad in [
            "",
            "NoUnderscores",
            "a_b_c_d",
            "a_b_c_d_e_f",
            "_1_x__hash",
            "a_1_x__",
        ] {
            assert_eq!(parse_full_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn builds_game_from_gdk_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("XboxGames/Racer/Content");
        gdk_game(&root, CONFIG, None);
        let entry = game_from_package(&package(&root, FULL, &["Game"])).unwrap();
        let g = &entry.game;
        assert_eq!(g.source_game_id, "Contoso.Racer_8wekyb3d8bbwe");
        assert_eq!(g.name, "Racer");
        assert!(g.installed);
        let dir = root.to_string_lossy().into_owned();
        assert_eq!(g.install_dir.as_deref(), Some(dir.as_str()));
        let action = g.play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(
            action.target,
            r"shell:AppsFolder\Contoso.Racer_8wekyb3d8bbwe!Game"
        );
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(action.tracking_path.as_deref(), Some(dir.as_str()));
        assert_eq!(entry.logos, vec![root.join("Assets").join("Logo.png")]);
    }

    #[test]
    fn name_falls_back_from_resource_reference_to_config() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Content");
        let manifest = r#"<Package><Identity Name="Contoso.Racer"/>
            <Properties><DisplayName>ms-resource:AppName</DisplayName></Properties>
            <Applications><Application Id="Editor"/><Application Id="Game"/></Applications></Package>"#;
        gdk_game(&root, CONFIG, Some(manifest));
        let entry = game_from_package(&package(&root, FULL, &[])).unwrap();
        assert_eq!(entry.game.name, "Racer");
        // Application id comes from the manifest when the registry lists none; "Game" preferred.
        assert!(entry.game.play_action.unwrap().target.ends_with("!Game"));

        // No config name either: use the identity name.
        let config = r#"<Game><ShellVisuals DefaultDisplayName="ms-resource:Name"/></Game>"#;
        gdk_game(&root, config, Some(manifest));
        let entry = game_from_package(&package(&root, FULL, &["Only"])).unwrap();
        assert_eq!(entry.game.name, "Contoso.Racer");
        assert!(entry.game.play_action.unwrap().target.ends_with("!Only"));
    }

    #[test]
    fn config_in_content_subfolder_sets_install_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Pkg");
        gdk_game(&root.join("Content"), CONFIG, None);
        let entry = game_from_package(&package(&root, FULL, &["Game"])).unwrap();
        let expected = root.join("Content").to_string_lossy().into_owned();
        assert_eq!(entry.game.install_dir.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn filters_non_games() {
        let tmp = tempfile::tempdir().unwrap();
        // No MicrosoftGame.config: an ordinary app.
        let app = tmp.path().join("App");
        fs::create_dir_all(&app).unwrap();
        fs::write(
            app.join(APPX_MANIFEST),
            "<Package><Identity Name=\"A.B\"/></Package>",
        )
        .unwrap();
        assert!(game_from_package(&package(&app, FULL, &["App"])).is_none());
        // Missing root folder.
        assert!(game_from_package(&package(&tmp.path().join("gone"), FULL, &["Game"])).is_none());
        // Resource package and framework package, even with a config present.
        let game = tmp.path().join("Game");
        gdk_game(&game, CONFIG, None);
        assert!(
            game_from_package(&package(
                &game,
                "Contoso.Racer_1.0.0.0_x64_split.scale-100_8wekyb3d8bbwe",
                &["Game"]
            ))
            .is_none()
        );
        let fw = tmp.path().join("Fw");
        gdk_game(
            &fw,
            CONFIG,
            Some("<Package><Properties><Framework>true</Framework></Properties></Package>"),
        );
        assert!(game_from_package(&package(&fw, FULL, &["Game"])).is_none());
        // Unparsable package names.
        assert!(game_from_package(&package(&game, "garbage", &["Game"])).is_none());
    }

    #[test]
    fn malformed_manifests_do_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Content");
        gdk_game(
            &root,
            "<Game><Identity Name=",
            Some("\u{feff}<Package><Identity"),
        );
        let entry = game_from_package(&package(&root, FULL, &[])).unwrap();
        assert_eq!(
            entry.game.name, "Contoso.Racer",
            "falls back to the package name"
        );
        assert!(entry.game.play_action.unwrap().target.ends_with("!Game"));
        fs::write(root.join(APPX_MANIFEST), [0xff, 0xfe, 0x00, 0x3c]).unwrap();
        assert!(game_from_package(&package(&root, FULL, &[])).is_some());
    }

    #[test]
    fn scan_dedups_and_sorts() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        let c = tmp.path().join("c");
        gdk_game(&a, CONFIG, None);
        gdk_game(&b, CONFIG, None); // older/newer registration of the same family
        gdk_game(
            &c,
            r#"<Game><ShellVisuals DefaultDisplayName="alpha"/></Game>"#,
            None,
        );
        let packages = vec![
            package(&a, FULL, &["Game"]),
            package(&b, "Contoso.Racer_2.0.0.0_x64__8wekyb3d8bbwe", &["Game"]),
            package(&c, "Other.Alpha_1.0.0.0_x64__abc", &["Game"]),
        ];
        let names: Vec<_> = scan_packages(&packages)
            .into_iter()
            .map(|e| e.game.name)
            .collect();
        assert_eq!(names, vec!["alpha", "Racer"]);
    }

    #[test]
    fn picks_largest_logo_variant() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for f in [
            "Logo.scale-100.png",
            "Logo.scale-200.png",
            "Logo.scale-400_contrast-white.png",
            "Logo.targetsize-48.png",
            "Logo.scale-200.jpg",
            "LogoWide.scale-400.png",
            "Other.scale-400.png",
        ] {
            fs::write(dir.join(f), "x").unwrap();
        }
        assert_eq!(
            resolve_logo(&dir.join("Logo.png")),
            Some(dir.join("Logo.scale-200.png"))
        );
        // The exact file counts as scale 100 only.
        fs::write(dir.join("Logo.png"), "x").unwrap();
        assert_eq!(
            resolve_logo(&dir.join("Logo.png")),
            Some(dir.join("Logo.scale-200.png"))
        );
        assert_eq!(resolve_logo(&dir.join("Missing.png")), None);
        assert_eq!(resolve_logo(&dir.join("nodir").join("Logo.png")), None);
    }

    #[test]
    fn declared_paths_cannot_escape() {
        assert_eq!(
            normalize_rel(r"..\..\x\Logo.png"),
            PathBuf::from("x").join("Logo.png")
        );
        assert_eq!(normalize_rel(r"C:\evil.png"), PathBuf::from("evil.png"));
        assert_eq!(
            normalize_rel("Assets/Logo.png"),
            PathBuf::from("Assets").join("Logo.png")
        );
    }
}
