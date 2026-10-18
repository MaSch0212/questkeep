//! Windows registry lookups.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: Steam's install path is read
//! from `HKCU\Software\Valve\Steam\SteamPath`. We also check the machine-wide keys written by the
//! installer.

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

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

/// The Epic Games Launcher keeps its data under `%ProgramData%\Epic\EpicGamesLauncher`, which
/// exists once the launcher ran on this machine.
pub(super) fn epic_launcher_installed() -> bool {
    let program_data = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    program_data.join(r"Epic\EpicGamesLauncher").is_dir()
}

/// `GalaxyClient.exe` of the installed GOG Galaxy, when present on disk (same registry keys the
/// GOG provider reads).
pub(super) fn gog_galaxy_client() -> Option<PathBuf> {
    const KEYS: [&str; 2] = [
        r"SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths",
        r"SOFTWARE\GOG.com\GalaxyClient\paths",
    ];
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    KEYS.iter().find_map(|path| {
        let key = hklm.open_subkey_with_flags(path, KEY_READ).ok()?;
        let dir: String = key.get_value("client").ok()?;
        let exe = PathBuf::from(dir.trim()).join("GalaxyClient.exe");
        exe.is_file().then_some(exe)
    })
}

/// Whether the Xbox app (`Microsoft.GamingApp`) is registered for the current user.
pub(super) fn xbox_app_installed() -> bool {
    appx_packages()
        .iter()
        .any(|p| p.full_name.starts_with("Microsoft.GamingApp_"))
}
