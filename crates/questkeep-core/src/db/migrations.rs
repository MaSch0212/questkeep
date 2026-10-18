//! Ordered schema migrations. Never edit a released step; append a new one instead.

use rusqlite::{Connection, OptionalExtension};

use crate::error::Result;

/// Migration steps; index + 1 is the schema version the step produces.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
    CREATE TABLE games (
        id               TEXT PRIMARY KEY NOT NULL,
        name             TEXT NOT NULL,
        sorting_name     TEXT NOT NULL,
        search_key       TEXT NOT NULL,
        source           TEXT NOT NULL,
        source_game_id   TEXT,
        installed        INTEGER NOT NULL DEFAULT 1,
        install_dir      TEXT,
        play_action      TEXT,
        playtime_seconds INTEGER NOT NULL DEFAULT 0,
        play_count       INTEGER NOT NULL DEFAULT 0,
        last_played      TEXT,
        date_added       TEXT NOT NULL,
        favorite         INTEGER NOT NULL DEFAULT 0,
        hidden           INTEGER NOT NULL DEFAULT 0,
        cover_path       TEXT,
        icon_path        TEXT,
        extra            TEXT NOT NULL DEFAULT '{}',
        UNIQUE (source, source_game_id)
    );
    CREATE INDEX games_sorting_name ON games (sorting_name COLLATE NOCASE);
    CREATE INDEX games_source ON games (source);

    CREATE TABLE play_sessions (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        game_id          TEXT NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        started_at       TEXT NOT NULL,
        last_seen_at     TEXT NOT NULL,
        ended_at         TEXT,
        duration_seconds INTEGER
    );
    CREATE INDEX play_sessions_open ON play_sessions (game_id) WHERE ended_at IS NULL;
    "#,
    // 2: store metadata and background images
    r#"
    ALTER TABLE games ADD COLUMN description TEXT;
    ALTER TABLE games ADD COLUMN release_date TEXT;
    ALTER TABLE games ADD COLUMN developers TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE games ADD COLUMN publishers TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE games ADD COLUMN genres TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE games ADD COLUMN background_path TEXT;
    ALTER TABLE games ADD COLUMN metadata_updated_at TEXT;
    "#,
];

/// Latest schema version this build knows.
pub const LATEST_VERSION: u32 = MIGRATIONS.len() as u32;

/// Returns the current schema version (0 for a fresh database).
pub fn current_version(conn: &Connection) -> Result<u32> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")?;
    let version: Option<u32> = conn
        .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(version.unwrap_or(0))
}

/// Applies all pending migrations, each in its own transaction.
pub fn migrate(conn: &mut Connection) -> Result<u32> {
    let mut version = current_version(conn)?;
    if version > LATEST_VERSION {
        return Err(crate::CoreError::Invalid(format!(
            "The library database was created by a newer Questkeep (schema {version}, this build supports {LATEST_VERSION}). Please update Questkeep."
        )));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let target = index as u32 + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute("DELETE FROM schema_version", [])?;
        tx.execute("INSERT INTO schema_version (version) VALUES (?1)", [target])?;
        tx.commit()?;
        tracing::info!(version = target, "applied database migration");
        version = target;
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_fresh_database_to_latest() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(current_version(&conn).unwrap(), 0);
        assert_eq!(migrate(&mut conn).unwrap(), LATEST_VERSION);
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        // Tables exist.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn migrate_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(migrate(&mut conn).unwrap(), LATEST_VERSION);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn upgrades_a_version_1_database_keeping_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (0);",
        )
        .unwrap();
        // Apply only step 1, insert a row, then migrate the rest.
        let tx = conn.transaction().unwrap();
        tx.execute_batch(MIGRATIONS[0]).unwrap();
        tx.execute("UPDATE schema_version SET version = 1", [])
            .unwrap();
        tx.execute(
            "INSERT INTO games (id, name, sorting_name, search_key, source, date_added) \
             VALUES ('a', 'A', 'A', 'a', 'manual', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(migrate(&mut conn).unwrap(), LATEST_VERSION);
        let genres: String = conn
            .query_row("SELECT genres FROM games WHERE id = 'a'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(genres, "[]");
    }

    #[test]
    fn refuses_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        conn.execute("UPDATE schema_version SET version = 999", [])
            .unwrap();
        assert!(migrate(&mut conn).is_err());
    }
}
