//! OS-specific helpers. Everything that touches the registry, OS paths or shell integration lives
//! here behind `cfg` so the rest of the core stays portable.

use std::io;
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
mod exe_icon;
#[cfg(windows)]
mod priority;
mod processes;

pub use processes::{ProcessRef, list_processes, open_process};

#[cfg(windows)]
pub use priority::BackgroundPriority;

/// Lowers the current thread's priority until dropped (a no-op outside Windows).
#[cfg(not(windows))]
pub struct BackgroundPriority;

#[cfg(not(windows))]
impl BackgroundPriority {
    pub fn begin() -> Self {
        Self
    }
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
}
