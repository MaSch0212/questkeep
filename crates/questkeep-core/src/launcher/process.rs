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
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
        );
        system
            .processes()
            .values()
            .filter(|p| !matches!(p.status(), ProcessStatus::Zombie | ProcessStatus::Dead))
            .map(|p| ProcInfo {
                pid: p.pid().as_u32(),
                parent: p.parent().map(|pp| pp.as_u32()),
                exe: p.exe().map(|e| e.display().to_string()),
                name: p.name().to_string_lossy().into_owned(),
            })
            .collect()
    }
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
    /// The root process and every descendant seen so far; optionally also any process from a
    /// directory (covers launchers that start the game and exit before we see the child).
    Tree {
        tracked: HashSet<u32>,
        dir: Option<String>,
    },
    /// Only the original process.
    Original { pid: u32 },
    /// Any process whose executable lives under the directory (normalized).
    Directory { dir: String },
    /// Any process with this executable file name (case-insensitive).
    Name { name: String },
}

impl Matcher {
    pub fn tree(pid: u32, dir: Option<&str>) -> Self {
        Self::Tree {
            tracked: HashSet::from([pid]),
            dir: dir.map(normalize_dir),
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
            Self::Tree { tracked, dir } => {
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
                !tracked.is_empty() || dir.as_deref().is_some_and(|d| any_under_dir(snapshot, d))
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
            .as_deref()
            .is_some_and(|exe| normalize_path(exe).starts_with(dir))
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
        }
    }

    #[test]
    fn tree_follows_children_after_root_exits() {
        let mut m = Matcher::tree(10, None);
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
    fn tree_with_dir_catches_detached_children() {
        let mut m = Matcher::tree(10, Some("/games/Foo"));
        assert!(m.is_running(&[proc(10, Some(1), "/games/Foo/launcher")]));
        // Launcher exited before we saw the game; the game was reparented.
        assert!(m.is_running(&[proc(20, Some(1), "/games/Foo/bin/foo")]));
        assert!(!m.is_running(&[proc(21, Some(1), "/games/Foo2/x")]));
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
