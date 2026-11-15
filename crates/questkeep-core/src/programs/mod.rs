//! Programs installed on this PC, for picking the executable of a game added by hand: the
//! Windows "Apps" list and the Start menu's shortcuts.

pub mod lnk;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use questkeep_store_api::platform::UninstallEntry;

/// Where an installed program was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ProgramOrigin {
    /// The Windows "Apps" list (installed and uninstallable programs).
    Apps,
    StartMenu,
}

/// A program the user can add as a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstalledProgram {
    pub name: String,
    pub executable: String,
    pub args: Option<String>,
    pub working_dir: Option<String>,
    pub origin: ProgramOrigin,
}

/// Names of programs that are never games (runtimes, drivers, tools that come with others).
const NOT_GAMES: &[&str] = &[
    "redistributable",
    "runtime",
    "visual c++",
    ".net",
    "directx",
    "driver",
    "update for",
    "hotfix",
    "sdk",
    "vcredist",
    "physx",
    "uninstall",
    "readme",
    "documentation",
    "website",
    "license",
];

/// Executable names that start no game: uninstallers, installers, crash reporters, helpers.
const NOT_GAME_EXES: &[&str] = &[
    "unins",
    "uninst",
    "setup",
    "install",
    "crash",
    "report",
    "update",
    "helper",
    "redist",
    "vc_redist",
    "dxsetup",
    "unitycrashhandler",
    "ue4prereq",
    "dotnet",
    "notification",
    "cefprocess",
];

fn lower(text: &str) -> String {
    text.to_lowercase()
}

fn is_excluded_name(name: &str) -> bool {
    let name = lower(name);
    NOT_GAMES.iter().any(|word| name.contains(word))
}

/// Whether an executable can't be the game itself (by name or place).
fn is_excluded_exe(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return true;
    };
    let stem = lower(stem);
    let full = lower(&path.to_string_lossy()).replace('/', "\\");
    !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        || NOT_GAME_EXES.iter().any(|word| stem.contains(word))
        || full.contains("\\windowsapps\\")
        || full.contains("\\windows\\system32\\")
        || full.contains("\\windows\\syswow64\\")
        || full.contains("\\windows\\installer\\")
        || full.contains("\\package cache\\")
}

/// The path with `.` and `..` resolved (shortcuts often point at `..\..\Program Files\x.exe`)
/// and the platform's separators.
fn clean_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(part);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Letters and digits only, for comparing a program's name with its file names.
fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// `C:\x\game.exe,0` or `"C:\x\game.exe"` as a path.
fn icon_path(display_icon: &str) -> PathBuf {
    let trimmed = display_icon.trim();
    let without_index = match trimmed.rsplit_once(',') {
        Some((path, index)) if index.trim().parse::<i32>().is_ok() => path,
        _ => trimmed,
    };
    PathBuf::from(without_index.trim().trim_matches('"'))
}

/// The executable of an "Apps" entry: its icon when that is a usable executable, else the one
/// executable in its install folder (or the one named like the program).
fn entry_executable(entry: &UninstallEntry, is_file: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    if let Some(icon) = entry
        .display_icon
        .as_deref()
        .map(|i| clean_path(&icon_path(i)))
        && !is_excluded_exe(&icon)
        && is_file(&icon)
    {
        return Some(icon);
    }
    let folder = entry
        .install_location
        .as_deref()
        .map(|l| clean_path(Path::new(l.trim().trim_matches('"'))))
        .filter(|l| !l.as_os_str().is_empty())?;
    let candidates: Vec<PathBuf> = std::fs::read_dir(&folder)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !is_excluded_exe(p) && is_file(p))
        .collect();
    if let [only] = candidates.as_slice() {
        return Some(only.clone());
    }
    let name = squash(entry.display_name.as_deref().unwrap_or_default());
    // The executable named most like the program: the same name, else the longest that is part
    // of it (or contains it).
    candidates
        .into_iter()
        .filter_map(|p| {
            let stem = squash(&p.file_stem().unwrap_or_default().to_string_lossy());
            let matches = !stem.is_empty() && (name.contains(&stem) || stem.contains(&name));
            matches.then(|| ((stem == name, stem.len()), p))
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, p)| p)
}

