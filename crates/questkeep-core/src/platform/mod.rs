//! OS-specific helpers. Everything that touches the registry, OS paths or shell integration lives
//! here behind `cfg` so the rest of the core stays portable.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
mod exe_icon;
#[cfg(windows)]
mod foreground;
mod processes;
#[cfg(windows)]
mod windows;

pub use processes::{ProcessRef, list_processes, open_process};

/// Candidate Steam installation directories, most likely first. Only existing directories that
/// contain a `steamapps` folder are returned, without duplicates.
pub fn steam_install_dirs() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    candidates.extend(windows::steam_registry_paths());

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

/// The icon of an executable at about `size` px, as RGBA pixels. Windows only; elsewhere (and
/// for files without an icon) `None`.
pub fn executable_icon(path: &Path, size: u32) -> Option<crate::images::IconRgba> {
    #[cfg(windows)]
    {
        exe_icon::executable_icon(path, size)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, size);
        None
    }
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

/// `GalaxyClient.exe` of the installed GOG Galaxy, when present on disk (Windows only).
#[cfg(windows)]
pub(crate) fn gog_galaxy_client() -> Option<PathBuf> {
    windows::gog_galaxy_client()
}

/// Opens a URI (e.g. `steam://rungameid/440`) with the system's registered handler.
pub fn open_uri(uri: &str) -> io::Result<()> {
    let mut command = uri_command(uri);
    hide_console(&mut command);
    // Not waited on: the handler may keep running (it can be the store client itself).
    command.spawn().map(|_| ())
}

/// Program and arguments that open a URI on Windows. `shell:` targets (e.g.
/// `shell:AppsFolder\<PackageFamilyName>!<AppId>` for Store apps) are not URL protocol handlers,
/// so they go through `explorer.exe`; everything else through the registered protocol handler.
#[cfg(windows)]
fn windows_uri_parts(uri: &str) -> (&'static str, Vec<String>) {
    if uri
        .get(..6)
        .is_some_and(|p| p.eq_ignore_ascii_case("shell:"))
    {
        ("explorer.exe", vec![uri.to_string()])
    } else {
        (
            "rundll32.exe",
            vec!["url.dll,FileProtocolHandler".into(), uri.to_string()],
        )
    }
}

#[cfg(windows)]
fn uri_command(uri: &str) -> Command {
    let (program, args) = windows_uri_parts(uri);
    let mut c = Command::new(program);
    c.args(args);
    c
}

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

#[cfg(target_os = "macos")]
fn uri_command(uri: &str) -> Command {
    let mut c = Command::new("open");
    c.arg(uri);
    c
}

#[cfg(not(any(windows, target_os = "macos")))]
fn uri_command(uri: &str) -> Command {
    let mut c = Command::new("xdg-open");
    c.arg(uri);
    c
}

/// Prevents a console window from flashing up for helper processes on Windows.
pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// Appends user-typed arguments to a command. On Windows the string is passed through verbatim so
/// the user's quoting rules apply as typed; elsewhere it is split like a shell would.
pub fn add_raw_args(command: &mut Command, args: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if !args.trim().is_empty() {
            command.raw_arg(args);
        }
    }
    #[cfg(not(windows))]
    command.args(split_args(args));
}

/// Splits an argument string on whitespace, honouring single and double quotes and backslash
/// escapes (a small subset of POSIX shell rules).
pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_arg = true;
            }
            (None, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    in_arg = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            (None, c) => {
                current.push(c);
                in_arg = true;
            }
        }
    }
    if in_arg {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_arguments_with_quotes() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("  -a   -b "), vec!["-a", "-b"]);
        assert_eq!(
            split_args(r#"-config "my file.cfg" 'x y' a\ b """#),
            vec!["-config", "my file.cfg", "x y", "a b", ""]
        );
        assert_eq!(split_args(r#""say \"hi\"""#), vec![r#"say "hi""#]);
    }

    #[cfg(windows)]
    #[test]
    fn shell_uris_open_through_explorer() {
        let target = r"shell:AppsFolder\Pub.Game_8wekyb3d8bbwe!Game";
        assert_eq!(
            windows_uri_parts(target),
            ("explorer.exe", vec![target.to_string()])
        );
        assert_eq!(windows_uri_parts("SHELL:AppsFolder\\x").0, "explorer.exe");
        assert_eq!(
            windows_uri_parts("steam://rungameid/440"),
            (
                "rundll32.exe",
                vec![
                    "url.dll,FileProtocolHandler".to_string(),
                    "steam://rungameid/440".to_string()
                ]
            )
        );
        assert_eq!(windows_uri_parts("sh").0, "rundll32.exe");
        // Byte 6 inside a multi-byte character must not panic.
        assert_eq!(windows_uri_parts("sh\u{e9}ll:x").0, "rundll32.exe");
        assert_eq!(windows_uri_parts("sh{e9}ll:x").0, "rundll32.exe");
    }

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
