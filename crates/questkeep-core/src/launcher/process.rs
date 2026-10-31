//! Process snapshots and running-state detection.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: process-tree tracking keeps
//! every descendant that was ever seen (so a launcher that spawns the game and exits keeps the
//! session alive), and directory tracking matches executables under a folder using a trailing
//! separator so `C:\Fallout` does not match `C:\Fallout 2`.

use std::collections::HashSet;
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
    path_hints(
        p.cwd().map(|c| c.display().to_string()),
        p.cmd().iter().map(|a| a.to_string_lossy().into_owned()),
    )
}

/// The working directory and path-like command-line arguments of a process.
#[cfg(target_os = "linux")]
pub(crate) fn path_hints(cwd: Option<String>, args: impl Iterator<Item = String>) -> Vec<String> {
    let mut paths: Vec<String> = cwd.into_iter().collect();
    for arg in args {
        // Wine maps the Unix root to drive Z:.
        if let Some(rest) = arg
            .strip_prefix("Z:\\")
            .or_else(|| arg.strip_prefix("z:\\"))
        {
            paths.push(format!("/{}", rest.replace('\\', "/")));
        } else if arg.starts_with('/') {
            paths.push(arg);
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
        // Split on both separators so Windows-style paths work on every OS.
        let file = name
            .rsplit(['/', '\\'])
            .find(|s| !s.is_empty())
            .unwrap_or(name);
        Self::Name {
            name: file.to_lowercase(),
        }
    }

    /// Updates internal state from a snapshot and reports whether the game is running.
    pub fn is_running(&mut self, snapshot: &[ProcInfo]) -> bool {
        self.presence(snapshot, &IgnoredPrograms::default()) == Presence::Game
    }

    /// Updates internal state from a snapshot and reports what of the game is running. Directory
    /// and tree matching tell `ignored` programs apart; the original-process and name modes follow
    /// exactly the process that was picked.
    pub fn presence(&mut self, snapshot: &[ProcInfo], ignored: &IgnoredPrograms) -> Presence {
        if let Self::Directory { dir } = self {
            return directory_presence(snapshot, dir, ignored);
        }
        let running = match self {
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
                if tracked.is_empty() {
                    return Presence::None;
                }
                let playing = snapshot.iter().any(|p| {
                    tracked.contains(&p.pid)
                        && !ignored.matches(p.exe.as_deref().unwrap_or(&p.name))
                });
                if !playing {
                    return Presence::Helpers;
                }
                true
            }
            Self::Original { pid } => snapshot.iter().any(|p| p.pid == *pid),
            Self::Directory { .. } => false,
            Self::Name { name } => snapshot.iter().any(|p| p.name.to_lowercase() == *name),
        };
        if running {
            Presence::Game
        } else {
            Presence::None
        }
    }
}

/// What of a game is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    None,
    /// Only ignored programs (launchers, crash handlers, installers, ...): the game may still be
    /// coming, but it is not being played.
    Helpers,
    Game,
}

fn directory_presence(snapshot: &[ProcInfo], dir: &str, ignored: &IgnoredPrograms) -> Presence {
    let mut presence = Presence::None;
    for p in snapshot {
        match ignored.classify(p.exe.iter().chain(p.hint_paths.iter()), dir) {
            Some(false) => return Presence::Game,
            Some(true) => presence = Presence::Helpers,
            None => {}
        }
    }
    presence
}

/// Programs that don't count as playing a game: built-in helpers that ship in game folders
/// without ever being the game (crash reporters, redistributable installers, uninstallers,
/// anti-cheat bootstrappers, a few known prelaunchers) plus the game's own list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnoredPrograms {
    /// The game's patterns: lowercase, without `.exe`, `*` as wildcard.
    patterns: Vec<String>,
}

impl IgnoredPrograms {
    /// `names` as the user entered them (`REDprelauncher.exe`, `*launcher*`).
    pub fn new(names: &[String]) -> Self {
        let patterns = names
            .iter()
            .map(|n| program_stem(n.trim()))
            .filter(|n| !n.is_empty() && n.chars().any(|c| c != '*'))
            .collect();
        Self { patterns }
    }

    /// Whether the program at `path` (or a bare file name) is ignored.
    pub fn matches(&self, path: &str) -> bool {
        let lower = path.replace('\\', "/").to_lowercase();
        let (folders, file) = lower.rsplit_once('/').unwrap_or(("", lower.as_str()));
        let stem = program_stem(file);
        is_builtin_helper(folders, &stem) || self.patterns.iter().any(|p| glob_match(p, &stem))
    }

    /// Whether one of a process's paths lies under the normalized `dir`: `Some(true)` when such
    /// a path is an ignored program, `Some(false)` when they are all part of the game.
    pub(crate) fn classify<'a>(
        &self,
        paths: impl IntoIterator<Item = &'a String>,
        dir: &str,
    ) -> Option<bool> {
        let mut found = None;
        for path in paths {
            if normalize_dir(path).starts_with(dir) {
                if self.matches(path) {
                    return Some(true);
                }
                found = Some(false);
            }
        }
        found
    }
}

