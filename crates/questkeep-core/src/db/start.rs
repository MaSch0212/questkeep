//! Start page data: the shelves, playtime per day and installed games per store. Also the small
//! app state the start page needs (the page open last).

use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use rusqlite::{OptionalExtension, params};

use super::Database;
use super::games::{COLUMNS, attach_copies, row_to_game};
use crate::error::Result;
use crate::model::{DayPlaytime, MANUAL_SOURCE, ShelfGame, StoreInstalls};

/// `app_state` key of the start page or view that was open last.
const LAST_PAGE: &str = "lastPage";

/// A play session's wall-clock span and the time credited for it (which skips system sleep).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSpan {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub credited: u64,
}

impl Database {
    /// The most recently played library entries (hidden ones left out), newest first.
    pub fn recently_played(&self, limit: u32) -> Result<Vec<ShelfGame>> {
        self.shelf(
            "last_played",
            "hidden = 0 AND last_played IS NOT NULL",
            limit,
        )
    }

    /// The most recently added library entries (hidden ones left out), newest first. An entry
    /// combined from several stores counts as added when its newest copy was.
    pub fn recently_added(&self, limit: u32) -> Result<Vec<ShelfGame>> {
        self.shelf(
            "MAX(date_added, COALESCE((SELECT MAX(m.date_added) FROM games m \
             WHERE m.merged_into = games.id), date_added))",
            "hidden = 0",
            limit,
        )
    }

