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

/// Packages registered for the current user, from
/// `HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages`.
/// Each key is a package full name with `PackageRootFolder` and one sub-key per application id.
pub(super) fn appx_packages() -> Vec<super::AppxPackage> {
    let Ok(packages) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(
        r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages",
    ) else {
        return Vec::new();
    };
    packages
        .enum_keys()
        .flatten()
        .filter_map(|full_name| {
            let key = packages.open_subkey(&full_name).ok()?;
            let root: String = key.get_value("PackageRootFolder").ok()?;
            let mut app_ids: Vec<String> = key.enum_keys().flatten().collect();
            app_ids.sort();
            Some(super::AppxPackage {
                full_name,
                root: PathBuf::from(root.trim()),
                app_ids,
            })
        })
        .collect()
}
