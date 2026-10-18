//! Process snapshots and running-state detection.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: process-tree tracking keeps
//! every descendant that was ever seen (so a launcher that spawns the game and exits keeps the
//! session alive), and directory tracking matches executables under a folder using a trailing
//! separator so `C:\Fallout` does not match `C:\Fallout 2`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use sysinfo::{ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};

/// Minimal information about one running process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    /// Full executable path when the OS allows reading it.
    pub exe: Option<String>,
    /// Executable file name.
    pub name: String,
    /// Other paths that identify the process (working directory, path-like command-line
    /// arguments). Filled on Linux only, where Proton games run as `wine64-preloader` from the
    /// Proton folder and only their working directory/arguments point into the game folder.
    pub hint_paths: Vec<String>,
}

/// Source of process snapshots. Abstracted so tracking logic is testable without real processes.
pub trait ProcessProbe: Send + Sync {
    fn snapshot(&self) -> Vec<ProcInfo>;
}

/// Real probe backed by `sysinfo` (Windows and Linux).
pub struct SystemProbe {
    system: Mutex<System>,
}

impl SystemProbe {
    pub fn new() -> Self {
        Self {
            system: Mutex::new(System::new()),
        }
    }
}

impl Default for SystemProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessProbe for SystemProbe {
    fn snapshot(&self) -> Vec<ProcInfo> {
        let mut system = self.system.lock().unwrap_or_else(PoisonError::into_inner);
        let kind = ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet);
        #[cfg(target_os = "linux")]
        let kind = kind
            .with_cwd(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet);
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        system
            .processes()
            .values()
            .filter(|p| !matches!(p.status(), ProcessStatus::Zombie | ProcessStatus::Dead))
            .map(|p| ProcInfo {
                pid: p.pid().as_u32(),
                parent: p.parent().map(|pp| pp.as_u32()),
                exe: p.exe().map(|e| e.display().to_string()),
                name: p.name().to_string_lossy().into_owned(),
                hint_paths: hint_paths(p),
            })
            .collect()
    }
}

#[cfg(target_os = "linux")]
fn hint_paths(p: &sysinfo::Process) -> Vec<String> {
    let mut paths: Vec<String> = p
        .cwd()
        .map(|c| c.display().to_string())
        .into_iter()
        .collect();
    for arg in p.cmd() {
        let arg = arg.to_string_lossy();
        // Wine maps the Unix root to drive Z:.
        if let Some(rest) = arg
            .strip_prefix("Z:\\")
            .or_else(|| arg.strip_prefix("z:\\"))
        {
            paths.push(format!("/{}", rest.replace('\\', "/")));
        } else if arg.starts_with('/') {
            paths.push(arg.into_owned());
        }
    }
    paths
}

#[cfg(not(target_os = "linux"))]
fn hint_paths(_: &sysinfo::Process) -> Vec<String> {
    Vec::new()
}

/// Normalizes a path for prefix comparison: unified separators, trailing separator, and
/// case-folded on Windows (where the file system is case-insensitive).
pub fn normalize_dir(dir: &str) -> String {
    let mut s = normalize_path(dir);
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

fn normalize_path(path: &str) -> String {
    let s = path.replace('\\', "/");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// Decides whether a game is running based on process snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher {
    /// The root process and every descendant seen so far.
    Tree { tracked: HashSet<u32> },
    /// Only the original process.
    Original { pid: u32 },
    /// Any process whose executable lives under the directory (normalized).
    Directory { dir: String },
    /// Any process with this executable file name (case-insensitive).
    Name { name: String },
}

impl Matcher {
    pub fn tree(pid: u32) -> Self {
        Self::Tree {
            tracked: HashSet::from([pid]),
        }
    }

    pub fn directory(dir: &str) -> Self {
        Self::Directory {
            dir: normalize_dir(dir),
        }
    }

    pub fn name(name: &str) -> Self {
        let file = Path::new(name)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.to_string());
        Self::Name {
            name: file.to_lowercase(),
        }
    }

    /// Updates internal state from a snapshot and reports whether the game is running.
    pub fn is_running(&mut self, snapshot: &[ProcInfo]) -> bool {
        match self {
            Self::Tree { tracked } => {
                // Adopt children until no new descendant appears (handles multi-level spawns
                // within one polling interval).
                loop {
                    let before = tracked.len();
                    for p in snapshot {
                        if p.parent.is_some_and(|parent| tracked.contains(&parent)) {
                            tracked.insert(p.pid);
                        }
                    }
                    if tracked.len() == before {
                        break;
                    }
                }
                tracked.retain(|pid| snapshot.iter().any(|p| p.pid == *pid));
                !tracked.is_empty()
            }
            Self::Original { pid } => snapshot.iter().any(|p| p.pid == *pid),
            Self::Directory { dir } => any_under_dir(snapshot, dir),
            Self::Name { name } => snapshot.iter().any(|p| p.name.to_lowercase() == *name),
        }
    }
}

