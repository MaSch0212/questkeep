//! Playtime history for the statistics page: the stores' playtime growth between syncs (per local
//! day) next to the play sessions Questkeep records itself.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::Database;
use crate::error::Result;

/// Recorded seconds the store hasn't reported yet are forgotten after this long (the store will
/// never count them, e.g. a game started outside its client).
const UNMATCHED_FOR: chrono::Duration = chrono::Duration::days(2);

/// A game's last store report: seconds, unmatched recorded seconds, since when, checked at.
type Snapshot = (i64, i64, Option<DateTime<Utc>>, DateTime<Utc>);

/// A play session with the entry and store it belongs to, for the statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatSession {
    pub entry: Uuid,
    pub source: String,
    pub span: super::start::SessionSpan,
}

/// Store playtime growth on one local day, for the statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDay {
    pub entry: Uuid,
    pub source: String,
    pub day: NaiveDate,
    pub seconds: u64,
}

impl Database {
    /// Remembers the playtime a store reported for its games (`source_game_id`, seconds). The
    /// growth since the last report that Questkeep did not record itself counts for `day`; the
    /// first report of a game is only remembered (its history before is unknown).
    pub fn record_store_playtime(
        &self,
        source: &str,
        reported: &[(String, u64)],
        now: DateTime<Utc>,
        day: NaiveDate,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (source_game_id, seconds) in reported {
            let Some(id): Option<String> = tx
                .query_row(
                    "SELECT id FROM games WHERE source = ?1 AND source_game_id = ?2",
                    params![source, source_game_id],
                    |r| r.get(0),
                )
                .optional()?
            else {
                continue;
            };
            let reported = i64::try_from(*seconds).unwrap_or(i64::MAX);
            let last: Option<Snapshot> = tx
                .query_row(
                    "SELECT seconds, unmatched, unmatched_at, checked_at FROM store_playtime \
                     WHERE game_id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            let Some((before, unmatched, unmatched_at, checked_at)) = last else {
                tx.execute(
                    "INSERT INTO store_playtime (game_id, seconds, checked_at) VALUES (?1, ?2, ?3)",
                    params![id, reported, now],
                )?;
                continue;
            };
            let recorded: i64 = tx.query_row(
                "SELECT COALESCE(SUM(duration_seconds), 0) FROM play_sessions \
                 WHERE game_id = ?1 AND ended_at > ?2 AND ended_at <= ?3",
                params![id, checked_at, now],
                |r| r.get(0),
            )?;
            let fresh = unmatched_at.is_some_and(|at| now - at < UNMATCHED_FOR);
            let mut unmatched = if fresh { unmatched } else { 0 } + recorded;
            // Less than before (a stale local file read between two online reports): ignored, so
            // the next report doesn't count the same time again.
            let grown = (reported - before).max(0);
            let reported = reported.max(before);
            let matched = grown.min(unmatched);
            unmatched -= matched;
            let played = grown - matched;
            if played > 0 {
                tx.execute(
                    "INSERT INTO playtime_days (game_id, day, seconds) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (game_id, day) DO UPDATE SET seconds = seconds + excluded.seconds",
                    params![id, day.to_string(), played],
                )?;
            }
            let unmatched_at = if recorded > 0 {
                Some(now)
            } else if unmatched > 0 {
                unmatched_at
            } else {
                None
            };
            tx.execute(
                "UPDATE store_playtime SET seconds = ?2, unmatched = ?3, unmatched_at = ?4, \
                 checked_at = ?5 WHERE game_id = ?1",
                params![id, reported, unmatched, unmatched_at, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Play sessions of games (not apps) still running at or after `since` (unfinished ones up
    /// to their last heartbeat), oldest first.
    pub fn stat_sessions_since(&self, since: DateTime<Utc>) -> Result<Vec<StatSession>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT COALESCE(g.merged_into, g.id), g.source, s.started_at,              COALESCE(s.ended_at, s.last_seen_at), s.duration_seconds              FROM play_sessions s JOIN games g ON g.id = s.game_id              JOIN games e ON e.id = COALESCE(g.merged_into, g.id)              WHERE COALESCE(s.ended_at, s.last_seen_at) >= ?1              AND COALESCE(e.kind_user, e.kind) <> 'app' ORDER BY s.started_at",
        )?;
        let rows = stmt.query_map([since], |r| {
            let start: DateTime<Utc> = r.get(2)?;
            let end: DateTime<Utc> = r.get(3)?;
            let credited = r
                .get::<_, Option<i64>>(4)?
                .unwrap_or_else(|| (end - start).num_seconds());
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                super::start::SessionSpan {
                    start,
                    end,
                    credited: u64::try_from(credited).unwrap_or_default(),
                },
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (entry, source, span) = row?;
            if let Ok(entry) = Uuid::parse_str(&entry) {
                out.push(StatSession {
                    entry,
                    source,
                    span,
                });
            }
        }
        Ok(out)
    }

    /// Store playtime growth on `from` and later days.
    pub fn store_days_since(&self, from: NaiveDate) -> Result<Vec<StoreDay>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT COALESCE(g.merged_into, g.id), g.source, d.day, d.seconds \
             FROM playtime_days d JOIN games g ON g.id = d.game_id \
             WHERE d.day >= ?1 ORDER BY d.day",
        )?;
        let rows = stmt.query_map([from.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (entry, source, day, seconds) = row?;
            if let (Ok(entry), Ok(day)) = (Uuid::parse_str(&entry), day.parse::<NaiveDate>()) {
                out.push(StoreDay {
                    entry,
                    source,
                    day,
                    seconds: u64::try_from(seconds).unwrap_or_default(),
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImportedGame;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn at(d: u32, h: u32) -> DateTime<Utc> {
        day(d).and_hms_opt(h, 0, 0).unwrap().and_utc()
    }

    fn store_days(db: &Database) -> Vec<(NaiveDate, u64)> {
        db.store_days_since(day(1))
            .unwrap()
            .into_iter()
            .map(|d| (d.day, d.seconds))
            .collect()
    }

    fn setup() -> (Database, Uuid) {
        let db = Database::open_in_memory().unwrap();
        db.upsert_imported("steam", &[ImportedGame::installed("1", "Hades")], at(1, 0))
            .unwrap();
        let id = db.find_store_game_row("steam", "1").unwrap();
        (db, id)
    }

    impl Database {
        fn find_store_game_row(&self, source: &str, source_game_id: &str) -> Result<Uuid> {
            let id: String = self.conn().query_row(
                "SELECT id FROM games WHERE source = ?1 AND source_game_id = ?2",
                params![source, source_game_id],
                |r| r.get(0),
            )?;
            Ok(Uuid::parse_str(&id).unwrap())
        }
    }

    #[test]
    fn store_growth_between_syncs_counts_for_the_sync_day() {
        let (db, _) = setup();
        let report = |s: u64| vec![("1".to_string(), s)];
        db.record_store_playtime("steam", &report(360_000), at(2, 10), day(2))
            .unwrap();
        assert!(
            store_days(&db).is_empty(),
            "the first report is the baseline"
        );
        db.record_store_playtime("steam", &report(363_600), at(3, 10), day(3))
            .unwrap();
        db.record_store_playtime("steam", &report(363_600), at(3, 11), day(3))
            .unwrap();
        db.record_store_playtime("steam", &report(365_400), at(3, 20), day(3))
            .unwrap();
        assert_eq!(store_days(&db), vec![(day(3), 5_400)]);
        // Less than before (a stale local file), then the online value again: nothing counted.
        db.record_store_playtime("steam", &report(100), at(4, 10), day(4))
            .unwrap();
        db.record_store_playtime("steam", &report(365_400), at(4, 11), day(4))
            .unwrap();
        assert_eq!(store_days(&db), vec![(day(3), 5_400)]);
    }

    #[test]
    fn time_questkeep_recorded_is_not_counted_twice() {
        let (db, id) = setup();
        let report = |s: u64| vec![("1".to_string(), s)];
        db.record_store_playtime("steam", &report(1_000), at(2, 10), day(2))
            .unwrap();
        let session = db.begin_session(id, at(2, 12)).unwrap();
        db.finish_session(session, at(2, 13), 3_600).unwrap();
        // The store hasn't counted it yet ...
        db.record_store_playtime("steam", &report(1_000), at(2, 14), day(2))
            .unwrap();
        // ... and does now, plus 600 s played elsewhere.
        db.record_store_playtime("steam", &report(5_200), at(2, 18), day(2))
            .unwrap();
        assert_eq!(store_days(&db), vec![(day(2), 600)]);
        assert_eq!(db.sessions_since(at(1, 0)).unwrap().len(), 1);
    }

    #[test]
    fn unmatched_time_is_forgotten_after_two_days() {
        let (db, id) = setup();
        let report = |s: u64| vec![("1".to_string(), s)];
        db.record_store_playtime("steam", &report(0), at(2, 10), day(2))
            .unwrap();
        // Started outside the store: the store never counts it.
        let session = db.begin_session(id, at(2, 11)).unwrap();
        db.finish_session(session, at(2, 12), 3_600).unwrap();
        db.record_store_playtime("steam", &report(0), at(2, 13), day(2))
            .unwrap();
        db.record_store_playtime("steam", &report(1_800), at(5, 13), day(5))
            .unwrap();
        assert_eq!(store_days(&db), vec![(day(5), 1_800)]);
    }
}
