//! Windows registry access for GOG: the thin, untestable layer. Everything it returns is plain
//! data handed to [`super::install`].

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use super::install::InstallRecord;

const GAMES_KEYS: [&str; 2] = [
    r"SOFTWARE\WOW6432Node\GOG.com\Games",
    r"SOFTWARE\GOG.com\Games",
];
const GALAXY_PATH_KEYS: [&str; 2] = [
    r"SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths",
    r"SOFTWARE\GOG.com\GalaxyClient\paths",
];

fn text(key: &RegKey, name: &str) -> Option<String> {
    let value: String = key.get_value(name).ok()?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// Install records of every game or add-on that a GOG installer registered. `None` when GOG has
/// never installed anything on this machine.
pub(super) fn install_records() -> Option<Vec<InstallRecord>> {
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
            })
        })
        .collect();
    Some(records)
}

/// `GalaxyClient.exe` of the installed GOG Galaxy, when present on disk.
pub(super) fn galaxy_client() -> Option<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    GALAXY_PATH_KEYS.iter().find_map(|path| {
        let key = hklm.open_subkey_with_flags(path, KEY_READ).ok()?;
        let exe = PathBuf::from(text(&key, "client")?).join("GalaxyClient.exe");
        exe.is_file().then_some(exe)
    })
}
