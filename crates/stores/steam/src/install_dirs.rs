//! Where Steam is installed.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: Steam's install path is read
//! from `HKCU\Software\Valve\Steam\SteamPath`. We also check the machine-wide keys written by the
//! installer.

use std::path::{Path, PathBuf};

/// Candidate Steam installation directories, most likely first. Only existing directories that
/// contain a `steamapps` folder are returned, without duplicates.
pub fn steam_install_dirs() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    candidates.extend(steam_registry_paths());

    #[cfg(windows)]
    candidates.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));

    #[cfg(target_os = "linux")]
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".steam/steam"));
        candidates.push(home.join(".steam/root"));
        candidates.push(home.join(".local/share/Steam"));
        candidates.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }

    existing_unique_dirs(candidates, |p| p.join("steamapps").is_dir())
}

fn existing_unique_dirs(candidates: Vec<PathBuf>, accept: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for candidate in candidates {
        if !accept(&candidate) {
            continue;
        }
        // Resolve symlinks (~/.steam/steam usually points at ~/.local/share/Steam).
        let key = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if !seen.contains(&key) {
            seen.push(key);
            out.push(candidate);
        }
    }
    out
}

#[cfg(windows)]
fn steam_registry_paths() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedups_and_filters_candidate_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        std::fs::create_dir_all(a.join("steamapps")).unwrap();
        let missing = tmp.path().join("missing");
        let dirs = existing_unique_dirs(vec![a.clone(), missing, a.clone()], |p| {
            p.join("steamapps").is_dir()
        });
        assert_eq!(dirs, vec![a]);
    }
}
