//! Playtime from GOG Galaxy's local database (Windows).
//!
//! Galaxy 2 keeps `%ProgramData%\GOG.com\Galaxy\storage\galaxy-2.0.db` (SQLite, WAL), with
//! `GameTimes(userId, releaseKey, minutesInGame)` and `LastPlayedDates(userId, gameReleaseKey,
//! lastPlayedDate 'YYYY-MM-DD HH:MM:SS')`. Release keys are `<platform>_<id>`; GOG games are
//! `gog_<product id>` (the others are games Galaxy tracks for other stores and are ignored).
//! Checked on the maintainer's PC on 2026-10-25 (one user, 160 `gog_` rows). This replaces GOG's
//! profile stats endpoint, which refuses the Galaxy sign-in token.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::Connection;

use super::super::ProviderError;
use super::super::sqlite::read_foreign;

/// Playtime of one game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playtime {
    pub seconds: u64,
    pub last_played: Option<DateTime<Utc>>,
}

/// Where Galaxy keeps its database on this machine, when it exists.
#[cfg(windows)]
pub fn default_path() -> Option<PathBuf> {
    let program_data = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let path = program_data.join(r"GOG.com\Galaxy\storage\galaxy-2.0.db");
    path.is_file().then_some(path)
}

#[cfg(not(windows))]
pub fn default_path() -> Option<PathBuf> {
    None
}

/// The product id of a GOG release key (`gog_1423049311`).
fn gog_id(release_key: &str) -> Option<&str> {
    let id = release_key.strip_prefix("gog_")?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

/// Galaxy's dates carry no zone; they are read as UTC (not verified: at worst the last played
/// time is off by the UTC offset).
fn parse_date(text: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|d| d.and_utc())
}

type Rows = (Vec<(String, i64)>, Vec<(String, String)>);

fn read_rows(conn: &Connection) -> rusqlite::Result<Rows> {
    let mut times = conn.prepare(
        "SELECT releaseKey, minutesInGame FROM GameTimes WHERE releaseKey LIKE 'gog\\_%' ESCAPE '\\'",
    )?;
    let times = times
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut dates = conn.prepare(
        "SELECT gameReleaseKey, lastPlayedDate FROM LastPlayedDates \
         WHERE gameReleaseKey LIKE 'gog\\_%' ESCAPE '\\'",
    )?;
    let dates = dates
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok((times, dates))
}

/// Playtime and last played date per GOG product id. Several Galaxy users on one PC: the largest
/// time and the latest date win.
pub fn read_playtime(path: &Path) -> Result<HashMap<String, Playtime>, ProviderError> {
    let (times, dates) = read_foreign(path, "GOG Galaxy database", read_rows)?;
    let mut out: HashMap<String, Playtime> = HashMap::new();
    for (key, minutes) in times {
        let Some(id) = gog_id(&key) else { continue };
        let seconds = u64::try_from(minutes).unwrap_or(0).saturating_mul(60);
        let entry = out.entry(id.to_string()).or_insert(Playtime {
            seconds: 0,
            last_played: None,
        });
        entry.seconds = entry.seconds.max(seconds);
    }
    for (key, date) in dates {
        let (Some(id), Some(date)) = (gog_id(&key), parse_date(&date)) else {
            continue;
        };
        let entry = out.entry(id.to_string()).or_insert(Playtime {
            seconds: 0,
            last_played: None,
        });
        entry.last_played = entry.last_played.max(Some(date));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database with the real table shapes (constraints left out).
    fn galaxy_db(dir: &Path) -> PathBuf {
        let path = dir.join("galaxy-2.0.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE GameTimes(userId INT64, releaseKey TEXT, minutesInGame INTEGER);
             CREATE TABLE LastPlayedDates(userId INT64, gameReleaseKey TEXT, lastPlayedDate TEXT);
             INSERT INTO GameTimes VALUES
               (49605623428909463, 'gog_1423049311', 13036),
               (2, 'gog_1423049311', 10),
               (49605623428909463, 'steam_252950', 81064),
               (49605623428909463, 'gog_1207658924', 0),
               (49605623428909463, 'gog_x', 5);
             INSERT INTO LastPlayedDates VALUES
               (49605623428909463, 'gog_1423049311', '2026-03-06 17:22:46'),
               (49605623428909463, 'gog_1456460669', '2025-12-30 09:51:38'),
               (49605623428909463, 'gog_1207658930', 'garbage'),
               (49605623428909463, 'xboxone_2030093255', '2026-01-03 11:29:23');",
        )
        .unwrap();
        path
    }

    #[test]
    fn reads_gog_rows_only() {
        let tmp = tempfile::tempdir().unwrap();
        let times = read_playtime(&galaxy_db(tmp.path())).unwrap();
        let cyberpunk = times["1423049311"];
        assert_eq!(cyberpunk.seconds, 13036 * 60, "largest user wins");
        assert_eq!(
            cyberpunk.last_played.map(|d| d.to_rfc3339()).as_deref(),
            Some("2026-03-06T17:22:46+00:00")
        );
        assert_eq!(times["1207658924"].seconds, 0);
        assert_eq!(times["1456460669"].seconds, 0, "date without time");
        assert!(times["1456460669"].last_played.is_some());
        assert!(!times.contains_key("252950") && !times.contains_key("x"));
        assert!(!times.contains_key("1207658930"), "unreadable date only");
        assert!(read_playtime(&tmp.path().join("missing.db")).is_err());
    }

    /// Read-only look at this machine: `cargo test -p questkeep-core galaxy_db -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn live_galaxy_db() {
        let Some(path) = default_path() else {
            println!("no Galaxy database");
            return;
        };
        let times = read_playtime(&path).unwrap();
        println!("{} GOG games", times.len());
        let mut top: Vec<_> = times.iter().collect();
        top.sort_by_key(|(_, t)| std::cmp::Reverse(t.seconds));
        for (id, t) in top.into_iter().take(5) {
            println!("{id}: {} min, last {:?}", t.seconds / 60, t.last_played);
        }
    }
}
