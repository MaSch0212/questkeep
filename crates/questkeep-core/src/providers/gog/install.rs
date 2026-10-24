//! Installed-game detection for GOG: turns install records (from the Windows registry or any
//! other source) plus the `goggame-<id>.info` files in the install folders into
//! [`ImportedGame`]s. Pure data in, data out, so it is fully testable without a registry.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::path::{Component, MAIN_SEPARATOR, MAIN_SEPARATOR_STR, Path, PathBuf};

use serde::Deserialize;

use crate::model::{ActionKind, ImportedGame, PlayAction, TrackingMode};

/// One install record, as written by the GOG installers to
/// `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>`. Every field is optional except the id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallRecord {
    /// The registry key name (the GOG product id).
    pub id: String,
    pub game_name: Option<String>,
    pub path: Option<String>,
    /// Full path of the main executable (`exe`).
    pub exe: Option<String>,
    pub launch_param: Option<String>,
    pub working_dir: Option<String>,
    /// Set for DLC and add-ons: the id of the game they extend.
    pub depends_on: Option<String>,
    /// `UninstallString` of the product's `Uninstall\<id>_is1` key (the Inno Setup uninstaller,
    /// quoted path plus optional arguments).
    pub uninstall_string: Option<String>,
}

/// One `...\Windows\CurrentVersion\Uninstall\<id>_is1` key of a GOG installer, with the numeric
/// product id already extracted from the key name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallEntry {
    pub id: String,
    pub display_name: Option<String>,
    pub uninstall_string: Option<String>,
    pub install_location: Option<String>,
}

/// The numeric GOG product id of an Inno Setup key name (`1423049311_is1`). Other installers use
/// the same suffix (`Git_is1`, `{GUID}_is1`), so anything that is not all digits is rejected.
pub fn product_id_of_uninstall_key(key_name: &str) -> Option<&str> {
    let id = key_name.strip_suffix("_is1")?;
    (!id.is_empty() && id.len() <= 20 && id.chars().all(|c| c.is_ascii_digit())).then_some(id)
}

/// Adds what the `Uninstall` keys know to the records of `GOG.com\Games`: the uninstaller of
/// known products, and products that GOG's own key does not list at all (their `.info` file then
/// decides whether they are a game, an add-on or something else).
pub fn merge_uninstall_entries(records: &mut Vec<InstallRecord>, entries: Vec<UninstallEntry>) {
    for entry in entries {
        let location = non_empty(&entry.install_location)
            .map(str::to_string)
            .or_else(|| {
                let (file, _) = split_command(entry.uninstall_string.as_deref()?);
                file.parent().map(|p| p.to_string_lossy().into_owned())
            });
        match records.iter_mut().find(|r| r.id == entry.id) {
            Some(record) => {
                if record.uninstall_string.is_none() {
                    record.uninstall_string = entry.uninstall_string;
                }
                if non_empty(&record.path).is_none() {
                    record.path = location;
                }
            }
            None => records.push(InstallRecord {
                id: entry.id,
                game_name: entry.display_name,
                path: location,
                uninstall_string: entry.uninstall_string,
                ..InstallRecord::default()
            }),
        }
    }
}

/// Splits a registry command line into the program and its arguments: `"C:\a b\x.exe" /silent`
/// or an unquoted `C:\a b\x.exe /silent`.
pub fn split_command(command: &str) -> (PathBuf, Option<String>) {
    let command = command.trim();
    let (file, rest) = if let Some(quoted) = command.strip_prefix('"') {
        match quoted.split_once('"') {
            Some((file, rest)) => (file, rest),
            None => (quoted, ""),
        }
    } else {
        match command.to_ascii_lowercase().find(".exe") {
            Some(end) => command.split_at(end + 4),
            None => (command, ""),
        }
    };
    let args = rest.trim();
    (
        PathBuf::from(file),
        (!args.is_empty()).then(|| args.to_string()),
    )
}

/// The parts of `goggame-<id>.info` that Questkeep uses.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub root_game_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub play_tasks: Vec<PlayTask>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayTask {
    #[serde(default)]
    pub is_primary: bool,
    /// `FileTask` or `URLTask`.
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub arguments: String,
    #[serde(default)]
    pub link: String,
}

