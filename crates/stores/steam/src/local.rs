//! Local Steam account data: the logged-in user (`config/loginusers.vdf`) and per-game playtime
//! (`userdata/<accountid>/config/localconfig.vdf`). Everything here is best effort: a missing or
//! broken file means "no data", never an error.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::vdf::{self, Vdf};

/// SteamID64 of account id 0 (individual accounts in the public universe).
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// Playtime and last-played time of one game, from Steam's local config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LocalStats {
    pub playtime_seconds: u64,
    pub last_played: Option<DateTime<Utc>>,
}

/// SteamID64 of the most recent login: `MostRecent` = 1, else the highest `Timestamp`.
pub fn most_recent_user(users: &Vdf) -> Option<String> {
    let mut best: Option<(bool, u64, &str)> = None;
    for (id, user) in users.entries() {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || user.as_str().is_some() {
            continue;
        }
        let recent = user.get_str("MostRecent").map(str::trim) == Some("1");
        let timestamp = user
            .get_str("Timestamp")
            .and_then(|t| t.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let rank = (recent, timestamp);
        if best.is_none_or(|(r, t, _)| rank > (r, t)) {
            best = Some((recent, timestamp, id));
        }
    }
    best.map(|(_, _, id)| id.to_string())
}

/// SteamID64 of the most recent login of this Steam installation.
pub fn most_recent_steam_id(root: &Path) -> Option<String> {
    let doc = read_vdf(&root.join("config").join("loginusers.vdf"))?;
    most_recent_user(&doc)
}

/// Account id (the folder name below `userdata`) of a SteamID64.
pub fn account_id(steam_id64: &str) -> Option<u64> {
    steam_id64
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_sub(STEAM_ID64_BASE)
}

/// Per-app stats from a parsed `localconfig.vdf` document, keyed by app id. Apps without any
/// playtime or last-played value are left out.
pub fn parse_local_config(config: &Vdf) -> HashMap<String, LocalStats> {
    let mut stats = HashMap::new();
    let apps = ["Software", "Valve", "Steam", "apps"]
        .iter()
        .try_fold(config, |node, key| node.get(key));
    let Some(apps) = apps else {
        return stats;
    };
    for (app_id, app) in apps.entries() {
        if app.as_str().is_some() {
            continue;
        }
        let number = |key: &str| app.get_str(key).and_then(|v| v.trim().parse::<u64>().ok());
        let playtime_seconds = number("Playtime").unwrap_or(0).saturating_mul(60);
        let last_played = number("LastPlayed")
            .filter(|&t| t > 0)
            .and_then(|t| i64::try_from(t).ok())
            .and_then(|t| DateTime::from_timestamp(t, 0));
        if playtime_seconds > 0 || last_played.is_some() {
            stats.insert(
                app_id.clone(),
                LocalStats {
                    playtime_seconds,
                    last_played,
                },
            );
        }
    }
    stats
}

/// Stats of the most recently logged-in user of one Steam root; empty when anything is missing.
pub fn local_stats(root: &Path) -> HashMap<String, LocalStats> {
    let Some(steam_id) = most_recent_steam_id(root) else {
        return HashMap::new();
    };
    let Some(account) = account_id(&steam_id) else {
        tracing::warn!("Steam login user id is not a valid SteamID64");
        return HashMap::new();
    };
    let path = root
        .join("userdata")
        .join(account.to_string())
        .join("config")
        .join("localconfig.vdf");
    match read_vdf(&path) {
        Some(doc) => parse_local_config(&doc),
        None => HashMap::new(),
    }
}

fn read_vdf(path: &Path) -> Option<Vdf> {
    if !path.is_file() {
        return None;
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("could not read {}: {e}", path.display());
            return None;
        }
    };
    match vdf::parse(&String::from_utf8_lossy(&bytes)) {
        Ok((_, doc)) => Some(doc),
        Err(e) => {
            tracing::warn!("could not parse {}: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Vdf {
        vdf::parse(text).unwrap().1
    }

    #[test]
    fn most_recent_flag_wins_over_timestamp() {
        let users = parse(
            r#""users" {
                "76561198000000001" { "AccountName" "a" "MostRecent" "0" "Timestamp" "900" }
                "76561198000000002" { "AccountName" "b" "MostRecent" "1" "Timestamp" "100" }
            }"#,
        );
        assert_eq!(
            most_recent_user(&users).as_deref(),
            Some("76561198000000002")
        );
    }

    #[test]
    fn highest_timestamp_without_flag() {
        let users = parse(
            r#""users" {
                "76561198000000001" { "Timestamp" "100" }
                "76561198000000002" { "Timestamp" "300" }
                "76561198000000003" { "Timestamp" "200" }
                "junk" { "MostRecent" "1" }
            }"#,
        );
        assert_eq!(
            most_recent_user(&users).as_deref(),
            Some("76561198000000002")
        );
        assert_eq!(most_recent_user(&parse(r#""users" { }"#)), None);
    }

    #[test]
    fn converts_steam_id_to_account_id() {
        assert_eq!(account_id("76561197960265729"), Some(1));
        assert_eq!(account_id("76561198000000000"), Some(39_734_272));
        assert_eq!(account_id("12"), None);
        assert_eq!(account_id("abc"), None);
    }

    #[test]
    fn parses_local_config_case_insensitively() {
        let doc = parse(
            r#""UserLocalConfigStore" {
                "software" { "VALVE" { "steam" { "Apps" {
                    "620" { "playtime" "90" "LASTPLAYED" "1700000000" "Other" "x" }
                    "440" { "Playtime" "0" "LastPlayed" "0" }
                    "730" { "LastPlayed" "1600000000" }
                    "1" "not an object"
                } } } }
            }"#,
        );
        let stats = parse_local_config(&doc);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats["620"].playtime_seconds, 5400);
        assert_eq!(stats["620"].last_played.unwrap().timestamp(), 1_700_000_000);
        assert_eq!(stats["730"].playtime_seconds, 0);
        assert!(stats["730"].last_played.is_some());
        assert!(!stats.contains_key("440"));
    }

    #[test]
    fn local_config_without_apps_is_empty() {
        assert!(
            parse_local_config(&parse(r#""UserLocalConfigStore" { "Software" { } }"#)).is_empty()
        );
    }

    #[test]
    fn reads_stats_from_disk_and_survives_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert!(local_stats(root).is_empty());

        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/loginusers.vdf"),
            r#""users" { "76561197960265729" { "MostRecent" "1" } }"#,
        )
        .unwrap();
        assert!(local_stats(root).is_empty());

        let dir = root.join("userdata/1/config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("localconfig.vdf"), "\"broken\" {").unwrap();
        assert!(local_stats(root).is_empty());

        std::fs::write(
            dir.join("localconfig.vdf"),
            r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" { "apps" {
                "620" { "Playtime" "2" }
            } } } } }"#,
        )
        .unwrap();
        assert_eq!(local_stats(root)["620"].playtime_seconds, 120);
        assert_eq!(
            most_recent_steam_id(root).as_deref(),
            Some("76561197960265729")
        );
    }
}