/// A program's file name, lowercase and without `.exe`.
fn program_stem(name: &str) -> String {
    let file = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_lowercase();
    match file.strip_suffix(".exe") {
        Some(stem) => stem.to_string(),
        None => file,
    }
}

fn is_builtin_helper(folders: &str, stem: &str) -> bool {
    const FOLDERS: &[&str] = &[
        "_commonredist",
        "redist",
        "__installer",
        "easyanticheat",
        "battleye",
    ];
    const NAME_PARTS: &[&str] = &[
        "crashhandler",
        "crashreport",
        "crashpad",
        "crashsender",
        "vc_redist",
        "vcredist",
        "dxsetup",
        "dxwebsetup",
        "easyanticheat",
        "battleye",
        "beservice",
        "start_protected_game",
    ];
    // Prelaunchers known to start the game and stay open; "launcher" alone is too common in
    // real game executables to ignore.
    const NAMES: &[&str] = &["redprelauncher", "gtavlauncher", "playgtav"];
    folders.split('/').any(|f| FOLDERS.contains(&f))
        || NAME_PARTS.iter().any(|part| stem.contains(part))
        || NAMES.contains(&stem)
        || stem.starts_with("unins")
        || matches!(stem, "setup" | "installer")
}

/// Matches `text` against `pattern`, where `*` stands for any run of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        // No `*`: exact match.
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

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
    fn builtin_helpers_are_ignored() {
        let none = IgnoredPrograms::default();
        for helper in [
            r"D:\Steam\steamapps\common\Cyberpunk 2077\REDprelauncher.exe",
            r"C:\Games\GTAV\GTAVLauncher.exe",
            r"C:\Games\Foo\UnityCrashHandler64.exe",
            r"C:\Games\Foo\unins000.exe",
            r"C:\Games\Foo\_CommonRedist\vcredist\2019\VC_redist.x64.exe",
            r"C:\Games\Foo\EasyAntiCheat\EasyAntiCheat_EOS_Setup.exe",
            r"C:\Games\Foo\start_protected_game.exe",
        ] {
            assert!(none.matches(helper), "{helper}");
        }
        for game in [
            r"D:\Steam\steamapps\common\Cyberpunk 2077\bin\x64\Cyberpunk2077.exe",
            r"C:\Games\Crash Bandicoot\CrashBandicootNSaneTrilogy.exe",
            r"C:\Games\Foo\Launcher.exe",
            r"C:\Games\Foo\GameLauncher.exe",
            "/games/foo/game.x86_64",
        ] {
            assert!(!none.matches(game), "{game}");
        }
    }

    #[test]
    fn a_games_own_list_matches_names_and_wildcards() {
        let ignored = IgnoredPrograms::new(&[
            "Launcher.EXE".into(),
            "*patch*".into(),
            "  ".into(),
            "*".into(),
        ]);
        assert!(ignored.matches(r"C:\Games\Foo\launcher.exe"));
        assert!(ignored.matches("launcher"));
        assert!(ignored.matches("/games/foo/AutoPatcher.exe"));
        assert!(!ignored.matches(r"C:\Games\Foo\GameLauncher.exe"));
        assert!(!ignored.matches(r"C:\Games\Foo\Game.exe"));
        assert!(glob_match("a*b*c", "axxbyyc"));
        assert!(glob_match("a*", "a"));
        assert!(!glob_match("a*b", "ab_"));
        assert!(!glob_match("ab*ba", "aba"));
    }

    #[test]
    fn directory_presence_ignores_prelaunchers() {
        let none = IgnoredPrograms::default();
        let mut m = Matcher::directory("/games/Cyberpunk 2077");
        let launcher = proc(1, None, "/games/Cyberpunk 2077/REDprelauncher.exe");
        let game = proc(
            2,
            Some(1),
            "/games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe",
        );
        assert_eq!(m.presence(&[], &none), Presence::None);
        assert_eq!(
            m.presence(std::slice::from_ref(&launcher), &none),
            Presence::Helpers
        );
        assert!(!m.is_running(std::slice::from_ref(&launcher)));
        assert_eq!(m.presence(&[launcher, game], &none), Presence::Game);
    }

    #[test]
    fn tree_presence_ignores_the_games_launcher() {
        let ignored = IgnoredPrograms::new(&["Launcher.exe".into()]);
        let launcher = proc(10, Some(1), "/g/Launcher.exe");
        let game = proc(11, Some(10), "/g/Game.exe");
        let mut m = Matcher::tree(10);
        assert_eq!(
            m.presence(std::slice::from_ref(&launcher), &ignored),
            Presence::Helpers
        );
        assert_eq!(
            m.presence(&[launcher.clone(), game], &ignored),
            Presence::Game
        );
        // The game exits, the launcher stays.
        assert_eq!(
            m.presence(std::slice::from_ref(&launcher), &ignored),
            Presence::Helpers
        );
        // Without the list the launcher is the game.
        assert!(Matcher::tree(10).is_running(&[launcher]));
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
