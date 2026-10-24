//! Uninstall support for GOG games: finds the uninstaller the GOG installer registered. Questkeep
//! only starts it; the uninstaller asks the user to confirm.
//!
//! Every GOG product (a game and each of its add-ons) has its own Inno Setup uninstaller
//! (`unins000.exe`, `unins001.exe`, ...) in the shared install folder and its own
//! `Uninstall\<id>_is1` registry key naming it, so the key of the game itself must be used and
//! never one of an add-on.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::path::{Path, PathBuf};

use super::install::{InstallRecord, read_info, split_command};

/// File name of the first Inno Setup uninstaller, the fallback when the registry does not name
/// one.
const UNINSTALLER: &str = "unins000.exe";

/// The uninstaller program and its command line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uninstaller {
    pub file: PathBuf,
    pub args: Option<String>,
}

/// The uninstaller of the game with this product id: the registry's `UninstallString` when its
/// program exists, otherwise `unins000.exe` in the install folder.
pub fn find_uninstaller(records: &[InstallRecord], id: &str) -> Option<Uninstaller> {
    let record = records
        .iter()
        .find(|r| r.id == id && r.depends_on.is_none())?;
    let dir = record
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(Path::new);
    // An add-on that GOG did not mark in the registry is still recognisable by its manifest.
    if let Some(info) = dir.and_then(|d| read_info(d, id))
        && !info.root_game_id.is_empty()
        && info.root_game_id != id
    {
        return None;
    }
    if let Some(command) = record.uninstall_string.as_deref() {
        let (file, args) = split_command(command);
        if file.is_file() {
            return Some(Uninstaller { file, args });
        }
    }
    let file = dir?.join(UNINSTALLER);
    file.is_file().then_some(Uninstaller { file, args: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, path: &Path) -> InstallRecord {
        InstallRecord {
            id: id.into(),
            path: Some(path.to_string_lossy().into_owned()),
            ..InstallRecord::default()
        }
    }

    fn found(file: PathBuf) -> Option<Uninstaller> {
        Some(Uninstaller { file, args: None })
    }

    #[test]
    fn finds_the_uninstaller_in_the_install_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(UNINSTALLER), b"").unwrap();
        let records = vec![record("1", tmp.path())];
        assert_eq!(
            find_uninstaller(&records, "1"),
            found(tmp.path().join(UNINSTALLER))
        );
        assert_eq!(find_uninstaller(&records, "2"), None);
    }

    #[test]
    fn missing_uninstaller_or_path_gives_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(find_uninstaller(&[record("1", tmp.path())], "1"), None);
        let no_path = InstallRecord {
            id: "1".into(),
            ..InstallRecord::default()
        };
        assert_eq!(find_uninstaller(&[no_path], "1"), None);
    }

    #[test]
    fn dlc_records_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(UNINSTALLER), b"").unwrap();
        let dlc = InstallRecord {
            depends_on: Some("1".into()),
            ..record("2", tmp.path())
        };
        assert_eq!(find_uninstaller(&[dlc], "2"), None);
    }

    /// The Cyberpunk 2077 layout of a real machine: one folder, three products, three uninstallers.
    #[test]
    fn registry_uninstall_string_picks_the_games_own_uninstaller() {
        let tmp = tempfile::tempdir().unwrap();
        for n in ["unins000.exe", "unins001.exe", "unins002.exe"] {
            std::fs::write(tmp.path().join(n), b"").unwrap();
        }
        let quoted = |n: &str| format!("\"{}\"", tmp.path().join(n).display());
        let info = |id: &str, root: &str| {
            format!(r#"{{"gameId":"{id}","rootGameId":"{root}","name":"x","playTasks":[]}}"#)
        };
        std::fs::write(
            tmp.path().join("goggame-1256837418.info"),
            info("1256837418", "1423049311"),
        )
        .unwrap();
        let records = vec![
            InstallRecord {
                uninstall_string: Some(quoted("unins000.exe")),
                ..record("1423049311", tmp.path())
            },
            // Not marked as DLC in the registry; only the manifest says so.
            InstallRecord {
                uninstall_string: Some(quoted("unins001.exe")),
                ..record("1256837418", tmp.path())
            },
            InstallRecord {
                uninstall_string: Some(format!("{} /SILENT", quoted("unins002.exe"))),
                depends_on: Some("1423049311".into()),
                ..record("1597316373", tmp.path())
            },
        ];
        assert_eq!(
            find_uninstaller(&records, "1423049311"),
            found(tmp.path().join("unins000.exe"))
        );
        assert_eq!(
            find_uninstaller(&records, "1256837418"),
            None,
            "manifest DLC"
        );
        assert_eq!(
            find_uninstaller(&records, "1597316373"),
            None,
            "registry DLC"
        );
    }

    #[test]
    fn uninstall_string_arguments_are_kept_and_stale_strings_fall_back() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("unins003.exe"), b"").unwrap();
        std::fs::write(tmp.path().join(UNINSTALLER), b"").unwrap();
        let with_args = InstallRecord {
            uninstall_string: Some(format!(
                "\"{}\" /LOG",
                tmp.path().join("unins003.exe").display()
            )),
            ..record("1", tmp.path())
        };
        let got = find_uninstaller(&[with_args], "1").unwrap();
        assert_eq!(got.file, tmp.path().join("unins003.exe"));
        assert_eq!(got.args.as_deref(), Some("/LOG"));

        let stale = InstallRecord {
            uninstall_string: Some("\"Z:\\gone\\unins000.exe\"".into()),
            ..record("1", tmp.path())
        };
        assert_eq!(
            find_uninstaller(&[stale], "1"),
            found(tmp.path().join(UNINSTALLER))
        );
    }
}
