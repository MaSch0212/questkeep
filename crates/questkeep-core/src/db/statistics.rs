//! Raw rows for the statistics page; `crate::statistics` turns them into figures.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::Database;
use crate::error::Result;

/// A library entry (games only, no apps and no fake games), with playtime over all its copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatEntry {
    pub id: Uuid,
    pub name: String,
    pub source: String,
    pub playtime_seconds: u64,
    pub play_count: u64,
    pub installed: bool,
    pub last_played: Option<DateTime<Utc>>,
    pub genres: Vec<String>,
    pub release_year: Option<i32>,
    /// Main story time to beat in seconds.
    pub ttb_main: Option<u64>,
    pub completion_status: Option<String>,
}

/// One store's copy row: its own playtime (per-store totals) and, while installed, its folder
/// and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatCopy {
    pub entry: Uuid,
    pub source: String,
    pub playtime_seconds: u64,
    pub install_dir: Option<String>,
    pub install_size: Option<u64>,
}

fn as_u64(value: Option<i64>) -> u64 {
    value
        .and_then(|v| u64::try_from(v).ok())
        .unwrap_or_default()
}

impl Database {
    pub fn stat_entries(&self, fake_source: &str) -> Result<Vec<StatEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, source, playtime_seconds, play_count, installed, last_played, \
             genres, release_year, ttb_main, completion_status FROM library_entries \
             WHERE COALESCE(kind_user, kind) <> 'app' AND source <> ?1",
        )?;
        let rows = stmt.query_map([fake_source], |r| {
            Ok((
                r.get::<_, String>(0)?,
                StatEntry {
                    id: Uuid::nil(),
                    name: r.get(1)?,
                    source: r.get(2)?,
                    playtime_seconds: as_u64(r.get(3)?),
                    play_count: as_u64(r.get(4)?),
                    installed: r.get(5)?,
                    last_played: r.get(6)?,
                    genres: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default(),
                    release_year: r.get(8)?,
                    ttb_main: r
                        .get::<_, Option<i64>>(9)?
                        .and_then(|v| u64::try_from(v).ok()),
                    completion_status: r.get(10)?,
                },
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, entry) = row?;
            if let Ok(id) = Uuid::parse_str(&id) {
                out.push(StatEntry { id, ..entry });
            }
        }
        Ok(out)
    }

    pub fn stat_copies(&self, fake_source: &str) -> Result<Vec<StatCopy>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT COALESCE(merged_into, id), source, playtime_seconds, \
             CASE WHEN installed THEN install_dir END, CASE WHEN installed THEN install_size END \
             FROM games WHERE COALESCE(kind_user, kind) <> 'app' AND source <> ?1",
        )?;
        let rows = stmt.query_map([fake_source], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (entry, source, playtime, install_dir, size) = row?;
            if let Ok(entry) = Uuid::parse_str(&entry) {
                out.push(StatCopy {
                    entry,
                    source,
                    playtime_seconds: as_u64(playtime),
                    install_dir: install_dir.filter(|d| !d.trim().is_empty()),
                    install_size: size.and_then(|v| u64::try_from(v).ok()),
                });
            }
        }
        Ok(out)
    }

    /// The longest finished session: its entry and length.
    pub fn longest_session(&self) -> Result<Option<(Uuid, u64)>> {
        let conn = self.conn();
        let row: Option<(String, i64)> = rusqlite::OptionalExtension::optional(conn.query_row(
            "SELECT COALESCE(g.merged_into, g.id), s.duration_seconds FROM play_sessions s \
             JOIN games g ON g.id = s.game_id WHERE s.duration_seconds IS NOT NULL \
             ORDER BY s.duration_seconds DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ))?;
        Ok(row.and_then(|(id, s)| Some((Uuid::parse_str(&id).ok()?, u64::try_from(s).ok()?))))
    }

    /// Number and total length of finished sessions.
    pub fn session_totals(&self) -> Result<(u64, u64)> {
        let (count, total): (i64, i64) = self.conn().query_row(
            "SELECT COUNT(*), COALESCE(SUM(duration_seconds), 0) FROM play_sessions \
             WHERE duration_seconds > 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((as_u64(Some(count)), as_u64(Some(total))))
    }

    /// The first day with playtime history (a recorded session or store growth), as
    /// `YYYY-MM-DD` (sessions in UTC).
    pub fn history_start(&self) -> Result<Option<String>> {
        Ok(self.conn().query_row(
            "SELECT MIN(d) FROM (SELECT MIN(day) AS d FROM playtime_days \
             UNION ALL SELECT MIN(substr(started_at, 1, 10)) FROM play_sessions \
             WHERE duration_seconds > 0)",
            [],
            |r| r.get(0),
        )?)
    }
}
