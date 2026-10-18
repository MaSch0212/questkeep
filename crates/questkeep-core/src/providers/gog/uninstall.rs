//! Uninstall support for GOG games: finds the uninstaller the GOG installer put into the game's
//! folder. Questkeep only starts it; the uninstaller asks the user to confirm.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::path::{Path, PathBuf};

use super::install::InstallRecord;

/// File name of the Inno Setup uninstaller next to the game files.
const UNINSTALLER: &str = "unins000.exe";

/// The uninstaller of the game with this product id, when its install folder has one.
pub fn find_uninstaller(records: &[InstallRecord], id: &str) -> Option<PathBuf> {
    let record = records
        .iter()
        .find(|r| r.id == id && r.depends_on.is_none())?;
    let dir = record.path.as_deref().filter(|p| !p.trim().is_empty())?;
    let file = Path::new(dir).join(UNINSTALLER);
    file.is_file().then_some(file)
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

    #[test]
    fn finds_the_uninstaller_in_the_install_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(UNINSTALLER), b"").unwrap();
        let records = vec![record("1", tmp.path())];
        assert_eq!(
            find_uninstaller(&records, "1"),
            Some(tmp.path().join(UNINSTALLER))
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
}