    fn shelf(&self, date_sql: &str, where_sql: &str, limit: u32) -> Result<Vec<ShelfGame>> {
        let sql = format!(
            "SELECT {COLUMNS}, {date_sql} AS shelf_date FROM library_entries AS games \
             WHERE {where_sql} \
             ORDER BY shelf_date DESC, sorting_name COLLATE NOCASE ASC, id ASC LIMIT ?1"
        );
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt
            .query_map([limit], |r| {
                Ok((row_to_game(r)?, r.get::<_, DateTime<Utc>>("shelf_date")?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let (mut games, dates): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
        attach_copies(&conn, &mut games)?;
        Ok(games
            .into_iter()
            .zip(dates)
            .map(|(game, date)| ShelfGame { game, date })
            .collect())
    }

    /// Play sessions that were still running at or after `since`. Unfinished sessions count up
    /// to their last heartbeat.
    pub fn sessions_since(&self, since: DateTime<Utc>) -> Result<Vec<SessionSpan>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT started_at, COALESCE(ended_at, last_seen_at), duration_seconds \
             FROM play_sessions WHERE COALESCE(ended_at, last_seen_at) >= ?1",
        )?;
        let spans = stmt
            .query_map([since], |r| {
                let start: DateTime<Utc> = r.get(0)?;
                let end: DateTime<Utc> = r.get(1)?;
                let credited = match r.get::<_, Option<i64>>(2)? {
                    Some(seconds) => seconds,
                    None => (end - start).num_seconds(),
                };
                Ok(SessionSpan {
                    start,
                    end,
                    credited: u64::try_from(credited).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(spans)
    }

    /// Installed and owned games per store, most owned first. Hidden entries and manually added
    /// games (no store) are left out; an entry in several stores counts once in each.
    pub fn installed_by_store(&self) -> Result<Vec<StoreInstalls>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT g.source, \
                 COUNT(DISTINCT CASE WHEN g.installed THEN e.id END), COUNT(DISTINCT e.id) \
             FROM games g JOIN games e ON e.id = COALESCE(g.merged_into, g.id) \
             WHERE e.hidden = 0 AND g.source <> ?1 \
             GROUP BY g.source ORDER BY COUNT(DISTINCT e.id) DESC, g.source",
        )?;
        let stores = stmt
            .query_map([MANUAL_SOURCE], |r| {
                Ok(StoreInstalls {
                    source: r.get(0)?,
                    installed: r.get(1)?,
                    owned: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(stores)
    }

    /// The start page (`"home"`) or view id that was open last, if any was recorded.
    pub fn last_page(&self) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT value FROM app_state WHERE key = ?1",
                [LAST_PAGE],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_last_page(&self, page: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO app_state (key, value) VALUES (?1, ?2) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![LAST_PAGE, page],
        )?;
        Ok(())
    }
}

/// The start of local calendar day `date` in `tz`. A midnight skipped by a DST change falls back
/// to the first hour that exists.
fn day_start<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> DateTime<Utc> {
    let midnight = date.and_time(chrono::NaiveTime::MIN);
    (0..3)
        .find_map(|hour| {
            tz.from_local_datetime(&(midnight + chrono::Duration::hours(hour)))
                .earliest()
        })
        .map_or_else(|| midnight.and_utc(), |t| t.with_timezone(&Utc))
}

/// Credited playtime for each of the `days` local days ending with `today` (oldest first). A
/// session spanning midnight spreads its credited time over the days in proportion to its
/// wall-clock span.
pub fn playtime_by_day<Tz: TimeZone>(
    sessions: &[SessionSpan],
    today: NaiveDate,
    tz: &Tz,
    days: u32,
) -> Vec<DayPlaytime> {
    let Some(first) = today.checked_sub_days(Days::new(u64::from(days.saturating_sub(1)))) else {
        return Vec::new();
    };
    let dates: Vec<NaiveDate> = first.iter_days().take(days as usize).collect();
    let bounds: Vec<DateTime<Utc>> = dates
        .iter()
        .map(|&d| day_start(tz, d))
        .chain(today.succ_opt().map(|d| day_start(tz, d)))
        .collect();
    let mut seconds = vec![0f64; dates.len()];
    for session in sessions {
        let wall = (session.end - session.start).num_milliseconds();
        for (i, slot) in seconds.iter_mut().enumerate() {
            let (Some(&from), Some(&to)) = (bounds.get(i), bounds.get(i + 1)) else {
                continue;
            };
            if wall <= 0 {
                if session.start >= from && session.start < to {
                    *slot += session.credited as f64;
                }
                continue;
            }
            let overlap = (session.end.min(to) - session.start.max(from)).num_milliseconds();
            if overlap > 0 {
                *slot += session.credited as f64 * overlap as f64 / wall as f64;
            }
        }
    }
    dates
        .into_iter()
        .zip(seconds)
        .map(|(date, s)| DayPlaytime {
            date,
            seconds: s.round() as u64,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, FixedOffset};
    use uuid::Uuid;

    use super::*;
    use crate::db::NewGameRecord;

    fn add(db: &Database, name: &str, source: &str, at: DateTime<Utc>) -> Uuid {
        db.insert_game(
            &NewGameRecord {
                name: name.into(),
                source: source.into(),
                source_game_id: Some(name.into()),
                ..Default::default()
            },
            at,
        )
        .unwrap()
        .id
    }

    fn set(db: &Database, id: Uuid, sql: &str) {
        db.conn()
            .execute(
                &format!("UPDATE games SET {sql} WHERE id = ?1"),
                [id.to_string()],
            )
            .unwrap();
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn names(shelf: &[ShelfGame]) -> Vec<&str> {
        shelf.iter().map(|s| s.game.name.as_str()).collect()
    }

    #[test]
    fn recently_played_is_newest_first_without_hidden_or_unplayed() {
        let db = Database::open_in_memory().unwrap();
        let t = at("2026-10-01T10:00:00Z");
        let a = add(&db, "A", "steam", t);
        let b = add(&db, "B", "steam", t);
        let c = add(&db, "C", "steam", t);
        add(&db, "Never", "steam", t);
        set(&db, a, "last_played = '2026-10-02T10:00:00Z'");
        set(&db, b, "last_played = '2026-10-05T10:00:00Z'");
        set(&db, c, "last_played = '2026-10-06T10:00:00Z', hidden = 1");

        let shelf = db.recently_played(15).unwrap();
        assert_eq!(names(&shelf), ["B", "A"]);
        assert_eq!(shelf[0].date, at("2026-10-05T10:00:00Z"));
        assert_eq!(db.recently_played(1).unwrap().len(), 1);
    }

    #[test]
    fn recently_added_uses_the_newest_copy_and_breaks_ties_by_name() {
        let db = Database::open_in_memory().unwrap();
        let old = at("2026-01-01T00:00:00Z");
        let new = at("2026-10-01T00:00:00Z");
        let entry = add(&db, "Old entry", "steam", old);
        let copy = add(&db, "Old entry gog", "gog", new);
        add(&db, "Zeta", "steam", old);
        add(&db, "Alpha", "steam", old);
        db.combine_games(entry, &[copy]).unwrap();

        let shelf = db.recently_added(15).unwrap();
        assert_eq!(names(&shelf), ["Old entry", "Alpha", "Zeta"]);
        assert_eq!(shelf[0].date, new);
        assert_eq!(shelf[0].game.copies.len(), 2);
    }

    #[test]
    fn installed_by_store_counts_copies_per_store() {
        let db = Database::open_in_memory().unwrap();
        let t = at("2026-01-01T00:00:00Z");
        let a = add(&db, "A", "steam", t);
        let a_gog = add(&db, "A gog", "gog", t);
        let b = add(&db, "B", "steam", t);
        let hidden = add(&db, "H", "steam", t);
        add(&db, "Manual", MANUAL_SOURCE, t);
        db.combine_games(a, &[a_gog]).unwrap();
        set(&db, a, "installed = 0");
        set(&db, a_gog, "installed = 1");
        set(&db, b, "installed = 1");
        set(&db, hidden, "hidden = 1, installed = 1");

        let stores = db.installed_by_store().unwrap();
        let expected = [
            StoreInstalls {
                source: "steam".into(),
                installed: 1,
                owned: 2,
            },
            StoreInstalls {
                source: "gog".into(),
                installed: 1,
                owned: 1,
            },
        ];
        assert_eq!(stores, expected);
    }

    #[test]
    fn sessions_since_includes_open_sessions_up_to_their_heartbeat() {
        let db = Database::open_in_memory().unwrap();
        let t = at("2026-10-01T10:00:00Z");
        let id = add(&db, "A", "steam", t);
        let old = db.begin_session(id, t - Duration::days(30)).unwrap();
        db.finish_session(old, t - Duration::days(30) + Duration::hours(1), 3600)
            .unwrap();
        let done = db.begin_session(id, t).unwrap();
        db.finish_session(done, t + Duration::hours(2), 1800)
            .unwrap();
        let open = db.begin_session(id, t + Duration::hours(3)).unwrap();
        db.heartbeat_session(open, t + Duration::hours(3) + Duration::minutes(10))
            .unwrap();

        let spans = db.sessions_since(t - Duration::days(1)).unwrap();
        assert_eq!(
            spans,
            [
                SessionSpan {
                    start: t,
                    end: t + Duration::hours(2),
                    credited: 1800
                },
                SessionSpan {
                    start: t + Duration::hours(3),
                    end: t + Duration::hours(3) + Duration::minutes(10),
                    credited: 600
                },
            ]
        );
    }

    #[test]
    fn last_page_roundtrips() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.last_page().unwrap(), None);
        db.set_last_page("home").unwrap();
        db.set_last_page("fav").unwrap();
        assert_eq!(db.last_page().unwrap().as_deref(), Some("fav"));
    }

    #[test]
    fn playtime_splits_sessions_at_local_midnight_by_credited_time() {
        // UTC+2: local midnight of 10-05 is 10-04T22:00Z.
        let tz = FixedOffset::east_opt(2 * 3600).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let sessions = [
            // 23:00-01:00 local, 2 h wall clock, 1 h credited (sleep skipped): half on each day.
            SessionSpan {
                start: at("2026-10-04T21:00:00Z"),
                end: at("2026-10-04T23:00:00Z"),
                credited: 3600,
            },
            // Today, 30 min.
            SessionSpan {
                start: at("2026-10-05T08:00:00Z"),
                end: at("2026-10-05T08:30:00Z"),
                credited: 1800,
            },
            // Before the window.
            SessionSpan {
                start: at("2026-09-01T08:00:00Z"),
                end: at("2026-09-01T09:00:00Z"),
                credited: 3600,
            },
        ];
        let days = playtime_by_day(&sessions, today, &tz, 3);
        let got: Vec<(String, u64)> = days
            .iter()
            .map(|d| (d.date.to_string(), d.seconds))
            .collect();
        assert_eq!(
            got,
            [
                ("2026-10-03".to_string(), 0),
                ("2026-10-04".to_string(), 1800),
                ("2026-10-05".to_string(), 3600),
            ]
        );
    }

    #[test]
    fn zero_length_sessions_count_on_their_start_day() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let t = at("2026-10-05T12:00:00Z");
        let sessions = [SessionSpan {
            start: t,
            end: t,
            credited: 60,
        }];
        let days = playtime_by_day(&sessions, today, &tz, 7);
        assert_eq!(days.len(), 7);
        assert_eq!(days.last().map(|d| d.seconds), Some(60));
    }
}
