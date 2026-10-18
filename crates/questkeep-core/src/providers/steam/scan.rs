//! Local Steam scanning: library folder discovery and `appmanifest_*.acf` parsing.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::path::{Component, Path, PathBuf};

use super::vdf::{self, Vdf};
use crate::model::{ImportedGame, PlayAction};

/// `StateFlags` bit set once an app is completely installed (other bits, e.g. 2 = update
/// required, may be set at the same time and are fine).
const STATE_FULLY_INSTALLED: u64 = 4;

/// Steamworks Common Redistributables: has a manifest but is not a game.
const REDISTRIBUTABLES_APP_ID: &str = "228980";

/// Steam runtimes and tools that have local manifests but are not games. Extend these lists when
/// new tools show up in users' libraries.
///
/// Known tool app ids: Steam Linux Runtime scout (1070560), soldier (1391110), sniper (1628350)
/// and SteamVR (250820).
const TOOL_APP_IDS: &[&str] = &["1070560", "1391110", "1628350", "250820"];

/// Tool names, matched by prefix (`Proton 8.0`, `Steam Linux Runtime 3.0 (sniper)`, ...).
const TOOL_NAME_PREFIXES: &[&str] = &[
    "Proton ",
    "Steam Linux Runtime",
    "Steamworks Common Redistributables",
];

/// Tool names that must match exactly.
const TOOL_NAMES: &[&str] = &["Proton Experimental"];

/// Reads every game installed in the given Steam roots (and the library folders they list).
/// Results are deduplicated by app id (first wins) and keep discovery order.
pub fn scan_roots(roots: &[PathBuf]) -> Vec<ImportedGame> {
    let mut libraries: Vec<PathBuf> = Vec::new();
    for root in roots {
        for library in library_folders(root) {
            push_unique_dir(&mut libraries, library);
        }
    }

    let mut games: Vec<ImportedGame> = Vec::new();
    for library in &libraries {
        for game in scan_library(library) {
            if !games
                .iter()
                .any(|g| g.source_game_id == game.source_game_id)
            {
                games.push(game);
            }
        }
    }
    games
}

/// Library folders of one Steam root: the root itself first, then the folders listed in
/// `steamapps/libraryfolders.vdf` (falling back to `config/libraryfolders.vdf`). Only existing
/// directories are returned, without duplicates.
///
/// A libraryfolders file that is missing, unreadable or malformed is logged and ignored: the
/// root alone is still a valid library, so the scan continues with it.
pub fn library_folders(root: &Path) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    push_unique_dir(&mut libraries, root.to_path_buf());

    let candidates = [
        root.join("steamapps").join("libraryfolders.vdf"),
        root.join("config").join("libraryfolders.vdf"),
    ];
    let Some(file) = candidates.iter().find(|p| p.is_file()) else {
        return libraries;
    };
    let text = match std::fs::read(file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) => {
            tracing::warn!("could not read {}: {e}", file.display());
            return libraries;
        }
    };
    match vdf::parse(&text) {
        Ok((_, folders)) => {
            for path in library_paths(&folders) {
                push_unique_dir(&mut libraries, PathBuf::from(path));
            }
        }
        Err(e) => tracing::warn!("could not parse {}: {e}", file.display()),
    }
    libraries
}

/// Extracts the library paths from a parsed libraryfolders document. Supports the old format
/// (`"1" "D:\\Lib"`) and the new one (`"1" { "path" "D:\\Lib" ... }`); only numeric keys count.
fn library_paths(folders: &Vdf) -> Vec<String> {
    folders
        .entries()
        .iter()
        .filter(|(key, _)| !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|(_, value)| match value {
            Vdf::Str(path) => Some(path.clone()),
            Vdf::Obj(_) => value.get_str("path").map(str::to_string),
        })
        .filter(|path| !path.trim().is_empty())
        .collect()
}

/// Adds `dir` if it is an existing directory that is not yet in `list` (compared canonicalized
/// where possible, case-insensitively on Windows).
fn push_unique_dir(list: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dir.is_dir() {
        return;
    }
    let key = dedupe_key(&dir);
    if !list.iter().any(|existing| dedupe_key(existing) == key) {
        list.push(dir);
    }
}

fn dedupe_key(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Reads all installed games of one library folder. Bad manifests are logged and skipped.
fn scan_library(library: &Path) -> Vec<ImportedGame> {
    let steamapps = library.join("steamapps");
    let entries = match std::fs::read_dir(&steamapps) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("could not list {}: {e}", steamapps.display());
            return Vec::new();
        }
    };

    let mut manifests: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            name.starts_with("appmanifest_") && name.ends_with(".acf") && path.is_file()
        })
        .collect();
    // Deterministic order, independent of the file system.
    manifests.sort();

    manifests
        .iter()
        .filter_map(|path| match read_manifest(&steamapps, path) {
            Ok(game) => game,
            Err(message) => {
                tracing::warn!("skipping {}: {message}", path.display());
                None
            }
        })
        .collect()
}

