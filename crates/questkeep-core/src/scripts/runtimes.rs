//! Scripting runtimes installed on this PC. Only runtimes found here are offered. Detection never
//! starts a program (Windows' `python.exe` in `WindowsApps` is a Microsoft Store stub that hangs).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{RuntimeId, RuntimeInfo};

/// The runtimes found on this PC, in [`RuntimeId::ALL`] order.
pub fn detect() -> Vec<RuntimeInfo> {
    detect_with(&search_path(), &Env::system())
}

/// Folders and well-known places to look in (tests pass their own).
pub(crate) struct Env {
    pub program_files: Vec<PathBuf>,
    pub system_root: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
}

impl Env {
    fn system() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Self {
            program_files: ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
                .iter()
                .filter_map(|n| var(n))
                .collect(),
            system_root: var("SystemRoot"),
            local_app_data: var("LOCALAPPDATA"),
        }
    }
}

fn search_path() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

pub(crate) fn detect_with(path: &[PathBuf], env: &Env) -> Vec<RuntimeInfo> {
    RuntimeId::ALL
        .iter()
        .filter_map(|&id| {
            find(id, path, env).map(|exe| RuntimeInfo {
                id,
                name: display_name(id).to_string(),
                path: exe.display().to_string(),
                extension: id.extension().to_string(),
            })
        })
        .collect()
}

pub fn display_name(id: RuntimeId) -> &'static str {
    match id {
        RuntimeId::Pwsh => "PowerShell 7",
        RuntimeId::WindowsPowershell => "Windows PowerShell 5.1",
        RuntimeId::Bash => "Bash",
        RuntimeId::Cmd => "Command Prompt",
        RuntimeId::Python => "Python",
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The first file named `name` in the search path that `accept` allows.
fn on_path(path: &[PathBuf], name: &str, accept: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    path.iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file() && accept(candidate))
}

fn is_store_stub(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
}

fn find(id: RuntimeId, path: &[PathBuf], env: &Env) -> Option<PathBuf> {
    match id {
        RuntimeId::Pwsh => on_path(path, &exe("pwsh"), |_| true).or_else(|| {
            env.program_files
                .iter()
                .map(|p| p.join("PowerShell").join("7").join("pwsh.exe"))
                .find(|p| cfg!(windows) && p.is_file())
        }),
        RuntimeId::WindowsPowershell => env
            .system_root
            .as_ref()
            .map(|root| {
                root.join("System32")
                    .join("WindowsPowerShell")
                    .join("v1.0")
                    .join("powershell.exe")
            })
            .filter(|p| cfg!(windows) && p.is_file()),
        RuntimeId::Cmd => env
            .system_root
            .as_ref()
            .map(|root| root.join("System32").join("cmd.exe"))
            .filter(|p| cfg!(windows) && p.is_file()),
        RuntimeId::Bash => {
            if cfg!(windows) {
                // Git for Windows; never WSL's System32\bash.exe.
                let mut bases = env.program_files.clone();
                bases.extend(env.local_app_data.iter().map(|l| l.join("Programs")));
                bases
                    .iter()
                    .map(|p| p.join("Git").join("bin").join("bash.exe"))
                    .find(|p| p.is_file())
                    .or_else(|| {
                        on_path(path, "bash.exe", |p| {
                            !p.components()
                                .any(|c| c.as_os_str().eq_ignore_ascii_case("System32"))
                                && !is_store_stub(p)
                        })
                    })
            } else {
                [PathBuf::from("/bin/bash"), PathBuf::from("/usr/bin/bash")]
                    .into_iter()
                    .find(|p| p.is_file())
                    .or_else(|| on_path(path, "bash", |_| true))
            }
        }
        RuntimeId::Python => {
            if cfg!(windows) {
                on_path(path, "py.exe", |p| !is_store_stub(p))
                    .or_else(|| on_path(path, "python.exe", |p| !is_store_stub(p)))
                    .or_else(|| {
                        env.system_root
                            .as_ref()
                            .map(|root| root.join("py.exe"))
                            .filter(|p| p.is_file())
                    })
            } else {
                on_path(path, "python3", |_| true)
            }
        }
    }
}

/// The runtime for a script file by its extension (`None`: run the file itself).
pub fn for_extension(file: &Path) -> Option<RuntimeId> {
    let ext = file.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "ps1" => Some(if cfg!(windows) {
            RuntimeId::WindowsPowershell
        } else {
            RuntimeId::Pwsh
        }),
        "sh" | "bash" => Some(RuntimeId::Bash),
        "cmd" | "bat" => Some(RuntimeId::Cmd),
        "py" | "pyw" => Some(RuntimeId::Python),
        _ => None,
    }
}

/// A command that runs `script` with `runtime`; the caller appends the user's arguments.
pub fn command(runtime: &RuntimeInfo, script: &Path) -> Command {
    let mut command = Command::new(&runtime.path);
    match runtime.id {
        RuntimeId::Pwsh | RuntimeId::WindowsPowershell => {
            command
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(script);
        }
        RuntimeId::Cmd => {
            command.args(["/d", "/c"]).arg(script);
        }
        RuntimeId::Bash | RuntimeId::Python => {
            command.arg(script);
        }
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_runtimes_and_skips_the_store_stub() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let touch = |rel: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"").unwrap();
            p
        };
        let stub_dir = root.join("Microsoft").join("WindowsApps");
        touch("Microsoft/WindowsApps/python.exe");
        touch("Microsoft/WindowsApps/python3");
        let env = Env {
            program_files: vec![root.join("pf")],
            system_root: Some(root.join("win")),
            local_app_data: None,
        };
        let found = detect_with(std::slice::from_ref(&stub_dir), &env);
        if cfg!(windows) {
            assert!(found.iter().all(|r| r.id != RuntimeId::Python), "{found:?}");
            touch("win/System32/WindowsPowerShell/v1.0/powershell.exe");
            touch("win/System32/cmd.exe");
            touch("pf/PowerShell/7/pwsh.exe");
            touch("pf/Git/bin/bash.exe");
            let py = touch("py/python.exe");
            let py = py.parent().unwrap().join("python.exe");
            let found = detect_with(&[stub_dir, py.parent().unwrap().to_path_buf()], &env);
            let ids: Vec<_> = found.iter().map(|r| r.id).collect();
            assert_eq!(ids, RuntimeId::ALL);
            assert_eq!(found[4].path, py.display().to_string());
        }
    }

    #[test]
    fn picks_runtimes_by_extension() {
        assert_eq!(for_extension(Path::new("a.PY")), Some(RuntimeId::Python));
        assert_eq!(for_extension(Path::new("a.bat")), Some(RuntimeId::Cmd));
        assert_eq!(for_extension(Path::new("a.sh")), Some(RuntimeId::Bash));
        assert_eq!(for_extension(Path::new("a.exe")), None);
    }
}
