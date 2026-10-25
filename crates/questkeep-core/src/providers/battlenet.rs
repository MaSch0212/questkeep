//! Battle.net provider, Windows only.
//!
//! The Battle.net agent keeps its installs in `%ProgramData%\Battle.net\Agent\product.db`, a
//! protobuf file: repeated field 1 is one install with the install id (`1`, e.g. `hs_beta`), the
//! product code (`2`, e.g. `hsb`), the settings (`3`, whose field 1 is the install folder) and the
//! cached state (`4`, whose field 1 holds `installed` and `playable`). An install that is still
//! downloading has `installed` = 0 and is skipped. Next to it, `aggregate.json` lists the
//! installed games the way the Battle.net app shows them: name, `battlenet://` launch URI, box
//! art and the executable used for the icon. The agent rewrites both files often, so reads are
//! retried briefly.
//!
//! Games start through `battlenet://game/<product code>` and uninstall through the "Blizzard
//! Uninstaller" command of the game's Windows "Apps" entry (`--uid=<install id>`).
//!
//! Owned games that are not installed would need a Battle.net web sign-in; not done yet.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::DateTime;
use serde::Deserialize;

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::model::{ImportedGame, PlayAction};

pub const BATTLENET_SOURCE: &str = "battlenet";
/// Product codes of the agent and the Battle.net app itself.
const NOT_GAMES: [&str; 2] = ["agent", "bna"];

/// One install from `product.db`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductInstall {
    /// Install id, e.g. `hs_beta`; the uninstaller's `--uid`.
    pub uid: String,
    /// Product code, e.g. `hsb`; the game's id in Questkeep and in `battlenet://game/`.
    pub product_code: String,
    pub install_path: Option<String>,
    pub installed: bool,
}

/// One entry of `aggregate.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct AggregateGame {
    pub product_id: String,
    pub name: Option<String>,
    pub launch_uri: Option<String>,
    pub box_art_uri: Option<String>,
    /// The executable the Battle.net app takes the game's icon from.
    pub icon_path: Option<String>,
    /// Unix seconds; 0 when never played.
    pub last_played_timestamp: i64,
}

#[derive(Deserialize)]
struct Aggregate {
    #[serde(default)]
    installed: Vec<AggregateGame>,
}

/// Minimal protobuf reading: the fields of one message as (number, value).
enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

fn read_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn fields(data: &[u8]) -> Option<Vec<(u64, Field<'_>)>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let key = read_varint(data, &mut pos)?;
        let field = match key & 7 {
            0 => Field::Varint(read_varint(data, &mut pos)?),
            1 => {
                pos = pos.checked_add(8)?;
                continue;
            }
            2 => {
                let len = usize::try_from(read_varint(data, &mut pos)?).ok()?;
                let end = pos.checked_add(len)?;
                let bytes = data.get(pos..end)?;
                pos = end;
                Field::Bytes(bytes)
            }
            5 => {
                pos = pos.checked_add(4)?;
                continue;
            }
            _ => return None,
        };
        out.push((key >> 3, field));
    }
    (pos == data.len()).then_some(out)
}

fn bytes_field<'a>(fields: &[(u64, Field<'a>)], number: u64) -> Option<&'a [u8]> {
    fields.iter().find_map(|(n, f)| match f {
        Field::Bytes(b) if *n == number => Some(*b),
        _ => None,
    })
}