impl GameInfo {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        // Some installers write a UTF-8 BOM.
        serde_json::from_str(json.trim_start_matches('\u{feff}'))
    }

    fn primary_task(&self) -> Option<&PlayTask> {
        self.play_tasks.iter().find(|t| t.is_primary)
    }
}

/// Reads and parses `<dir>/goggame-<id>.info`. `None` when missing or malformed.
pub(super) fn read_info(dir: &Path, id: &str) -> Option<GameInfo> {
    let file = dir.join(format!("goggame-{id}.info"));
    let text = std::fs::read_to_string(&file).ok()?;
    match GameInfo::parse(&text) {
        Ok(info) => Some(info),
        Err(err) => {
            tracing::warn!("ignoring malformed GOG manifest {}: {err}", file.display());
            None
        }
    }
}

/// Converts `\` and `/` in a path coming from a manifest to the platform separator.
fn fix_separators(path: &str) -> String {
    path.replace(['\\', '/'], MAIN_SEPARATOR_STR)
}

/// Joins a manifest-relative path onto the install dir. Absolute paths (and paths that escape the
/// install dir via `..`) are rejected.
fn join_relative(dir: &Path, rel: &str) -> Option<PathBuf> {
    let rel = fix_separators(rel);
    let rel = Path::new(rel.trim_start_matches(MAIN_SEPARATOR));
    if rel.as_os_str().is_empty()
        || rel.has_root()
        || rel
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return None;
    }
    Some(dir.join(rel))
}

/// Strips trademark signs that GOG appends to some titles.
fn clean_name(name: &str) -> String {
    name.replace(['\u{2122}', '\u{00ae}'], "")
        .trim()
        .to_string()
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Builds the Galaxy launch action. `/launchViaAutostart` starts the game without bringing the
/// Galaxy window to the front; Galaxy exits again when it was not running before.
fn galaxy_action(client: &Path, id: &str, dir: &Path) -> PlayAction {
    let dir_text = dir.to_string_lossy().into_owned();
    PlayAction {
        kind: ActionKind::File,
        target: client.to_string_lossy().into_owned(),
        args: Some(format!(
            "/launchViaAutostart /gameId={id} /command=runGame /path=\"{dir_text}\""
        )),
        working_dir: client.parent().map(|p| p.to_string_lossy().into_owned()),
        // The started process is only the launcher; the game runs from the install dir.
        tracking: TrackingMode::Directory,
        tracking_path: Some(dir_text),
    }
}

/// The direct launch action from the manifest's primary task, or from registry data when the
/// manifest is unusable.
fn direct_action(
    record: &InstallRecord,
    dir: &Path,
    info: Option<&GameInfo>,
) -> Option<PlayAction> {
    if let Some(task) = info.and_then(GameInfo::primary_task) {
        if task.kind.eq_ignore_ascii_case("URLTask") {
            if task.link.trim().is_empty() {
                return None;
            }
            return Some(PlayAction::uri(task.link.trim()));
        }
        let exe = join_relative(dir, &task.path)?;
        let working_dir = if task.working_dir.trim().is_empty() {
            dir.to_path_buf()
        } else {
            join_relative(dir, &task.working_dir).unwrap_or_else(|| dir.to_path_buf())
        };
        return Some(PlayAction {
            args: Some(task.arguments.trim().to_string()).filter(|a| !a.is_empty()),
            working_dir: Some(working_dir.to_string_lossy().into_owned()),
            ..PlayAction::file(exe.to_string_lossy().into_owned())
        });
    }
    let exe = non_empty(&record.exe)?;
    let working_dir = non_empty(&record.working_dir)
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.to_path_buf());
    Some(PlayAction {
        args: non_empty(&record.launch_param).map(str::to_string),
        working_dir: Some(working_dir.to_string_lossy().into_owned()),
        ..PlayAction::file(fix_separators(exe))
    })
}