/// An "Apps" entry as a program, when it has a name and a usable executable.
fn from_entry(entry: &UninstallEntry, is_file: &dyn Fn(&Path) -> bool) -> Option<InstalledProgram> {
    let name = entry.display_name.as_deref()?.trim();
    if name.is_empty() || is_excluded_name(name) {
        return None;
    }
    let exe = entry_executable(entry, is_file)?;
    Some(InstalledProgram {
        name: name.to_string(),
        working_dir: exe.parent().map(|p| p.display().to_string()),
        executable: exe.display().to_string(),
        args: None,
        origin: ProgramOrigin::Apps,
    })
}

/// A Start menu shortcut as a program, when it starts an executable that exists.
fn from_shortcut(
    path: &Path,
    shortcut: &lnk::Shortcut,
    is_file: &dyn Fn(&Path) -> bool,
) -> Option<InstalledProgram> {
    let name = path.file_stem()?.to_string_lossy().trim().to_string();
    if name.is_empty() || is_excluded_name(&name) {
        return None;
    }
    let target = match (&shortcut.target, &shortcut.relative_path) {
        (Some(target), _) => clean_path(Path::new(target)),
        (None, Some(relative)) => clean_path(&path.parent()?.join(relative)),
        (None, None) => return None,
    };
    if is_excluded_exe(&target) || !is_file(&target) {
        return None;
    }
    Some(InstalledProgram {
        name,
        executable: target.display().to_string(),
        args: shortcut.args.clone(),
        working_dir: shortcut
            .working_dir
            .clone()
            .or_else(|| target.parent().map(|p| p.display().to_string())),
        origin: ProgramOrigin::StartMenu,
    })
}

/// The `.lnk` files under the Start menu folders (a few levels deep).
fn start_menu_shortcuts(roots: &[PathBuf]) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 4;
    let mut found = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
            {
                found.push(path);
            }
        }
    }
    found
}

/// The Start menu's program folders: the user's and everyone's.
fn start_menu_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Some(data) = std::env::var_os("ProgramData") {
        roots.push(PathBuf::from(data).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    roots
}

/// One entry per executable (a Start menu shortcut wins: it may carry arguments), sorted by
/// name.
fn merge(programs: impl IntoIterator<Item = InstalledProgram>) -> Vec<InstalledProgram> {
    let mut by_exe: HashMap<String, InstalledProgram> = HashMap::new();
    for program in programs {
        let key = lower(&program.executable);
        match by_exe.get(&key) {
            Some(existing)
                if existing.origin == ProgramOrigin::StartMenu
                    || program.origin == ProgramOrigin::Apps => {}
            _ => {
                by_exe.insert(key, program);
            }
        }
    }
    let mut list: Vec<InstalledProgram> = by_exe.into_values().collect();
    list.sort_by_cached_key(|p| (lower(&p.name), lower(&p.executable)));
    list
}

/// Drops programs the library has already: a game's own executable, or anything inside a store
/// game's install folder. `rows` are (store, install folder, launch target) per game row.
pub fn not_in_library(
    programs: Vec<InstalledProgram>,
    rows: &[crate::db::InstallTarget],
) -> Vec<InstalledProgram> {
    let normalize = |p: &str| lower(p.trim()).replace('/', "\\");
    let mut targets = std::collections::HashSet::new();
    let mut folders = Vec::new();
    for (source, folder, target) in rows {
        if let Some(target) = target {
            targets.insert(normalize(target));
        }
        // A game added by hand has its exe's folder as install folder, which may hold others.
        if source != crate::model::MANUAL_SOURCE
            && let Some(folder) = folder.as_deref().map(normalize)
            && folder.len() > 3
        {
            folders.push(format!("{}\\", folder.trim_end_matches('\\')));
        }
    }
    programs
        .into_iter()
        .filter(|p| {
            let exe = normalize(&p.executable);
            !targets.contains(&exe) && !folders.iter().any(|f| exe.starts_with(f.as_str()))
        })
        .collect()
}

/// Size of the icons shown in the program list.
const LIST_ICON_SIZE: u32 = 32;

/// The icons of executables as `data:image/png` URLs, in order (`None` for a file without a
/// usable icon, and everywhere off Windows). Only `.exe` files are read.
pub fn executable_icons(paths: &[String]) -> Vec<Option<String>> {
    use base64::Engine as _;
    paths
        .iter()
        .map(|path| {
            let path = Path::new(path);
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                return None;
            }
            let icon = crate::platform::executable_icon(path, LIST_ICON_SIZE)?;
            let png =
                questkeep_imaging::icon_png_from_rgba(&icon, questkeep_imaging::SquareFrom::Icon)?;
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ))
        })
        .collect()
}

