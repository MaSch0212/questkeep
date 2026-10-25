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
//! Owned games come from the Battle.net account website, the way Playnite's Battle.net library
//! reads them: the login window signs in on account.battle.net in a browser profile of its own
//! ([`WebAccount`]), and before a scan a hidden window opens the account page, whose script reads
//! `/api/games-and-subs` (and the BattleTag from `/api/details`) with that session and reports
//! them through [`CAPTURE_PREFIX`]. Each game account's `titleId` is the product's four-letter
//! code as a big-endian number (`WTCG` = 1465140039), which maps to the install's product code.
//! Classic games bought as CD keys (`/api/classic-games`) are not installed through Battle.net and
//! are left out.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::DateTime;
use serde::Deserialize;

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::auth::{AccountAuth, WebAccount, WebAccountSpec};
use crate::model::{ImportedGame, PlayAction};

pub const BATTLENET_SOURCE: &str = "battlenet";
/// Product codes of the agent and the Battle.net app itself.
const NOT_GAMES: [&str; 2] = ["agent", "bna"];
/// The page the login window opens; it signs in through Battle.net's own login and ends on the
/// account overview. The hidden window opens it too: it renews the session's cookies.
pub const LOGIN_URL: &str = "https://account.battle.net/oauth2/authorization/account-settings";
pub const SESSION_URL: &str = LOGIN_URL;
/// Navigations to this address carry what the account page reported; the window never loads it.
pub const CAPTURE_PREFIX: &str = "https://questkeep.invalid/battlenet-session";
/// A capture this recent is reused (the scan right after signing in).
const CAPTURE_REUSE_SECS: i64 = 120;
/// Product codes that differ from the lower-case title code.
const CODE_OVERRIDES: [(&str, &str); 3] = [("wtcg", "hsb"), ("fen", "fenris"), ("vipr", "viper")];

/// What the account sign-in needs to know about Battle.net.
pub const ACCOUNT_SPEC: WebAccountSpec = WebAccountSpec {
    provider: BATTLENET_SOURCE,
    name: "Battle.net",
    login_url: LOGIN_URL,
    capture_prefix: CAPTURE_PREFIX,
};

/// The four-letter code packed into an account `titleId` (`1465140039` → `WTCG`).
pub fn title_code(title_id: i64) -> Option<String> {
    let bytes = u32::try_from(title_id).ok()?.to_be_bytes();
    let code: String = bytes
        .iter()
        .skip_while(|&&b| b == 0)
        .map(|&b| b as char)
        .collect();
    (!code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric())).then_some(code)
}

/// The install product code (the game's id here) for a title code.
pub fn product_code(title_code: &str) -> String {
    let lower = title_code.to_ascii_lowercase();
    CODE_OVERRIDES
        .iter()
        .find(|(title, _)| *title == lower)
        .map_or(lower, |(_, code)| (*code).to_string())
}

/// One owned title as the account page reports it (`games` capture parameter).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OwnedTitle {
    title_id: i64,
    #[serde(default)]
    localized_game_name: Option<String>,
}

/// Owned games from the `games` capture parameter (the `gameAccounts` list of
/// `/api/games-and-subs`): one per product, ids as install product codes.
pub fn parse_owned(json: &str) -> Result<Vec<ImportedGame>, ProviderError> {
    let titles: Vec<OwnedTitle> = serde_json::from_str(json)
        .map_err(|e| ProviderError::Parse(format!("Battle.net games list: {e}")))?;
    let mut games: Vec<ImportedGame> = Vec::new();
    for title in titles {
        let Some(code) = title_code(title.title_id).map(|c| product_code(&c)) else {
            tracing::debug!(title = title.title_id, "unknown Battle.net title id");
            continue;
        };
        // Several game accounts (e.g. WoW regions) share one product.
        if games.iter().any(|g| g.source_game_id == code) {
            continue;
        }
        let name = title
            .localized_game_name
            .as_deref()
            .map(clean_name)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| code.clone());
        let mut game = ImportedGame::installed(code, name);
        game.installed = false;
        games.push(game);
    }
    Ok(games)
}