/// `Ok(None)` means a valid manifest that is intentionally not a game.
fn read_manifest(steamapps: &Path, path: &Path) -> Result<Option<ImportedGame>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let (_, state) = vdf::parse(&text).map_err(|e| e.to_string())?;
    Ok(game_from_manifest(steamapps, &state))
}

/// Applies the skip rules to a parsed `AppState` object.
fn game_from_manifest(steamapps: &Path, state: &Vdf) -> Option<ImportedGame> {
    let flags: u64 = state.get_str("StateFlags")?.trim().parse().ok()?;
    if flags & STATE_FULLY_INSTALLED == 0 {
        return None;
    }

    let app_id = state.get_str("appid")?.trim();
    if app_id.is_empty() || app_id == REDISTRIBUTABLES_APP_ID || TOOL_APP_IDS.contains(&app_id) {
        return None;
    }

    let raw_name = state
        .get_str("name")
        .filter(|n| !n.trim().is_empty())
        .or_else(|| state.get("UserConfig").and_then(|u| u.get_str("name")))?;
    let name = clean_name(raw_name);
    if name.is_empty() || is_tool_name(&name) {
        return None;
    }

    let install_name = state.get_str("installdir")?.trim();
    if !is_plain_relative(install_name) {
        return None;
    }
    // Soundtracks live in `steamapps/music/<installdir>`; anything not under `common` is not a
    // game (or is not actually installed).
    let install_dir = steamapps.join("common").join(install_name);
    if !install_dir.is_dir() {
        return None;
    }

    Some(ImportedGame {
        install_dir: Some(install_dir.to_string_lossy().into_owned()),
        play_action: Some(PlayAction::uri(format!("steam://rungameid/{app_id}"))),
        ..ImportedGame::installed(app_id, name)
    })
}

/// A non-empty relative path without `..` or a root, so a manifest cannot point outside `common`.
fn is_plain_relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// Whether an app is a tool/runtime rather than a game, by id or name.
pub(super) fn is_tool_app(app_id: &str, name: &str) -> bool {
    app_id == REDISTRIBUTABLES_APP_ID || TOOL_APP_IDS.contains(&app_id) || is_tool_name(name)
}

fn is_tool_name(name: &str) -> bool {
    TOOL_NAMES.contains(&name) || TOOL_NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Trims, removes trademark/registered/copyright signs and collapses runs of whitespace.
pub fn clean_name(name: &str) -> String {
    let without_marks: String = name
        .chars()
        .filter(|c| !matches!(c, '\u{2122}' | '\u{00ae}' | '\u{00a9}'))
        .collect();
    without_marks
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_name_strips_marks_and_whitespace() {
        assert_eq!(
            clean_name("  Cyberpunk\u{2122}   2077\u{ae} "),
            "Cyberpunk 2077"
        );
        assert_eq!(clean_name("Game \u{a9}"), "Game");
        assert_eq!(clean_name("\u{2122}"), "");
    }

    #[test]
    fn library_paths_support_both_formats() {
        let (_, new) = vdf::parse(
            r#""libraryfolders" { "0" { "PATH" "A" } "1" { "path" "B" "apps" { } } "x" { "path" "C" } "2" { "label" "no path" } }"#,
        )
        .unwrap();
        assert_eq!(library_paths(&new), ["A", "B"]);
        let (_, old) = vdf::parse(
            r#""LibraryFolders" { "TimeNextStatsReport" "1" "1" "D" "2" "E" "label" "F" }"#,
        )
        .unwrap();
        assert_eq!(library_paths(&old), ["D", "E"]);
    }

    #[test]
    fn plain_relative_rejects_escapes() {
        assert!(is_plain_relative("Portal 2"));
        assert!(is_plain_relative("a/b"));
        assert!(!is_plain_relative(""));
        assert!(!is_plain_relative("../x"));
        assert!(!is_plain_relative("/abs"));
    }

    #[test]
    fn tool_names_are_recognized() {
        assert!(is_tool_name("Proton Experimental"));
        assert!(is_tool_name("Proton 9.0 (Beta)"));
        assert!(is_tool_name("Steam Linux Runtime 3.0 (sniper)"));
        assert!(!is_tool_name("Protonwood"));
        assert!(!is_tool_name("Portal 2"));
    }
}
