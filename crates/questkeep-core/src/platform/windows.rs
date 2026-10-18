//! Windows registry lookups.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: Steam's install path is read
//! from `HKCU\Software\Valve\Steam\SteamPath`. We also check the machine-wide keys written by the
//! installer.

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

pub(super) fn steam_registry_paths() -> Vec<PathBuf> {
    let lookups = [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Valve\Steam",
            "InstallPath",
        ),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath"),
    ];
    lookups
        .iter()
        .filter_map(|(hive, key, value)| {
            let path: String = RegKey::predef(*hive)
                .open_subkey(key)
                .ok()?
                .get_value(value)
                .ok()?;
            let path = path.trim().replace('/', "\\");
            (!path.is_empty()).then(|| PathBuf::from(path))
        })
        .collect()
}