/// Strips trademark signs and surplus whitespace from a title.
fn clean_name(name: &str) -> String {
    name.replace(['\u{2122}', '\u{00ae}', '\u{00a9}'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

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
    account: Option<Arc<WebAccount>>,
}

impl BattleNetProvider {
    pub fn detect() -> Self {
        Self {
            agent_dir: cfg!(windows).then(default_agent_dir),
            uninstallers: Uninstallers::System,
            account: None,
        }
    }

    /// Adds the account sign-in, which lists owned games that are not installed.
    pub fn with_account(mut self, account: Arc<WebAccount>) -> Self {
        self.account = Some(account);
        self
    }

    /// Reads `product.db` and `aggregate.json` from `agent_dir` (tests and tools).
    pub fn with_agent_dir(agent_dir: PathBuf, uninstallers: Vec<(String, String)>) -> Self {
        Self {
            agent_dir: Some(agent_dir),
            uninstallers: Uninstallers::Fixed(uninstallers),
            account: None,
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

    fn can_scan_owned(&self) -> bool {
        self.account.is_some()
    }

    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        let Some(account) = &self.account else {
            return Err(ProviderError::Unsupported);
        };
        let capture = account.capture(CAPTURE_REUSE_SECS)?;
        let games = capture.get("games").ok_or_else(|| {
            ProviderError::Parse("The Battle.net account page did not list any games.".into())
        })?;
        parse_owned(games)
    }

    fn account(&self) -> Option<&dyn AccountAuth> {
        self.account.as_deref().map(|a| a as &dyn AccountAuth)
    }

    /// Opens the game's page in Battle.net, which offers the install.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        (self.agent_dir.is_some() && source_game_id.chars().all(|c| c.is_ascii_alphanumeric()))
            .then(|| PlayAction::uri(format!("battlenet://game/{source_game_id}")))
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
    fn maps_account_titles_to_product_codes() {
        assert_eq!(title_code(1_465_140_039).as_deref(), Some("WTCG"));
        assert_eq!(title_code(5_730_135).as_deref(), Some("WoW"));
        assert_eq!(title_code(17_459).as_deref(), Some("D3"));
        assert_eq!(title_code(0), None);
        assert_eq!(title_code(-1), None);
        assert_eq!(title_code(0x0102_0304), None);
        assert_eq!(product_code("WTCG"), "hsb");
        assert_eq!(product_code("Fen"), "fenris");
        assert_eq!(product_code("Pro"), "pro");

        let games = parse_owned(
            r#"[{"titleId":1465140039,"localizedGameName":"Hearthstone®"},
                {"titleId":5730135,"localizedGameName":"World of Warcraft"},
                {"titleId":5730135,"localizedGameName":"World of Warcraft"},
                {"titleId":4613486},{"titleId":1}]"#,
        )
        .unwrap();
        let ids: Vec<_> = games
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str(), g.installed))
            .collect();
        assert_eq!(
            ids,
            [
                ("hsb", "Hearthstone", false),
                ("wow", "World of Warcraft", false),
                ("fenris", "fenris", false),
            ]
        );
        assert!(parse_owned("{}").is_err());
    }

    #[test]
    fn owned_scan_needs_an_account_and_installs_through_battlenet() {
        let (_tmp, provider) = provider(false);
        assert!(!provider.can_scan_owned());
        assert!(matches!(
            provider.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::Unsupported)
        ));
        let provider = provider.with_account(Arc::new(WebAccount::new(
            ACCOUNT_SPEC,
            Arc::new(crate::auth::MemoryTokenStore::new()),
            None,
        )));
        assert!(provider.can_scan_owned());
        assert!(matches!(
            provider.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::NotConfigured(_))
        ));
        provider
            .account()
            .unwrap()
            .sign_in_with_code(&format!(
                "{CAPTURE_PREFIX}?name=Ann%231234&games={}",
                crate::auth::url_encode(
                    r#"[{"titleId":1465140039,"localizedGameName":"Hearthstone"}]"#
                )
            ))
            .unwrap();
        assert_eq!(
            provider.account().unwrap().status().display_name.as_deref(),
            Some("Ann#1234")
        );
        let owned = provider.scan_owned(&serde_json::Value::Null).unwrap();
        assert_eq!(owned[0].source_game_id, "hsb");
        assert_eq!(
            provider.install_action("d3").unwrap().target,
            "battlenet://game/d3"
        );
        assert!(provider.install_action("d3/../x").is_none());
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
