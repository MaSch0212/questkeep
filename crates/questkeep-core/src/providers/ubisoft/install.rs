//! Installed-game detection: turns the launcher's install records into [`ImportedGame`]s. Pure
//! data in, data out, so it is tested without a registry.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::path::{MAIN_SEPARATOR_STR, PathBuf};

use super::config::Product;
use crate::model::{ImportedGame, PlayAction, TrackingMode};

/// One `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>` key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallRecord {
    /// The registry key name: Ubisoft's numeric product id.
    pub id: String,
    /// `InstallDir`; the launcher writes it with forward slashes and a trailing one.
    pub install_dir: Option<String>,
    /// `DisplayName` of the matching `Uninstall\Uplay Install <id>` key, a name fallback.
    pub display_name: Option<String>,
}

/// Whether `id` is a Ubisoft product id (digits only).
pub fn is_product_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 10 && id.chars().all(|c| c.is_ascii_digit())
}

/// The URI that starts a game through Ubisoft Connect (`/0` is the plain launch mode the
/// launcher's own shortcuts use).
pub fn launch_uri(id: &str) -> String {
    format!("uplay://launch/{id}/0")
}

/// The launcher's install dialog for a game.
pub fn install_uri(id: &str) -> String {
    format!("uplay://install/{id}")
}

/// The launcher's uninstall command, the same one its `Uninstall\Uplay Install <id>` key uses.
pub fn uninstall_uri(id: &str) -> String {
    format!("uplay://uninstall/{id}")
}

/// Install directory as a native path without a trailing separator.
pub fn native_dir(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    Some(
        PathBuf::from(raw.replace(['/', '\\'], MAIN_SEPARATOR_STR))
            .components()
            .collect(),
    )
}

/// Drops trademark signs and surrounding space from a store name.
pub fn clean_name(name: &str) -> String {
    name.replace(['\u{2122}', '\u{00ae}'], "")
        .trim()
        .to_string()
}

/// Placeholders some configs carry instead of a name (their real name lives elsewhere).
pub(super) fn is_placeholder(name: &str) -> bool {
    matches!(name, "GAMENAME" | "NAME")
}

/// Installed games: records whose install folder exists. The name is the config cache's, then
/// the registry display name, then the folder name.
pub fn games_from_records(
    records: &[InstallRecord],
    products: &HashMap<u32, Product>,
) -> Vec<ImportedGame> {
    let mut games: Vec<ImportedGame> = Vec::new();
    for record in records {
        let id = record.id.trim();
        if !is_product_id(id) || games.iter().any(|g| g.source_game_id == id) {
            continue;
        }
        let Some(dir) = record.install_dir.as_deref().and_then(native_dir) else {
            continue;
        };
        if !dir.is_dir() {
            continue;
        }
        let from_config = id
            .parse::<u32>()
            .ok()
            .and_then(|n| products.get(&n))
            .and_then(|p| p.name.as_deref())
            .map(clean_name)
            .filter(|n| !n.is_empty() && !is_placeholder(n));
        let from_registry = record
            .display_name
            .as_deref()
            .map(clean_name)
            .filter(|n| !n.is_empty());
        let from_folder = dir
            .file_name()
            .map(|n| clean_name(&n.to_string_lossy()))
            .filter(|n| !n.is_empty());
        let Some(name) = from_config.or(from_registry).or(from_folder) else {
            continue;
        };
        let dir_text = dir.to_string_lossy().into_owned();
        let mut action = PlayAction::uri(launch_uri(id));
        // The URI starts the launcher; the game itself runs from its install dir.
        action.tracking = TrackingMode::Directory;
        action.tracking_path = Some(dir_text.clone());
        games.push(ImportedGame {
            install_dir: Some(dir_text),
            play_action: Some(action),
            ..ImportedGame::installed(id, name)
        });
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ubisoft::config::parse_product;
    use crate::providers::ubisoft::config::tests::{BROTHERHOOD, TRACKMANIA};

    fn record(id: &str, dir: &std::path::Path) -> InstallRecord {
        InstallRecord {
            id: id.into(),
            install_dir: Some(dir.to_string_lossy().into_owned()),
            display_name: None,
        }
    }

    fn products() -> HashMap<u32, Product> {
        HashMap::from([
            (5595, parse_product(5595, TRACKMANIA)),
            (26, parse_product(26, BROTHERHOOD)),
        ])
    }

    #[test]
    fn installed_game_gets_launch_uri_and_directory_tracking() {
        let tmp = tempfile::tempdir().unwrap();
        let games = games_from_records(&[record("5595", tmp.path())], &products());
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert_eq!(g.source_game_id, "5595");
        assert_eq!(g.name, "Trackmania");
        assert!(g.installed);
        let dir = tmp.path().to_string_lossy().into_owned();
        assert_eq!(g.install_dir.as_deref(), Some(dir.as_str()));
        let action = g.play_action.as_ref().unwrap();
        assert_eq!(action.target, "uplay://launch/5595/0");
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(action.tracking_path.as_deref(), Some(dir.as_str()));
    }

    #[test]
    fn missing_install_dir_or_bad_id_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let gone = tmp.path().join("gone");
        let no_dir = InstallRecord {
            id: "5595".into(),
            ..InstallRecord::default()
        };
        let games = games_from_records(
            &[
                record("5595", &gone),
                no_dir,
                record("Uplay", tmp.path()),
                record("", tmp.path()),
            ],
            &products(),
        );
        assert!(games.is_empty());
    }

    #[test]
    fn name_falls_back_to_registry_then_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("Some Game");
        std::fs::create_dir(&folder).unwrap();
        let none = HashMap::new();
        let mut rec = record("77", &folder);
        assert_eq!(
            games_from_records(&[rec.clone()], &none)[0].name,
            "Some Game"
        );
        rec.display_name = Some("Some Game™ Deluxe".into());
        assert_eq!(
            games_from_records(&[rec], &none)[0].name,
            "Some Game Deluxe"
        );
    }

    #[test]
    fn placeholder_config_names_are_not_used() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("Real Name");
        std::fs::create_dir(&folder).unwrap();
        let products = HashMap::from([(
            40,
            Product {
                id: 40,
                name: Some("GAMENAME".into()),
                ..Product::default()
            },
        )]);
        assert_eq!(
            games_from_records(&[record("40", &folder)], &products)[0].name,
            "Real Name"
        );
    }

    #[test]
    fn forward_slash_dirs_with_trailing_slash_are_normalised() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = format!("{}/", tmp.path().to_string_lossy().replace('\\', "/"));
        assert_eq!(native_dir(&raw), Some(tmp.path().to_path_buf()));
        assert_eq!(native_dir("  "), None);
    }

    #[test]
    fn duplicate_ids_yield_one_game() {
        let tmp = tempfile::tempdir().unwrap();
        let r = record("26", tmp.path());
        assert_eq!(games_from_records(&[r.clone(), r], &products()).len(), 1);
    }

    #[test]
    fn uris_and_ids() {
        assert_eq!(install_uri("5595"), "uplay://install/5595");
        assert_eq!(uninstall_uri("5595"), "uplay://uninstall/5595");
        assert!(is_product_id("635") && !is_product_id("") && !is_product_id("6a"));
    }
}
