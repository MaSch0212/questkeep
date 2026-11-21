//! What a store plugin may ask the app for besides files and web requests: where things are on
//! this PC, the registry, other programs' SQLite databases, its own secrets and the store's
//! website session.
//!
//! Each part needs its permission in `plugin.json` (`permissions.filesystem`,
//! `permissions.registry`, `permissions.login`); without it the call fails with a readable
//! message (or answers `None` where noted). Outside Questkeep (a native unit test) the calls
//! that need the app fail; [`host_path`] is a pure function and works everywhere.

use crate::ProviderError;

/// Where the plugin sees the files of this PC: the root of the file system on Linux and macOS
/// (`/home/me` is `/host/home/me`), each drive on Windows (`C:\Games` is `/host/c/Games`).
pub const HOST_ROOT: &str = "/host";

/// The operating system the app runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    MacOs,
}

/// The operating system the app runs on.
pub fn platform() -> Platform {
    match sys::platform().as_str() {
        "windows" => Platform::Windows,
        "macos" => Platform::MacOs,
        _ => Platform::Linux,
    }
}

/// A folder of this PC as the plugin sees it (below [`HOST_ROOT`]): `home`, `local-app-data`,
/// `app-data`, `program-data`, `program-files`, `program-files-x86`, `config` (the user's config
/// folder), `data` (the user's data folder). `None` without `permissions.filesystem`, for an
/// unknown name, or when the folder does not exist on this platform.
pub fn known_folder(name: &str) -> Option<String> {
    sys::known_folder(name)
}

/// A path of this PC as the plugin sees it: `C:\Program Files\Steam` becomes
/// `/host/c/Program Files/Steam`, `/home/me` becomes `/host/home/me`. Values read from the
/// registry or another program's files are such paths. `None` for paths with no drive or root
/// (relative paths, `\\server\share`).
pub fn host_path(native: &str) -> Option<String> {
    let native = native.trim();
    let bytes = native.as_bytes();
    if bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes.get(2).is_none_or(|b| matches!(b, b'\\' | b'/'))
    {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = native[2..].replace('\\', "/");
        let rest = rest.trim_matches('/');
        return Some(if rest.is_empty() {
            format!("{HOST_ROOT}/{drive}")
        } else {
            format!("{HOST_ROOT}/{drive}/{rest}")
        });
    }
    if native.starts_with("//") || native.starts_with("\\\\") {
        return None;
    }
    if native.starts_with('/') {
        let rest = native.trim_end_matches('/');
        return Some(format!("{HOST_ROOT}{rest}"));
    }
    None
}

/// The registry (Windows). Needs `permissions.registry`.
pub mod registry {
    use super::{ProviderError, sys};

    /// One program of the Windows "Apps" list.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct UninstallEntry {
        /// The registry key name (a product code, a GUID, or the program's own id).
        pub key: String,
        pub display_name: Option<String>,
        pub publisher: Option<String>,
        pub install_location: Option<String>,
        pub uninstall_string: Option<String>,
        pub display_icon: Option<String>,
    }

    /// An installed Store app package.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct AppxPackage {
        pub full_name: String,
        /// The package folder as the plugin sees it (below `/host`).
        pub root: String,
        pub app_ids: Vec<String>,
    }

    /// A registry string, given as `HKEY_LOCAL_MACHINE\Key\Path\Value` (also `HKLM`, `HKCU`,
    /// `HKCR`). A trailing `\` reads the key's default value. `Ok(None)` when it is missing, and
    /// always off Windows.
    pub fn read(path: &str) -> Result<Option<String>, ProviderError> {
        sys::registry_read(path).map_err(ProviderError::Io)
    }

    /// The names of a key's subkeys (empty when it is missing).
    pub fn subkeys(path: &str) -> Result<Vec<String>, ProviderError> {
        sys::registry_enum(path).map_err(ProviderError::Io)
    }

    /// The programs of the Windows "Apps" list.
    pub fn uninstall_entries() -> Result<Vec<UninstallEntry>, ProviderError> {
        sys::uninstall_entries().map_err(ProviderError::Io)
    }

    /// The Store app packages of the current user.
    pub fn appx_packages() -> Result<Vec<AppxPackage>, ProviderError> {
        sys::appx_packages().map_err(ProviderError::Io)
    }
}

