//! Windows registry access for Ubisoft Connect: the thin, untestable layer. Everything it returns
//! is plain data handed to [`super::install`].

use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use super::install::InstallRecord;

const INSTALLS_KEYS: [&str; 2] = [
    r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs",
    r"SOFTWARE\Ubisoft\Launcher\Installs",
];

const UNINSTALL_KEYS: [&str; 2] = [
    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
];

fn text(key: &RegKey, name: &str) -> Option<String> {
    let value: String = key.get_value(name).ok()?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// `DisplayName` of `Uninstall\Uplay Install <id>`, which the launcher writes for every game.
fn display_name(hklm: &RegKey, id: &str) -> Option<String> {
    UNINSTALL_KEYS.iter().find_map(|path| {
        let parent = hklm.open_subkey_with_flags(path, KEY_READ).ok()?;
        let key = parent
            .open_subkey_with_flags(format!("Uplay Install {id}"), KEY_READ)
            .ok()?;
        text(&key, "DisplayName")
    })
}

/// The install records of every game the launcher installed. `None` when the launcher has never
/// installed anything on this machine.
pub(super) fn install_records() -> Option<Vec<InstallRecord>> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let installs = INSTALLS_KEYS
        .iter()
        .find_map(|path| hklm.open_subkey_with_flags(path, KEY_READ).ok())?;
    let records = installs
        .enum_keys()
        .flatten()
        .filter_map(|id| {
            let key = installs.open_subkey_with_flags(&id, KEY_READ).ok()?;
            Some(InstallRecord {
                install_dir: text(&key, "InstallDir"),
                display_name: display_name(&hklm, &id),
                id,
            })
        })
        .collect();
    Some(records)
}