/// The programs on this PC (Windows only; empty elsewhere).
pub fn installed_programs() -> Vec<InstalledProgram> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let is_file = |p: &Path| p.is_file();
    let apps = questkeep_store_api::platform::uninstall_entries()
        .into_iter()
        .filter_map(|e| from_entry(&e, &is_file))
        .collect::<Vec<_>>();
    let shortcuts = start_menu_shortcuts(&start_menu_roots())
        .into_iter()
        .filter_map(|path| {
            let data = std::fs::read(&path).ok()?;
            from_shortcut(&path, &lnk::parse(&data)?, &is_file)
        });
    merge(apps.into_iter().chain(shortcuts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, icon: Option<&str>, location: Option<&str>) -> UninstallEntry {
        UninstallEntry {
            key: name.into(),
            display_name: Some(name.into()),
            display_icon: icon.map(str::to_string),
            install_location: location.map(str::to_string),
            ..UninstallEntry::default()
        }
    }

    #[test]
    fn apps_entries_use_their_icon_or_the_matching_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Into the Breach");
        std::fs::create_dir_all(&dir).unwrap();
        for exe in [
            "Breach.exe",
            "unins000.exe",
            "IntoTheBreach.exe",
            "notes.txt",
        ] {
            std::fs::write(dir.join(exe), b"x").unwrap();
        }
        let is_file = |p: &Path| p.is_file();
        let location = dir.display().to_string();

        // The icon wins when it is a real executable.
        let icon = format!("\"{}\",0", dir.join("Breach.exe").display());
        let program = from_entry(&entry("Into the Breach", Some(&icon), None), &is_file).unwrap();
        assert!(program.executable.ends_with("Breach.exe"));
        assert_eq!(program.origin, ProgramOrigin::Apps);

        // An uninstaller icon is skipped; of several executables the one named like the game.
        let uninstaller = dir.join("unins000.exe").display().to_string();
        let program = from_entry(
            &entry("Into the Breach", Some(&uninstaller), Some(&location)),
            &is_file,
        )
        .unwrap();
        assert!(program.executable.ends_with("IntoTheBreach.exe"));

        // Nothing named like it: no guess.
        assert_eq!(
            from_entry(&entry("Other", None, Some(&location)), &is_file),
            None
        );
        // Runtimes are never offered.
        let runtime = entry(
            "Microsoft Visual C++ 2015 Redistributable",
            Some(&icon),
            None,
        );
        assert_eq!(from_entry(&runtime, &is_file), None);
    }

    #[test]
    fn shortcuts_keep_arguments_and_skip_missing_or_helper_targets() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("Game.exe");
        std::fs::write(&exe, b"x").unwrap();
        let link = tmp.path().join("My Game.lnk");
        let is_file = |p: &Path| p.is_file();
        let shortcut = lnk::Shortcut {
            target: Some(exe.display().to_string()),
            args: Some("-launcher".into()),
            ..lnk::Shortcut::default()
        };
        let program = from_shortcut(&link, &shortcut, &is_file).unwrap();
        assert_eq!(program.name, "My Game");
        assert_eq!(program.args.as_deref(), Some("-launcher"));
        assert_eq!(
            program.working_dir.as_deref(),
            Some(tmp.path().display().to_string().as_str())
        );

        let missing = lnk::Shortcut {
            target: Some(tmp.path().join("gone.exe").display().to_string()),
            ..lnk::Shortcut::default()
        };
        assert_eq!(from_shortcut(&link, &missing, &is_file), None);
        let uninstall = tmp.path().join("Uninstall My Game.lnk");
        assert_eq!(from_shortcut(&uninstall, &shortcut, &is_file), None);
        let relative = lnk::Shortcut {
            relative_path: Some("Game.exe".into()),
            ..lnk::Shortcut::default()
        };
        assert!(from_shortcut(&link, &relative, &is_file).is_some());
    }

    #[test]
    fn cleans_dot_segments() {
        let path = Path::new("a").join("b").join("..").join(".").join("c.exe");
        assert_eq!(clean_path(&path), Path::new("a").join("c.exe"));
        assert_eq!(clean_path(Path::new("../x")), Path::new("../x"));
    }

    #[test]
    fn hides_programs_the_library_has() {
        let program = |exe: &str| InstalledProgram {
            name: exe.into(),
            executable: exe.into(),
            args: None,
            working_dir: None,
            origin: ProgramOrigin::Apps,
        };
        let rows = vec![
            (
                "manual".to_string(),
                Some(r"C:\Tools".to_string()),
                Some(r"c:\tools\a.exe".to_string()),
            ),
            (
                "steam".to_string(),
                Some(r"D:\Steam\common\Hades\".to_string()),
                None,
            ),
            ("epic".to_string(), Some(r"C:\".to_string()), None),
        ];
        let kept = not_in_library(
            vec![
                program(r"C:\Tools\A.exe"),
                program(r"C:\Tools\b.exe"),
                program(r"D:\Steam\common\Hades\x64\Hades.exe"),
                program(r"D:\Steam\common\Hades2\Hades2.exe"),
            ],
            &rows,
        );
        let names: Vec<&str> = kept.iter().map(|p| p.executable.as_str()).collect();
        assert_eq!(
            names,
            [r"C:\Tools\b.exe", r"D:\Steam\common\Hades2\Hades2.exe"]
        );
    }

    #[test]
    fn finds_shortcuts_in_nested_folders_and_merges_by_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("Studio").join("Game");
        std::fs::create_dir_all(&nested).unwrap();
        let exe = tmp.path().join("game.exe");
        std::fs::write(&exe, b"x").unwrap();
        let bytes = lnk::tests::shortcut_bytes(&exe.display().to_string(), "", "-x");
        std::fs::write(nested.join("Game.lnk"), &bytes).unwrap();
        std::fs::write(nested.join("readme.txt"), b"x").unwrap();
        let found = start_menu_shortcuts(&[tmp.path().to_path_buf()]);
        assert_eq!(found.len(), 1);
        let data = std::fs::read(&found[0]).unwrap();
        let from_menu =
            from_shortcut(&found[0], &lnk::parse(&data).unwrap(), &|p| p.is_file()).unwrap();

        let from_apps = InstalledProgram {
            name: "Game (Apps)".into(),
            executable: exe.display().to_string().to_uppercase(),
            args: None,
            working_dir: None,
            origin: ProgramOrigin::Apps,
        };
        let other = InstalledProgram {
            name: "another".into(),
            executable: "c:\\b.exe".into(),
            ..from_apps.clone()
        };
        let merged = merge([from_apps, from_menu, other]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].name, "another");
        assert_eq!(merged[1].origin, ProgramOrigin::StartMenu);
        assert_eq!(merged[1].args.as_deref(), Some("-x"));
    }
}
