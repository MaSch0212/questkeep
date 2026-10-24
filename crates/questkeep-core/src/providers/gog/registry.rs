//! Windows registry access for GOG: the thin, untestable layer. Everything it returns is plain
//! data handed to [`super::install`].

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

use super::install::{
    InstallRecord, UninstallEntry, merge_uninstall_entries, product_id_of_uninstall_key,
};

const GAMES_KEYS: [&str; 2] = [
    r"SOFTWARE\WOW6432Node\GOG.com\Games",
    r"SOFTWARE\GOG.com\Games",
];

/// Where Inno Setup (which the GOG installers are built with) registers uninstallers.
const UNINSTALL_KEYS: [&str; 2] = [
    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
];

fn text(key: &RegKey, name: &str) -> Option<String> {
    let value: String = key.get_value(name).ok()?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn games_records() -> Option<Vec<InstallRecord>> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let games = GAMES_KEYS
        .iter()
        .find_map(|path| hklm.open_subkey_with_flags(path, KEY_READ).ok())?;
    let records = games
        .enum_keys()
        .flatten()
        .filter_map(|id| {
            let key = games.open_subkey_with_flags(&id, KEY_READ).ok()?;
            Some(InstallRecord {
                game_name: text(&key, "gameName"),
                path: text(&key, "path"),
                exe: text(&key, "exe"),
                launch_param: text(&key, "launchParam"),
                working_dir: text(&key, "workingDir"),
                depends_on: text(&key, "dependsOn"),
                id,
                ..InstallRecord::default()
            })
        })
        .collect();
    Some(records)
}

/// The `<product id>_is1` keys under the Uninstall keys of the machine and the current user.
fn uninstall_entries() -> Vec<UninstallEntry> {
    let mut entries = Vec::new();
    for root in [
        RegKey::predef(HKEY_LOCAL_MACHINE),
        RegKey::predef(HKEY_CURRENT_USER),
    ] {
        for path in UNINSTALL_KEYS {
            let Ok(parent) = root.open_subkey_with_flags(path, KEY_READ) else {
                continue;
            };
            for name in parent.enum_keys().flatten() {
                let Some(id) = product_id_of_uninstall_key(&name) else {
                    continue;
                };
                let Ok(key) = parent.open_subkey_with_flags(&name, KEY_READ) else {
                    continue;
                };
                entries.push(UninstallEntry {
                    id: id.to_string(),
                    display_name: text(&key, "DisplayName"),
                    uninstall_string: text(&key, "UninstallString"),
                    install_location: text(&key, "InstallLocation"),
                });
            }
        }
    }
    entries
}

/// Install records of every game or add-on that a GOG installer registered. `None` when GOG has
/// never installed anything on this machine.
pub(super) fn install_records() -> Option<Vec<InstallRecord>> {
    let entries = uninstall_entries();
    let mut records = games_records();
    if records.is_none() && entries.is_empty() {
        return None;
    }
    merge_uninstall_entries(records.get_or_insert_with(Vec::new), entries);
    records
}

/// `GalaxyClient.exe` of the installed GOG Galaxy, when present on disk.
pub(super) fn galaxy_client() -> Option<PathBuf> {
    crate::platform::gog_galaxy_client()
}