/// Other programs' SQLite databases. Needs `permissions.filesystem`.
pub mod sqlite {
    use super::{ProviderError, sys};

    /// The answer of [`query`].
    #[derive(Debug, Clone, PartialEq)]
    pub struct Rows {
        pub columns: Vec<String>,
        /// Numbers, text and null; a blob is `{"blob": "<hex>"}`.
        pub rows: Vec<Vec<serde_json::Value>>,
    }

    /// Runs one read-only statement on the database at `path` (a path as the plugin sees it,
    /// see [`host_path`](super::host_path)). `?` placeholders are bound to `params` as text. A
    /// database its owner has locked is read from a temporary copy.
    pub fn query(path: &str, sql: &str, params: &[&str]) -> Result<Rows, ProviderError> {
        let json = sys::sqlite_query(path, sql, params).map_err(ProviderError::Io)?;
        #[derive(serde::Deserialize)]
        struct Wire {
            columns: Vec<String>,
            rows: Vec<Vec<serde_json::Value>>,
        }
        let wire: Wire = serde_json::from_str(&json)
            .map_err(|e| ProviderError::Parse(format!("unreadable query answer: {e}")))?;
        Ok(Rows {
            columns: wire.columns,
            rows: wire.rows,
        })
    }
}

/// The plugin's own secrets, kept in the system keyring (never in a file). Keys are any text;
/// the app keeps them apart from every other plugin's. Every value is limited to 1200
/// characters. Use them for tokens: `/data` is readable by other plugins.
pub mod secrets {
    use super::{ProviderError, sys};

    pub fn get(key: &str) -> Result<Option<String>, ProviderError> {
        sys::secret_get(key).map_err(ProviderError::NotConfigured)
    }

    pub fn set(key: &str, value: &str) -> Result<(), ProviderError> {
        sys::secret_set(key, value).map_err(ProviderError::NotConfigured)
    }

    /// Deleting something that does not exist is not an error.
    pub fn delete(key: &str) -> Result<(), ProviderError> {
        sys::secret_delete(key).map_err(ProviderError::NotConfigured)
    }
}

/// The store's website session (`permissions.login.session` in `plugin.json`): a hidden window
/// loads the session page on the plugin's own browser profile, and the page's script reports
/// what it read by navigating to an address starting with `https://questkeep.invalid/`.
pub mod session {
    use super::{ProviderError, sys};

    /// What the session page reported (the whole reported address), or `None` when the website
    /// session is signed out.
    pub fn refresh() -> Result<Option<String>, ProviderError> {
        sys::session_refresh().map_err(ProviderError::Network)
    }

    /// The same for the renewal page (`session.renewUrl`).
    pub fn renew() -> Result<Option<String>, ProviderError> {
        sys::session_renew().map_err(ProviderError::Network)
    }
}

#[cfg(target_arch = "wasm32")]
mod sys {
    use super::registry::{AppxPackage, UninstallEntry};
    use crate::bindings::questkeep::plugin::host;

    pub fn platform() -> String {
        host::platform()
    }

    pub fn known_folder(name: &str) -> Option<String> {
        host::known_folder(name)
    }

    pub fn registry_read(path: &str) -> Result<Option<String>, String> {
        host::registry_read(path)
    }

    pub fn registry_enum(path: &str) -> Result<Vec<String>, String> {
        host::registry_enum(path)
    }

