//! Play sessions: persisted while a game runs so playtime survives app exits and crashes.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::GameSession;

fn to_i64(seconds: u64) -> i64 {
    i64::try_from(seconds).unwrap_or(i64::MAX)
}

/// When a session that started at `started_at` and was credited `seconds` ended.
fn session_end(started_at: DateTime<Utc>, seconds: u64) -> Result<DateTime<Utc>> {
    i64::try_from(seconds)
        .ok()
        .and_then(chrono::Duration::try_seconds)
        .and_then(|d| started_at.checked_add_signed(d))
        .ok_or_else(|| CoreError::Invalid("The session is too long.".into()))
}

fn parse_game_id(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|e| CoreError::Invalid(format!("Corrupt session game id: {e}")))
}

/// The game and credited seconds of a finished session.
fn finished_session(tx: &rusqlite::Transaction<'_>, session_id: i64) -> Result<(String, i64)> {
    tx.query_row(
        "SELECT game_id, COALESCE(duration_seconds, 0) FROM play_sessions \
         WHERE id = ?1 AND ended_at IS NOT NULL",
        [session_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("This session no longer exists.".into()))
}

/// A session that has not been finished yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSession {
    pub id: i64,
    pub game_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

impl Database {
    /// Starts a session row for `game_id`.
    pub fn begin_session(&self, game_id: Uuid, now: DateTime<Utc>) -> Result<i64> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO play_sessions (game_id, started_at, last_seen_at) VALUES (?1, ?2, ?2)",
                params![game_id.to_string(), now],
            )?;
            conn.last_insert_rowid()
        };
        // The first start moves a new game to the "played" status (if the user keeps that rule).
        self.apply_first_start(game_id)?;
        Ok(id)
    }

    /// Records that the game was still running at `now`.
    pub fn heartbeat_session(&self, session_id: i64, now: DateTime<Utc>) -> Result<()> {
        self.conn().execute(
            "UPDATE play_sessions SET last_seen_at = ?2 WHERE id = ?1 AND ended_at IS NULL",
            params![session_id, now],
        )?;
        Ok(())
    }

    /// Closes a session and adds its duration to the game's playtime, bumps the play count and
    /// sets last played, atomically. Finishing an already finished session is a no-op that
    /// returns `None`.
    pub fn finish_session(
        &self,
        session_id: i64,
        ended_at: DateTime<Utc>,
        duration_seconds: u64,
    ) -> Result<Option<Uuid>> {
        let duration = i64::try_from(duration_seconds).unwrap_or(i64::MAX);
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let game_id: Option<String> = tx
            .query_row(
                "SELECT game_id FROM play_sessions WHERE id = ?1 AND ended_at IS NULL",
                [session_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(game_id) = game_id else {
            return Ok(None);
        };
        tx.execute(
            "UPDATE play_sessions SET ended_at = ?2, last_seen_at = ?2, duration_seconds = ?3 \
             WHERE id = ?1",
            params![session_id, ended_at, duration],
        )?;
        tx.execute(
            "UPDATE games SET playtime_seconds = playtime_seconds + ?2, \
             play_count = play_count + 1, last_played = ?3 WHERE id = ?1",
            params![game_id, duration, ended_at],
        )?;
        tx.commit()?;
        Uuid::parse_str(&game_id)
            .map(Some)
            .map_err(|e| CoreError::Invalid(format!("Corrupt session game id: {e}")))
    }

    /// Deletes an unfinished session without crediting anything (a run too short to count).
    pub fn discard_session(&self, session_id: i64) -> Result<()> {
        self.conn().execute(
            "DELETE FROM play_sessions WHERE id = ?1 AND ended_at IS NULL",
            [session_id],
        )?;
        Ok(())
    }

    pub fn open_sessions(&self) -> Result<Vec<OpenSession>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, game_id, started_at, last_seen_at FROM play_sessions \
             WHERE ended_at IS NULL ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, DateTime<Utc>>(2)?,
                r.get::<_, DateTime<Utc>>(3)?,
            ))
        })?;
        let mut sessions = Vec::new();
        for row in rows {
            let (id, game_id, started_at, last_seen_at) = row?;
            if let Ok(game_id) = Uuid::parse_str(&game_id) {
                sessions.push(OpenSession {
                    id,
                    game_id,
                    started_at,
                    last_seen_at,
                });
            }
        }
        Ok(sessions)
    }

    /// The finished sessions of a library entry (all its copies), newest first.
    pub fn entry_sessions(&self, entry: Uuid) -> Result<Vec<GameSession>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, started_at, ended_at, duration_seconds FROM play_sessions \
             WHERE ended_at IS NOT NULL \
             AND game_id IN (SELECT id FROM games WHERE id = ?1 OR merged_into = ?1) \
             ORDER BY started_at DESC, id DESC",
        )?;
        let sessions = stmt
            .query_map([entry.to_string()], |r| {
                let started_at: DateTime<Utc> = r.get(1)?;
                let ended_at: DateTime<Utc> = r.get(2)?;
                let credited = r
                    .get::<_, Option<i64>>(3)?
                    .unwrap_or_else(|| (ended_at - started_at).num_seconds());
                Ok(GameSession {
                    id: r.get(0)?,
                    started_at,
                    ended_at,
                    duration_seconds: u64::try_from(credited).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sessions)
    }

    /// Adds a finished session to a game (its playtime and play count grow, last played moves
    /// up when the session is newer).
    pub fn add_session(
        &self,
        game_id: Uuid,
        started_at: DateTime<Utc>,
        duration_seconds: u64,
    ) -> Result<i64> {
        let ended_at = session_end(started_at, duration_seconds)?;
        let duration = to_i64(duration_seconds);
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO play_sessions (game_id, started_at, last_seen_at, ended_at, \
             duration_seconds) VALUES (?1, ?2, ?3, ?3, ?4)",
            params![game_id.to_string(), started_at, ended_at, duration],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "UPDATE games SET playtime_seconds = playtime_seconds + ?2, \
             play_count = play_count + 1, \
             last_played = MAX(COALESCE(last_played, ?3), ?3) WHERE id = ?1",
            params![game_id.to_string(), duration, ended_at],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Changes a finished session; the game's playtime follows by the difference.
    pub fn update_session(
        &self,
        session_id: i64,
        started_at: DateTime<Utc>,
        duration_seconds: u64,
    ) -> Result<Uuid> {
        let ended_at = session_end(started_at, duration_seconds)?;
        let duration = to_i64(duration_seconds);
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (game_id, before) = finished_session(&tx, session_id)?;
        tx.execute(
            "UPDATE play_sessions SET started_at = ?2, last_seen_at = ?3, ended_at = ?3, \
             duration_seconds = ?4 WHERE id = ?1",
            params![session_id, started_at, ended_at, duration],
        )?;
        tx.execute(
            "UPDATE games SET playtime_seconds = MAX(0, playtime_seconds + ?2 - ?3), \
             last_played = MAX(COALESCE(last_played, ?4), ?4) WHERE id = ?1",
            params![game_id, duration, before, ended_at],
        )?;
        tx.commit()?;
        parse_game_id(&game_id)
    }

    /// Deletes a finished session and takes its time and its play count off the game.
    pub fn delete_session(&self, session_id: i64) -> Result<Uuid> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let (game_id, before) = finished_session(&tx, session_id)?;
        let ended_at: DateTime<Utc> = tx.query_row(
            "SELECT ended_at FROM play_sessions WHERE id = ?1",
            [session_id],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM play_sessions WHERE id = ?1", [session_id])?;
        tx.execute(
            "UPDATE games SET playtime_seconds = MAX(0, playtime_seconds - ?2), \
             play_count = MAX(0, play_count - 1) WHERE id = ?1",
            params![game_id, before],
        )?;
        // Last played is recalculated only when the deleted session was the latest one (a
        // store's own, newer date stays).
        tx.execute(
            "UPDATE games SET last_played = (SELECT MAX(ended_at) FROM play_sessions \
                 WHERE game_id = ?1 AND ended_at IS NOT NULL) \
             WHERE id = ?1 AND last_played = ?2",
            params![game_id, ended_at],
        )?;
        tx.commit()?;
        parse_game_id(&game_id)
    }

    /// Closes sessions left open by a crash, crediting the time up to their last heartbeat.
    /// Sessions without any recorded time are dropped. Returns the ids of games whose playtime
    /// changed.
    pub fn recover_open_sessions(&self) -> Result<Vec<Uuid>> {
        let mut changed = Vec::new();
        for session in self.open_sessions()? {
            let seconds = (session.last_seen_at - session.started_at)
                .num_seconds()
                .max(0);
            let seconds = u64::try_from(seconds).unwrap_or_default();
            if seconds == 0 {
                self.discard_session(session.id)?;
                continue;
            }
            if let Some(game_id) = self.finish_session(session.id, session.last_seen_at, seconds)? {
                tracing::info!(%game_id, seconds, "recovered unfinished play session");
                changed.push(game_id);
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::db::NewGameRecord;

    fn setup() -> (Database, Uuid) {
        let db = Database::open_in_memory().unwrap();
        let g = db
            .insert_game(
                &NewGameRecord {
                    name: "G".into(),
                    source: "manual".into(),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap();
        (db, g.id)
    }

    #[test]
    fn sessions_can_be_added_edited_and_deleted() {
        let (db, id) = setup();
        let t0 = Utc::now() - Duration::days(3);
        let a = db.add_session(id, t0, 600).unwrap();
        let b = db.add_session(id, t0 + Duration::hours(5), 1200).unwrap();
        let g = db.get_game(id).unwrap();
        assert_eq!((g.playtime_seconds, g.play_count), (1800, 2));
        assert_eq!(
            g.last_played,
            Some(t0 + Duration::hours(5) + Duration::seconds(1200))
        );
        let listed = db.entry_sessions(id).unwrap();
        assert_eq!(listed.iter().map(|s| s.id).collect::<Vec<_>>(), [b, a]);

        db.update_session(a, t0, 900).unwrap();
        assert_eq!(db.get_game(id).unwrap().playtime_seconds, 2100);

        db.delete_session(b).unwrap();
        let g = db.get_game(id).unwrap();
        assert_eq!((g.playtime_seconds, g.play_count), (900, 1));
        assert_eq!(g.last_played, Some(t0 + Duration::seconds(900)));

        db.delete_session(a).unwrap();
        let g = db.get_game(id).unwrap();
        assert_eq!(
            (g.playtime_seconds, g.play_count, g.last_played),
            (0, 0, None)
        );
        assert!(db.delete_session(a).is_err());
    }

    #[test]
    fn playtime_accumulates_over_sessions() {
        let (db, id) = setup();
        let t0 = Utc::now();
        let s1 = db.begin_session(id, t0).unwrap();
        assert_eq!(db.open_sessions().unwrap().len(), 1);
        assert_eq!(
            db.finish_session(s1, t0 + Duration::seconds(90), 90)
                .unwrap(),
            Some(id)
        );
        let s2 = db.begin_session(id, t0 + Duration::hours(1)).unwrap();
        let end = t0 + Duration::hours(2);
        db.finish_session(s2, end, 3600).unwrap();

        let g = db.get_game(id).unwrap();
        assert_eq!(g.playtime_seconds, 3690);
        assert_eq!(g.play_count, 2);
        assert_eq!(g.last_played, Some(end));
        assert!(db.open_sessions().unwrap().is_empty());
    }

    #[test]
    fn finishing_twice_is_a_noop() {
        let (db, id) = setup();
        let s = db.begin_session(id, Utc::now()).unwrap();
        db.finish_session(s, Utc::now(), 10).unwrap();
        assert_eq!(db.finish_session(s, Utc::now(), 10).unwrap(), None);
        assert_eq!(db.get_game(id).unwrap().playtime_seconds, 10);
    }

    #[test]
    fn recovery_drops_sessions_without_recorded_time() {
        let (db, id) = setup();
        db.begin_session(id, Utc::now()).unwrap();
        assert!(db.recover_open_sessions().unwrap().is_empty());
        assert!(db.open_sessions().unwrap().is_empty());
        let game = db.get_game(id).unwrap();
        assert_eq!(game.play_count, 0);
        assert!(game.last_played.is_none());
    }

    #[test]
    fn recovers_crashed_sessions_up_to_last_heartbeat() {
        let (db, id) = setup();
        let t0 = Utc::now();
        let s = db.begin_session(id, t0).unwrap();
        db.heartbeat_session(s, t0 + Duration::seconds(300))
            .unwrap();

        assert_eq!(db.recover_open_sessions().unwrap(), vec![id]);
        let g = db.get_game(id).unwrap();
        assert_eq!(g.playtime_seconds, 300);
        assert_eq!(g.play_count, 1);
        assert!(db.recover_open_sessions().unwrap().is_empty());
    }
}
