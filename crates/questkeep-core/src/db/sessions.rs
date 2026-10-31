//! Play sessions: persisted while a game runs so playtime survives app exits and crashes.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};

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
        let conn = self.conn();
        conn.execute(
            "INSERT INTO play_sessions (game_id, started_at, last_seen_at) VALUES (?1, ?2, ?2)",
            params![game_id.to_string(), now],
        )?;
        Ok(conn.last_insert_rowid())
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
