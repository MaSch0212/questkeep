//! Windows registry lookups.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: Steam's install path is read
//! from `HKCU\Software\Valve\Steam\SteamPath`. We also check the machine-wide keys written by the
//! installer.

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

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

/// Every program of the Windows "Apps" list: the 64- and 32-bit machine keys and the user key.
pub(super) fn uninstall_entries() -> Vec<super::UninstallEntry> {
    const KEYS: [(winreg::HKEY, &str); 3] = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];
    let mut entries = Vec::new();
    for (hive, path) in KEYS {
        let Ok(parent) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else {
            continue;
        };
        for name in parent.enum_keys().flatten() {
            let Ok(key) = parent.open_subkey_with_flags(&name, KEY_READ) else {
                continue;
            };
            let text = |value: &str| {
                key.get_value::<String, _>(value)
                    .ok()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            entries.push(super::UninstallEntry {
                key: name.clone(),
                display_name: text("DisplayName"),
                publisher: text("Publisher"),
                install_location: text("InstallLocation"),
                uninstall_string: text("UninstallString"),
                display_icon: text("DisplayIcon"),
            });
        }
    }
    entries
}

/// A registry value given as `HKEY_LOCAL_MACHINE\Some\Key\Value` (the 64-bit view first, then
/// the 32-bit one).
pub(super) fn registry_value(path: &str) -> Option<String> {
    use winreg::enums::{HKEY_CLASSES_ROOT, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    let (hive_name, rest) = path.split_once('\\')?;
    let hive = match hive_name.to_ascii_uppercase().as_str() {
        "HKEY_LOCAL_MACHINE" | "HKLM" => HKEY_LOCAL_MACHINE,
        "HKEY_CURRENT_USER" | "HKCU" => HKEY_CURRENT_USER,
        "HKEY_CLASSES_ROOT" | "HKCR" => HKEY_CLASSES_ROOT,
        _ => return None,
    };
    let (key, value) = rest.rsplit_once('\\')?;
    [KEY_WOW64_64KEY, KEY_WOW64_32KEY].iter().find_map(|view| {
        RegKey::predef(hive)
            .open_subkey_with_flags(key, KEY_READ | view)
            .ok()?
            .get_value::<String, _>(value)
            .ok()
    })
}