fn any_under_dir(snapshot: &[ProcInfo], dir: &str) -> bool {
    snapshot.iter().any(|p| {
        p.exe
            .iter()
            .chain(p.hint_paths.iter())
            .any(|path| normalize_dir(path).starts_with(dir))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: Option<u32>, exe: &str) -> ProcInfo {
        ProcInfo {
            pid,
            parent,
            exe: Some(exe.to_string()),
            name: Path::new(exe)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            hint_paths: Vec::new(),
        }
    }

    #[test]
    fn directory_matches_hint_paths() {
        let mut m = Matcher::directory("/home/u/.steam/steamapps/common/Game");
        let mut wine = proc(
            7,
            None,
            "/home/u/.steam/steamapps/common/Proton 9.0/files/bin/wine64-preloader",
        );
        assert!(!m.is_running(std::slice::from_ref(&wine)));
        wine.hint_paths = vec!["/home/u/.steam/steamapps/common/Game".into()];
        assert!(m.is_running(&[wine]));
    }

    #[test]
    fn tree_follows_children_after_root_exits() {
        let mut m = Matcher::tree(10);
        // Launcher 10 spawns 11, which spawns 12 within one interval.
        let snap = [
            proc(10, Some(1), "/g/launcher"),
            proc(11, Some(10), "/g/game"),
            proc(12, Some(11), "/g/helper"),
            proc(99, Some(1), "/other"),
        ];
        assert!(m.is_running(&snap));
        // Launcher and helper exit; game keeps running.
        assert!(m.is_running(&[proc(11, Some(10), "/g/game"), proc(99, Some(1), "/other")]));
        // Game exits.
        assert!(!m.is_running(&[proc(99, Some(1), "/other")]));
    }

    #[test]
    fn directory_requires_separator_boundary() {
        let mut m = Matcher::directory("/games/Fallout");
        assert!(!m.is_running(&[proc(1, None, "/games/Fallout 2/fallout2")]));
        assert!(m.is_running(&[proc(1, None, "/games/Fallout/fallout")]));
    }

    #[cfg(windows)]
    #[test]
    fn directory_is_case_insensitive_on_windows() {
        let mut m = Matcher::directory(r"C:\Games\Portal 2\");
        assert!(m.is_running(&[proc(1, None, r"c:\games\portal 2\portal2.exe")]));
    }

    #[test]
    fn original_and_name_matchers() {
        let mut o = Matcher::Original { pid: 5 };
        assert!(o.is_running(&[proc(5, None, "/a")]));
        assert!(!o.is_running(&[proc(6, Some(5), "/a")]));

        let mut n = Matcher::name(r"C:\x\Game.EXE");
        assert!(n.is_running(&[proc(1, None, "/elsewhere/game.exe")]));
        assert!(!n.is_running(&[proc(1, None, "/elsewhere/other.exe")]));
    }

    #[test]
    fn system_probe_sees_current_process() {
        let me = std::process::id();
        let snap = SystemProbe::new().snapshot();
        assert!(snap.iter().any(|p| p.pid == me));
    }
}