fn varint_field(fields: &[(u64, Field<'_>)], number: u64) -> Option<u64> {
    fields.iter().find_map(|(n, f)| match f {
        Field::Varint(v) if *n == number => Some(*v),
        _ => None,
    })
}

fn string_field(fields: &[(u64, Field<'_>)], number: u64) -> Option<String> {
    let text = std::str::from_utf8(bytes_field(fields, number)?)
        .ok()?
        .trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Parses `product.db`. Installs without a product code are skipped.
pub fn parse_product_db(data: &[u8]) -> Result<Vec<ProductInstall>, ProviderError> {
    let top = fields(data)
        .ok_or_else(|| ProviderError::Parse("product.db is not a valid protobuf file".into()))?;
    Ok(top
        .iter()
        .filter_map(|(number, field)| match field {
            Field::Bytes(install) if *number == 1 => fields(install),
            _ => None,
        })
        .filter_map(|install| {
            let settings = bytes_field(&install, 3).and_then(fields);
            let base_state = bytes_field(&install, 4)
                .and_then(fields)
                .and_then(|state| bytes_field(&state, 1).and_then(fields));
            Some(ProductInstall {
                uid: string_field(&install, 1)?,
                product_code: string_field(&install, 2)?,
                install_path: settings.and_then(|s| string_field(&s, 1)),
                installed: base_state.and_then(|s| varint_field(&s, 1)) == Some(1),
            })
        })
        .collect())
}

pub fn parse_aggregate(text: &str) -> Result<Vec<AggregateGame>, ProviderError> {
    serde_json::from_str::<Aggregate>(text)
        .map(|a| a.installed)
        .map_err(|e| ProviderError::Parse(format!("aggregate.json: {e}")))
}

/// Reads a file the agent may be replacing right now (locked, or briefly missing while it is
/// renamed into place): a few tries, 100 ms apart, as long as the folder exists.
fn read_busy(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut tries = 0;
    loop {
        match std::fs::read(path) {
            Err(_) if tries < 5 && path.parent().is_some_and(Path::is_dir) => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
            other => return other,
        }
    }
}

/// `%ProgramData%\Battle.net\Agent`.
fn default_agent_dir() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join(r"Battle.net\Agent")
}

/// Where uninstall commands come from.
enum Uninstallers {
    /// The Windows "Apps" list of this machine.
    System,
    /// Fixed `(install id, command)` pairs, for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(Vec<(String, String)>),
}

pub struct BattleNetProvider {
    /// The agent's data folder (`product.db`, `aggregate.json`); `None` off Windows.
    agent_dir: Option<PathBuf>,
    uninstallers: Uninstallers,
}

impl BattleNetProvider {
    pub fn detect() -> Self {
        Self {
            agent_dir: cfg!(windows).then(default_agent_dir),
            uninstallers: Uninstallers::System,
        }
    }

    /// Reads `product.db` and `aggregate.json` from `agent_dir` (tests and tools).
    pub fn with_agent_dir(agent_dir: PathBuf, uninstallers: Vec<(String, String)>) -> Self {
        Self {
            agent_dir: Some(agent_dir),
            uninstallers: Uninstallers::Fixed(uninstallers),
        }
    }

    fn installs(&self) -> Result<Vec<ProductInstall>, ProviderError> {
        let Some(dir) = &self.agent_dir else {
            return Err(ProviderError::NotFound(
                "Battle.net is not installed".into(),
            ));
        };
        let path = dir.join("product.db");
        let data = read_busy(&path).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => {
                ProviderError::NotFound("Battle.net is not installed".into())
            }
            _ => ProviderError::Io {
                path: path.display().to_string(),
                source: err,
            },
        })?;
        parse_product_db(&data)
    }

    /// `aggregate.json` by product code. Missing or broken only costs names and artwork.
    fn aggregate(&self) -> HashMap<String, AggregateGame> {
        let Some(dir) = &self.agent_dir else {
            return HashMap::new();
        };
        let parsed = read_busy(&dir.join("aggregate.json"))
            .map_err(|e| ProviderError::NotFound(e.to_string()))
            .and_then(|data| parse_aggregate(&String::from_utf8_lossy(&data)));
        match parsed {
            Ok(games) => games
                .into_iter()
                .map(|g| (g.product_id.to_lowercase(), g))
                .collect(),
            Err(err) => {
                tracing::debug!(%err, "no Battle.net aggregate.json");
                HashMap::new()
            }
        }
    }

    /// The uninstall command for an install id, from its Windows "Apps" entry.
    fn uninstall_command(&self, uid: &str) -> Option<String> {
        let needle = format!("--uid={uid}");
        let matches = |command: &str| {
            command
                .split_whitespace()
                .any(|arg| arg.trim_matches('"') == needle)
        };
        match &self.uninstallers {
            Uninstallers::Fixed(list) => list
                .iter()
                .find(|(id, _)| id == uid)
                .map(|(_, command)| command.clone()),
            Uninstallers::System => crate::platform::uninstall_entries()
                .into_iter()
                .filter_map(|e| e.uninstall_string)
                .find(|command| matches(command)),
        }
    }

    fn game_installs(&self) -> Result<Vec<ProductInstall>, ProviderError> {
        Ok(self
            .installs()?
            .into_iter()
            .filter(|i| i.installed && !NOT_GAMES.contains(&i.product_code.as_str()))
            .collect())
    }

    fn install(&self, product_code: &str) -> Option<ProductInstall> {
        self.game_installs()
            .ok()?
            .into_iter()
            .find(|i| i.product_code.eq_ignore_ascii_case(product_code))
    }
}

impl LibraryProvider for BattleNetProvider {
    fn id(&self) -> &str {
        BATTLENET_SOURCE
    }

    fn name(&self) -> &str {
        "Battle.net"
    }

    fn icon_svg(&self) -> &'static str {
        crate::providers::icons::BATTLENET
    }

    fn can_scan(&self) -> bool {
        self.agent_dir.is_some()
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        let aggregate = self.aggregate();
        let mut games: Vec<ImportedGame> = self
            .game_installs()?
            .into_iter()
            .map(|install| {
                let code = install.product_code.to_lowercase();
                let info = aggregate.get(&code);
                let name = info
                    .and_then(|i| i.name.clone())
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| install.product_code.clone());
                let uri = info
                    .and_then(|i| i.launch_uri.clone())
                    .unwrap_or_else(|| format!("battlenet://game/{code}"));
                ImportedGame {
                    install_dir: install.install_path.map(|p| p.replace('/', "\\")),
                    play_action: Some(PlayAction::uri(uri)),
                    last_played: info
                        .map(|i| i.last_played_timestamp)
                        .filter(|&t| t > 0)
                        .and_then(|t| DateTime::from_timestamp(t, 0)),
                    ..ImportedGame::installed(code, name)
                }
            })
            .collect();
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    fn can_uninstall(&self) -> bool {
        self.agent_dir.is_some()
    }

    /// The Blizzard Uninstaller, which asks for confirmation.
    fn uninstall_action(&self, source_game_id: &str) -> Option<PlayAction> {
        let install = self.install(source_game_id)?;
        let (program, args) =
            crate::platform::split_command_line(&self.uninstall_command(&install.uid)?)?;
        let mut action = PlayAction::file(program);
        action.args = Some(args).filter(|a| !a.is_empty());
        Some(action)
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.aggregate()
            .remove(&source_game_id.to_lowercase())
            .and_then(|g| g.box_art_uri)
            .filter(|uri| uri.starts_with("https://"))
            .map(ImageCandidate::Url)
            .into_iter()
            .collect()
    }

    /// The executable the Battle.net app takes the icon from.
    fn icon_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.aggregate()
            .remove(&source_game_id.to_lowercase())
            .and_then(|g| g.icon_path)
            .map(|p| PathBuf::from(p.replace('/', "\\")))
            .filter(|p| p.is_file())
            .map(ImageCandidate::File)
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn bytes(number: u64, data: &[u8], out: &mut Vec<u8>) {
        varint(number << 3 | 2, out);
        varint(data.len() as u64, out);
        out.extend_from_slice(data);
    }

    fn uint(number: u64, value: u64, out: &mut Vec<u8>) {
        varint(number << 3, out);
        varint(value, out);
    }

    /// One `product.db` install as the agent writes it (with unrelated fields around).
    fn install(uid: &str, code: &str, path: &str, installed: bool) -> Vec<u8> {
        let mut settings = Vec::new();
        bytes(1, path.as_bytes(), &mut settings);
        bytes(2, b"eu", &mut settings);
        uint(3, 0, &mut settings);
        let mut base = Vec::new();
        uint(1, u64::from(installed), &mut base);
        uint(2, u64::from(installed), &mut base);
        bytes(7, b"1.2.3", &mut base);
        let mut state = Vec::new();
        bytes(1, &base, &mut state);
        varint(2 << 3 | 1, &mut state);
        state.extend_from_slice(&[0; 8]);
        let mut out = Vec::new();
        bytes(1, uid.as_bytes(), &mut out);
        bytes(2, code.as_bytes(), &mut out);
        bytes(3, &settings, &mut out);
        bytes(4, &state, &mut out);
        uint(7, 0, &mut out);
        out
    }

    fn product_db() -> Vec<u8> {
        let mut db = Vec::new();
        for entry in [
            install("agent", "agent", "C:/ProgramData/Battle.net/Agent", true),
            install(
                "battle.net",
                "bna",
                "C:/Program Files (x86)/Battle.net",
                true,
            ),
            install("hs_beta", "hsb", "F:/Battle.net/Hearthstone", true),
            install("diablo3", "d3", "F:/Battle.net/Diablo III", false),
            install("wow", "wow", "F:/World of Warcraft", true),
        ] {
            bytes(1, &entry, &mut db);
        }
        bytes(3, b"\x0a\x05other", &mut db);
        db
    }

    const AGGREGATE: &str = r#"{"installed":[{
        "box_art_uri":"https://bnetxboxassets.akamaized.net/hsb/box-deDE.webp",
        "icon_index":0,"icon_path":"F:/Battle.net/Hearthstone/Hearthstone Beta Launcher.exe",
        "last_played_timestamp":1761380000,"launch_uri":"battlenet://game/hsb",
        "logo_art_uri":"https://bnetxboxassets.akamaized.net/hsb/logo-deDE.webp",
        "name":"Hearthstone","product_id":"hsb"}]}"#;

    #[test]
    fn parses_product_db_installs() {
        let installs = parse_product_db(&product_db()).unwrap();
        assert_eq!(installs.len(), 5);
        assert_eq!(
            installs[2],
            ProductInstall {
                uid: "hs_beta".into(),
                product_code: "hsb".into(),
                install_path: Some("F:/Battle.net/Hearthstone".into()),
                installed: true,
            }
        );
        assert!(!installs[3].installed, "still downloading");
        assert!(parse_product_db(b"\x0a\xff").is_err());
        assert!(parse_product_db(b"").unwrap().is_empty());
    }

    fn provider(with_aggregate: bool) -> (tempfile::TempDir, BattleNetProvider) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("product.db"), product_db()).unwrap();
        if with_aggregate {
            std::fs::write(tmp.path().join("aggregate.json"), AGGREGATE).unwrap();
        }
        let provider = BattleNetProvider::with_agent_dir(
            tmp.path().to_path_buf(),
            vec![(
                "hs_beta".into(),
                r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=deDE --uid=hs_beta --displayname="Hearthstone""#.into(),
            )],
        );
        (tmp, provider)
    }

    #[test]
    fn scans_installed_games_with_aggregate_details() {
        let (_tmp, provider) = provider(true);
        let games = provider.scan_installed().unwrap();
        let ids: Vec<_> = games
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str()))
            .collect();
        assert_eq!(ids, [("hsb", "Hearthstone"), ("wow", "wow")]);
        let hs = &games[0];
        assert_eq!(
            hs.install_dir.as_deref(),
            Some(r"F:\Battle.net\Hearthstone")
        );
        assert_eq!(
            hs.play_action.as_ref().unwrap().target,
            "battlenet://game/hsb"
        );
        assert_eq!(hs.last_played.unwrap().timestamp(), 1_761_380_000);
        assert_eq!(
            games[1].play_action.as_ref().unwrap().target,
            "battlenet://game/wow"
        );
        assert_eq!(
            provider.cover_candidates("hsb"),
            [ImageCandidate::Url(
                "https://bnetxboxassets.akamaized.net/hsb/box-deDE.webp".into()
            )]
        );
        assert!(provider.cover_candidates("wow").is_empty());
    }

    #[test]
    fn works_without_aggregate() {
        let (_tmp, provider) = provider(false);
        let games = provider.scan_installed().unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].name, "hsb");
    }

    #[test]
    fn uninstalls_through_the_blizzard_uninstaller() {
        let (_tmp, provider) = provider(true);
        let action = provider.uninstall_action("hsb").unwrap();
        assert_eq!(
            action.target,
            r"C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe"
        );
        assert_eq!(
            action.args.as_deref(),
            Some(r#"--lang=deDE --uid=hs_beta --displayname="Hearthstone""#)
        );
        assert!(provider.uninstall_action("wow").is_none(), "no Apps entry");
        assert!(provider.uninstall_action("d3").is_none(), "not installed");
    }

    #[test]
    fn missing_agent_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = BattleNetProvider::with_agent_dir(tmp.path().join("nope"), Vec::new());
        assert!(matches!(
            provider.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
    }
}
