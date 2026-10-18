//! OS-specific helpers. Everything that touches the registry, OS paths or shell integration lives
//! here behind `cfg` so the rest of the core stays portable.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
mod windows;

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

/// Opens a URI (e.g. `steam://rungameid/440`) with the system's registered handler.
pub fn open_uri(uri: &str) -> io::Result<()> {
    let mut command = uri_command(uri);
    hide_console(&mut command);
    // Not waited on: the handler may keep running (it can be the store client itself).
    command.spawn().map(|_| ())
}

#[cfg(windows)]
fn uri_command(uri: &str) -> Command {
    let mut c = Command::new("rundll32.exe");
    c.args(["url.dll,FileProtocolHandler", uri]);
    c
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
