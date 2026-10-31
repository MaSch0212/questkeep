//! OS helpers stores need: the registry, the Windows "Apps" list, Store app packages, starting
//! launcher dialogs. Everything is behind `cfg` so stores stay portable.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
mod foreground;
#[cfg(windows)]
mod windows;

/// One program of the Windows "Apps" list (an `...\CurrentVersion\Uninstall\<key>` entry).
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

/// The Windows "Apps" list (machine and user entries); empty elsewhere.
pub fn uninstall_entries() -> Vec<UninstallEntry> {
    #[cfg(windows)]
    {
        windows::uninstall_entries()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// A registry string given as `HKEY_LOCAL_MACHINE\Key\Path\Value`; `None` elsewhere or when
/// missing.
pub fn registry_value(path: &str) -> Option<String> {
    #[cfg(windows)]
    {
        windows::registry_value(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// A command line such as an `UninstallString` (`"C:\x\y.exe" /a b` or `C:\x\y.exe /a`) as a
/// program and its argument string.
pub fn split_command_line(command: &str) -> Option<(String, String)> {
    let command = command.trim();
    let (program, rest) = if let Some(quoted) = command.strip_prefix('"') {
        let (program, rest) = quoted.split_once('"')?;
        (program.to_string(), rest)
    } else {
        // Unquoted paths may contain spaces; the program ends after ".exe".
        let lower = command.to_ascii_lowercase();
        match lower.find(".exe") {
            Some(end) => (command[..end + 4].to_string(), &command[end + 4..]),
            None => match command.split_once(' ') {
                Some((program, rest)) => (program.to_string(), rest),
                None => (command.to_string(), ""),
            },
        }
    };
    (!program.is_empty()).then(|| (program, rest.trim().to_string()))
}

/// Starts `program` with `args` for something the user just asked for (a launcher dialog), and
/// lets it come to the front. Does not wait.
pub fn start_in_front(program: &Path, args: &[&str]) -> io::Result<()> {
    #[cfg(windows)]
    foreground::allow_foreground_handoff();
    let mut command = Command::new(program);
    command.args(args);
    command.spawn().map(|_| ())
}

/// Presses Ctrl+`key` (a Windows virtual-key code, e.g. `b'N'`) in the program `exe` once one of
/// its windows whose title starts with `title_prefix` is in front, waiting up to `timeout`. It
/// never types into another program; false when that window did not come up. Windows only.
pub fn press_ctrl_key_in(
    exe: &str,
    title_prefix: &str,
    key: u8,
    timeout: std::time::Duration,
) -> bool {
    #[cfg(windows)]
    {
        foreground::press_ctrl_key_in(exe, title_prefix, u16::from(key), timeout)
    }
    #[cfg(not(windows))]
    {
        let _ = (exe, title_prefix, key, timeout);
        false
    }
}

/// An installed AppX/MSIX package registered for the current user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppxPackage {
    /// `<Name>_<Version>_<Arch>_<ResourceId>_<PublisherHash>`.
    pub full_name: String,
    /// Folder the package is installed in (may not exist or may be unreadable).
    pub root: PathBuf,
    /// Application ids (`Application Id` in the manifest) registered for the package.
    pub app_ids: Vec<String>,
}

/// Packages registered for the current user (Windows only; empty elsewhere).
pub fn appx_packages() -> Vec<AppxPackage> {
    #[cfg(windows)]
    {
        windows::appx_packages()
    }
    #[cfg(not(windows))]
    Vec::new()
}