    pub fn uninstall_entries() -> Result<Vec<UninstallEntry>, String> {
        Ok(host::uninstall_entries()?
            .into_iter()
            .map(|e| UninstallEntry {
                key: e.key,
                display_name: e.display_name,
                publisher: e.publisher,
                install_location: e.install_location,
                uninstall_string: e.uninstall_string,
                display_icon: e.display_icon,
            })
            .collect())
    }

    pub fn appx_packages() -> Result<Vec<AppxPackage>, String> {
        Ok(host::appx_packages()?
            .into_iter()
            .map(|p| AppxPackage {
                full_name: p.full_name,
                root: p.root,
                app_ids: p.app_ids,
            })
            .collect())
    }

    pub fn sqlite_query(path: &str, sql: &str, params: &[&str]) -> Result<String, String> {
        let params: Vec<String> = params.iter().map(|p| p.to_string()).collect();
        host::sqlite_query(path, sql, &params)
    }

    pub fn secret_get(key: &str) -> Result<Option<String>, String> {
        host::secret_get(key)
    }

    pub fn secret_set(key: &str, value: &str) -> Result<(), String> {
        host::secret_set(key, value)
    }

    pub fn secret_delete(key: &str) -> Result<(), String> {
        host::secret_delete(key)
    }

    pub fn session_refresh() -> Result<Option<String>, String> {
        host::session_refresh()
    }

    pub fn session_renew() -> Result<Option<String>, String> {
        host::session_renew()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod sys {
    use super::registry::{AppxPackage, UninstallEntry};

    const OUTSIDE: &str = "this only works inside Questkeep";

    pub fn platform() -> String {
        if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        }
        .into()
    }

    pub fn known_folder(_name: &str) -> Option<String> {
        None
    }

    pub fn registry_read(_path: &str) -> Result<Option<String>, String> {
        Err(OUTSIDE.into())
    }

    pub fn registry_enum(_path: &str) -> Result<Vec<String>, String> {
        Err(OUTSIDE.into())
    }

    pub fn uninstall_entries() -> Result<Vec<UninstallEntry>, String> {
        Err(OUTSIDE.into())
    }

    pub fn appx_packages() -> Result<Vec<AppxPackage>, String> {
        Err(OUTSIDE.into())
    }

    pub fn sqlite_query(_path: &str, _sql: &str, _params: &[&str]) -> Result<String, String> {
        Err(OUTSIDE.into())
    }

    pub fn secret_get(_key: &str) -> Result<Option<String>, String> {
        Err(OUTSIDE.into())
    }

    pub fn secret_set(_key: &str, _value: &str) -> Result<(), String> {
        Err(OUTSIDE.into())
    }

    pub fn secret_delete(_key: &str) -> Result<(), String> {
        Err(OUTSIDE.into())
    }

    pub fn session_refresh() -> Result<Option<String>, String> {
        Err(OUTSIDE.into())
    }

    pub fn session_renew() -> Result<Option<String>, String> {
        Err(OUTSIDE.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_become_drive_folders() {
        assert_eq!(
            host_path(r"C:\Program Files (x86)\Steam").as_deref(),
            Some("/host/c/Program Files (x86)/Steam")
        );
        assert_eq!(host_path("d:/Games/").as_deref(), Some("/host/d/Games"));
        assert_eq!(host_path(r"E:\").as_deref(), Some("/host/e"));
        assert_eq!(host_path("E:").as_deref(), Some("/host/e"));
        assert_eq!(host_path(r"  C:\x  ").as_deref(), Some("/host/c/x"));
    }

    #[test]
    fn unix_paths_go_below_the_host_root() {
        assert_eq!(
            host_path("/home/me/.config").as_deref(),
            Some("/host/home/me/.config")
        );
        assert_eq!(host_path("/").as_deref(), Some("/host"));
    }

    #[test]
    fn paths_without_a_drive_or_root_have_no_host_path() {
        for path in [
            "",
            "relative/path",
            r"\\server\share\x",
            "//server/share",
            "C:rel",
            "1:\\x",
        ] {
            assert_eq!(host_path(path), None, "{path}");
        }
    }
}