/// Turns install records into games. Skips DLC, entries whose folder is gone and entries without
/// anything launchable. `galaxy_client` is the path of `GalaxyClient.exe` when Galaxy is installed
/// and `use_galaxy` is set; Galaxy then launches the games.
pub fn games_from_records(
    records: &[InstallRecord],
    galaxy_client: Option<&Path>,
    use_galaxy: bool,
) -> Vec<ImportedGame> {
    let mut games: Vec<ImportedGame> = Vec::new();
    for record in records {
        let id = record.id.trim();
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Some(parent) = non_empty(&record.depends_on)
            && parent != id
        {
            continue; // DLC
        }
        let Some(dir) = non_empty(&record.path).map(|p| PathBuf::from(fix_separators(p))) else {
            continue;
        };
        if !dir.is_dir() {
            continue;
        }
        let info = read_info(&dir, id);
        if let Some(info) = &info {
            if !info.root_game_id.is_empty() && info.root_game_id != id {
                continue; // DLC
            }
            if !info.game_id.is_empty() && info.game_id != id {
                continue;
            }
        }
        let direct = direct_action(record, &dir, info.as_ref());
        // No play task and no registry exe means an add-on (Playnite treats these as DLC too).
        if direct.is_none() {
            continue;
        }
        let name = info
            .as_ref()
            .map(|i| clean_name(&i.name))
            .filter(|n| !n.is_empty())
            .or_else(|| {
                non_empty(&record.game_name)
                    .map(clean_name)
                    .filter(|n| !n.is_empty())
            });
        let Some(name) = name else { continue };
        let play_action = match galaxy_client {
            Some(client) if use_galaxy => Some(galaxy_action(client, id, &dir)),
            _ => direct,
        };
        if games.iter().any(|g| g.source_game_id == id) {
            continue;
        }
        games.push(ImportedGame {
            install_dir: Some(dir.to_string_lossy().into_owned()),
            play_action,
            ..ImportedGame::installed(id, name)
        });
    }
    games
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const INFO: &str = include_str!("../../../tests/fixtures/gog/goggame-1207658924.info");

    fn record(id: &str, dir: &Path) -> InstallRecord {
        InstallRecord {
            id: id.into(),
            game_name: Some("Registry Name".into()),
            path: Some(dir.to_string_lossy().into_owned()),
            ..Default::default()
        }
    }

    fn installed(tmp: &Path, id: &str, info: Option<&str>) -> PathBuf {
        let dir = tmp.join(format!("game{id}"));
        fs::create_dir_all(&dir).unwrap();
        if let Some(info) = info {
            fs::write(dir.join(format!("goggame-{id}.info")), info).unwrap();
        }
        dir
    }

    #[test]
    fn parses_info_and_picks_primary_task() {
        let info = GameInfo::parse(INFO).unwrap();
        assert_eq!(info.root_game_id, "1207658924");
        assert_eq!(info.play_tasks.len(), 2);
        assert_eq!(
            info.primary_task().unwrap().path.replace('\\', "/"),
            "System/witcher.exe"
        );
        assert!(GameInfo::parse(&format!("\u{feff}{INFO}")).is_ok());
        assert!(GameInfo::parse("{ nope").is_err());
    }

    #[test]
    fn direct_launch_from_primary_task() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = installed(tmp.path(), "1207658924", Some(INFO));
        let games = games_from_records(&[record("1207658924", &dir)], None, true);
        assert_eq!(games.len(), 1);
        let game = &games[0];
        assert_eq!(game.name, "The Witcher: Enhanced Edition");
        assert_eq!(game.source_game_id, "1207658924");
        assert!(game.installed);
        let action = game.play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::File);
        assert_eq!(
            PathBuf::from(&action.target),
            dir.join("System").join("witcher.exe")
        );
        assert_eq!(action.args.as_deref(), Some("-fullscreen"));
        assert_eq!(
            PathBuf::from(action.working_dir.as_ref().unwrap()),
            dir.join("System")
        );
        assert_eq!(action.tracking, TrackingMode::Default);
    }

    #[test]
    fn galaxy_launch_tracks_install_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = installed(tmp.path(), "1207658924", Some(INFO));
        let client = tmp.path().join("Galaxy").join("GalaxyClient.exe");
        let games = games_from_records(&[record("1207658924", &dir)], Some(&client), true);
        let action = games[0].play_action.as_ref().unwrap();
        assert_eq!(PathBuf::from(&action.target), client);
        let args = action.args.as_ref().unwrap();
        assert!(args.starts_with("/launchViaAutostart /gameId=1207658924 /command=runGame"));
        assert!(args.contains(&format!("/path=\"{}\"", dir.display())));
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(
            action.tracking_path.as_deref(),
            Some(dir.to_string_lossy().as_ref())
        );

        // Galaxy launching can be switched off.
        let games = games_from_records(&[record("1207658924", &dir)], Some(&client), false);
        assert!(
            games[0]
                .play_action
                .as_ref()
                .unwrap()
                .target
                .ends_with("witcher.exe")
        );
    }

    #[test]
    fn skips_dlc_and_missing_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let game_dir = installed(
            tmp.path(),
            "1",
            Some(INFO.replace("1207658924", "1").as_str()),
        );
        let mut dlc_by_registry = record("2", &game_dir);
        dlc_by_registry.depends_on = Some("1".into());
        let dlc_info = INFO
            .replace("\"gameId\": \"1207658924\"", "\"gameId\": \"3\"")
            .replace("\"rootGameId\": \"1207658924\"", "\"rootGameId\": \"1\"");
        let dlc_dir = installed(tmp.path(), "3", Some(&dlc_info));
        let gone = tmp.path().join("gone");
        let records = [
            record("1", &game_dir),
            dlc_by_registry,
            record("3", &dlc_dir),
            record("4", &gone),
            InstallRecord {
                id: "5".into(),
                ..Default::default()
            },
            record("not-a-number", &game_dir),
        ];
        let games = games_from_records(&records, None, true);
        let ids: Vec<_> = games.iter().map(|g| g.source_game_id.as_str()).collect();
        assert_eq!(ids, ["1"]);
    }

    #[test]
    fn depends_on_itself_is_not_dlc() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = installed(tmp.path(), "1207658924", Some(INFO));
        let mut rec = record("1207658924", &dir);
        rec.depends_on = Some("1207658924".into());
        assert_eq!(games_from_records(&[rec], None, true).len(), 1);
    }

    #[test]
    fn malformed_or_missing_manifest_falls_back_to_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = installed(tmp.path(), "10", Some("{ not json"));
        let none = installed(tmp.path(), "11", None);
        let mut a = record("10", &bad);
        a.exe = Some(bad.join("game.exe").to_string_lossy().into_owned());
        a.launch_param = Some("-windowed".into());
        let b = record("11", &none); // no exe either: nothing launchable
        let games = games_from_records(&[a, b], None, true);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Registry Name");
        let action = games[0].play_action.as_ref().unwrap();
        assert!(action.target.ends_with("game.exe"));
        assert_eq!(action.args.as_deref(), Some("-windowed"));
    }

    #[test]
    fn manifest_without_primary_task_is_an_addon() {
        let tmp = tempfile::tempdir().unwrap();
        let info = r#"{"gameId":"7","rootGameId":"7","name":"Soundtrack","playTasks":[{"name":"x","path":"a.exe","type":"FileTask"}]}"#;
        let dir = installed(tmp.path(), "7", Some(info));
        assert!(games_from_records(&[record("7", &dir)], None, true).is_empty());
    }

    #[test]
    fn url_task_becomes_uri_action_and_paths_cannot_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let info = r#"{"gameId":"8","name":"Web","playTasks":[{"isPrimary":true,"type":"URLTask","link":"https://example.com/play"}]}"#;
        let dir = installed(tmp.path(), "8", Some(info));
        let games = games_from_records(&[record("8", &dir)], None, true);
        let action = games[0].play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(action.target, "https://example.com/play");

        let evil = r#"{"gameId":"9","name":"Evil","playTasks":[{"isPrimary":true,"type":"FileTask","path":"..\\..\\evil.exe"}]}"#;
        let dir = installed(tmp.path(), "9", Some(evil));
        assert!(games_from_records(&[record("9", &dir)], None, true).is_empty());
    }

    #[test]
    fn only_numeric_inno_keys_are_gog_products() {
        assert_eq!(
            product_id_of_uninstall_key("1423049311_is1"),
            Some("1423049311")
        );
        for other in [
            "Git_is1",
            "{0A1B2C3D-0000-1111-2222-333344445555}_is1",
            "Playnite_is1",
            "_is1",
            "123",
            "12a_is1",
        ] {
            assert_eq!(product_id_of_uninstall_key(other), None, "{other}");
        }
    }

    #[test]
    fn splits_registry_command_lines() {
        let (file, args) = split_command(r#""F:\GOG Games\Cyberpunk 2077\unins000.exe""#);
        assert_eq!(
            file,
            PathBuf::from(r"F:\GOG Games\Cyberpunk 2077\unins000.exe")
        );
        assert_eq!(args, None);
        let (file, args) = split_command(r#" "C:\a b\unins000.exe" /SILENT /LOG="x y" "#);
        assert_eq!(file, PathBuf::from(r"C:\a b\unins000.exe"));
        assert_eq!(args.as_deref(), Some(r#"/SILENT /LOG="x y""#));
        let (file, args) = split_command(r"C:\a b\UNINS000.EXE /VERYSILENT");
        assert_eq!(file, PathBuf::from(r"C:\a b\UNINS000.EXE"));
        assert_eq!(args.as_deref(), Some("/VERYSILENT"));
    }

    fn entry(id: &str, name: &str, dir: &str, n: u8) -> UninstallEntry {
        UninstallEntry {
            id: id.into(),
            display_name: Some(name.into()),
            uninstall_string: Some(format!("\"{dir}\\unins00{n}.exe\"")),
            install_location: Some(format!("{dir}\\")),
        }
    }

    /// Installs that only the `Uninstall` keys know become games; add-ons in the same folder do
    /// not, and known records only gain the uninstaller.
    #[test]
    fn uninstall_keys_add_missing_installs_but_not_dlc() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Cyberpunk 2077");
        fs::create_dir_all(&dir).unwrap();
        let info = |id: &str, root: &str, tasks: &str| {
            format!(
                r#"{{"gameId":"{id}","rootGameId":"{root}","name":"Cyberpunk {id}","playTasks":[{tasks}]}}"#
            )
        };
        let play = r#"{"isPrimary":true,"type":"FileTask","path":"REDprelauncher.exe"}"#;
        for (id, root, tasks) in [
            ("1423049311", "1423049311", play),
            ("1256837418", "1423049311", ""),
            ("1597316373", "1423049311", ""),
        ] {
            fs::write(
                dir.join(format!("goggame-{id}.info")),
                info(id, root, tasks),
            )
            .unwrap();
        }
        let dir_text = dir.to_string_lossy().into_owned();
        let entries = vec![
            entry("1423049311", "Cyberpunk 2077", &dir_text, 0),
            entry(
                "1256837418",
                "Cyberpunk 2077: Phantom Liberty",
                &dir_text,
                1,
            ),
            entry("1597316373", "Cyberpunk 2077 REDmod", &dir_text, 2),
            UninstallEntry {
                id: "1423049311".into(),
                uninstall_string: Some("\"duplicate.exe\"".into()),
                ..UninstallEntry::default()
            },
        ];

        let mut records = Vec::new();
        merge_uninstall_entries(&mut records, entries.clone());
        assert_eq!(records.len(), 3);
        assert_eq!(
            records[0].uninstall_string.as_deref(),
            Some(format!("\"{dir_text}\\unins000.exe\"").as_str()),
            "the first key of an id wins"
        );
        let games = games_from_records(&records, None, false);
        assert_eq!(games.len(), 1, "add-ons do not become games");
        assert_eq!(games[0].source_game_id, "1423049311");
        assert_eq!(games[0].name, "Cyberpunk 1423049311");

        // A record from GOG.com\Games keeps its fields and gains the uninstaller and path.
        let mut known = vec![InstallRecord {
            id: "1423049311".into(),
            exe: Some("x.exe".into()),
            ..Default::default()
        }];
        merge_uninstall_entries(&mut known, entries);
        assert_eq!(known.len(), 3);
        assert_eq!(known[0].exe.as_deref(), Some("x.exe"));
        assert!(known[0].uninstall_string.is_some());
        assert!(known[0].path.as_deref().unwrap().starts_with(&dir_text));
    }

    #[test]
    fn missing_install_location_comes_from_the_uninstaller_path() {
        let mut records = Vec::new();
        merge_uninstall_entries(
            &mut records,
            vec![UninstallEntry {
                id: "5".into(),
                uninstall_string: Some(r#""F:\G\Game\unins000.exe""#.into()),
                ..UninstallEntry::default()
            }],
        );
        assert_eq!(
            PathBuf::from(records[0].path.as_deref().unwrap()),
            PathBuf::from(r"F:\G\Game")
        );
    }

    #[test]
    fn trademark_signs_are_removed() {
        assert_eq!(clean_name(" Game\u{2122} \u{00ae}"), "Game");
    }
}
