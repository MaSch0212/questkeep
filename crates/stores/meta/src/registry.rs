//! Windows registry access for Meta Horizon Link: the thin, untestable layer.

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

const LIBRARIES_KEY: &str = r"Software\Oculus VR, LLC\Oculus\Libraries";
const INSTALL_KEYS: [&str; 2] = [
    r"SOFTWARE\WOW6432Node\Oculus VR, LLC\Oculus",
    r"SOFTWARE\Oculus VR, LLC\Oculus",
];

fn text(key: &RegKey, name: &str) -> Option<String> {
    let value: String = key.get_value(name).ok()?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// `(Path, OriginalPath)` of every library, the default library first.
pub(super) fn libraries() -> Vec<(String, Option<String>)> {
    let Ok(root) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(LIBRARIES_KEY, KEY_READ)
    else {
        return Vec::new();
    };
    let default = text(&root, "DefaultLibrary");
    let mut names: Vec<String> = root.enum_keys().flatten().collect();
    names.sort_by_key(|n| Some(n) != default.as_ref());
    names
        .into_iter()
        .filter_map(|name| {
            let key = root.open_subkey_with_flags(&name, KEY_READ).ok()?;
            let path = text(&key, "Path");
            let original = text(&key, "OriginalPath");
            Some((path.or_else(|| original.clone())?, original))
        })
        .collect()
}

/// The Meta app's install folder (`Base`), when it exists.
pub(super) fn install_dir() -> Option<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    INSTALL_KEYS.iter().find_map(|path| {
        let key = hklm.open_subkey_with_flags(path, KEY_READ).ok()?;
        let dir = PathBuf::from(text(&key, "Base")?.trim_end_matches(['\\', '/']));
        dir.is_dir().then_some(dir)
    })
}
