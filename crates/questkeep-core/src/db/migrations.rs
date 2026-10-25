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
    // 3: saved library views in sidebar sections, and when each store last synced.
    // `query` is GameQuery JSON; missing fields take their defaults.
    r#"
    CREATE TABLE view_sections (
        id       TEXT PRIMARY KEY NOT NULL,
        position INTEGER NOT NULL,
        name     TEXT NOT NULL
    );
    CREATE TABLE views (
        id         TEXT PRIMARY KEY NOT NULL,
        section_id TEXT NOT NULL REFERENCES view_sections (id) ON DELETE CASCADE,
        position   INTEGER NOT NULL,
        name       TEXT NOT NULL,
        icon       TEXT NOT NULL,
        query      TEXT NOT NULL DEFAULT '{}',
        layout     TEXT NOT NULL DEFAULT 'grid'
    );
    CREATE INDEX views_section ON views (section_id, position);

    INSERT INTO view_sections (id, position, name) VALUES
        ('library', 0, 'Library'),
        ('other', 1, 'Other');
    INSERT INTO views (id, section_id, position, name, icon, query, layout) VALUES
        ('all', 'library', 0, 'All games', 'library', '{}', 'grid'),
        ('favorites', 'library', 1, 'Favorites', 'star', '{"favorite":"yes"}', 'grid'),
        ('installed', 'library', 2, 'Installed', 'hard-drive', '{"installed":"yes"}', 'list'),
        ('recent', 'library', 3, 'Recently played', 'clock',
            '{"played":"played","sort":"lastPlayed","descending":true}', 'list'),
        ('hidden', 'other', 0, 'Hidden', 'eye-off', '{"hidden":"hidden"}', 'list');

    CREATE TABLE sync_times (
        provider    TEXT PRIMARY KEY NOT NULL,
        finished_at TEXT NOT NULL
    );
    "#,
    // 4: categories (n:m with games) and grouping in views. `source` is the store whose import
    // created a category; `imported_from` is the store that set a link (NULL: the user did), so a
    // rescan only replaces its own links.
    r#"
    CREATE TABLE categories (
        id     TEXT PRIMARY KEY NOT NULL,
        name   TEXT NOT NULL,
        source TEXT
    );
    CREATE UNIQUE INDEX categories_name ON categories (name COLLATE NOCASE);
    CREATE TABLE game_categories (
        game_id       TEXT NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        category_id   TEXT NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
        imported_from TEXT,
        PRIMARY KEY (game_id, category_id)
    );
    CREATE INDEX game_categories_category ON game_categories (category_id);

    ALTER TABLE views ADD COLUMN group_by TEXT NOT NULL DEFAULT 'none';
    "#,
    // 5: category order and store mappings. A store's own tags are kept as the store reports
    // them (`game_tags`) and reach categories through `category_sources`, so merging, separating
    // and renaming only move mappings. `game_categories` keeps the user's links and, with
    // `excluded`, the imported categories the user took off a game. `game_category_links` is
    // what a game ends up with.
    r#"
    ALTER TABLE categories ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
    UPDATE categories SET position =
        (SELECT COUNT(*) FROM categories c WHERE c.name < categories.name COLLATE NOCASE);

    CREATE TABLE category_sources (
        source      TEXT NOT NULL,
        name        TEXT NOT NULL COLLATE NOCASE,
        category_id TEXT NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
        PRIMARY KEY (source, name)
    );
    CREATE INDEX category_sources_category ON category_sources (category_id);
    INSERT OR IGNORE INTO category_sources (source, name, category_id)
        SELECT source, name, id FROM categories WHERE source IS NOT NULL;
    INSERT OR IGNORE INTO category_sources (source, name, category_id)
        SELECT DISTINCT gc.imported_from, c.name, c.id
        FROM game_categories gc JOIN categories c ON c.id = gc.category_id
        WHERE gc.imported_from IS NOT NULL;

    CREATE TABLE game_tags (
        game_id TEXT NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        source  TEXT NOT NULL,
        name    TEXT NOT NULL COLLATE NOCASE,
        PRIMARY KEY (game_id, name)
    );
    CREATE INDEX game_tags_source ON game_tags (source, name);
    INSERT OR IGNORE INTO game_tags (game_id, source, name)
        SELECT gc.game_id, gc.imported_from, c.name
        FROM game_categories gc JOIN categories c ON c.id = gc.category_id
        WHERE gc.imported_from IS NOT NULL;

    CREATE TABLE user_game_categories (
        game_id     TEXT NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        category_id TEXT NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
        excluded    INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (game_id, category_id)
    );
    INSERT INTO user_game_categories (game_id, category_id)
        SELECT game_id, category_id FROM game_categories WHERE imported_from IS NULL;
    DROP TABLE game_categories;
    ALTER TABLE user_game_categories RENAME TO game_categories;
    CREATE INDEX game_categories_category ON game_categories (category_id);
    ALTER TABLE categories DROP COLUMN source;

    CREATE VIEW game_category_links (game_id, category_id) AS
        SELECT game_id, category_id FROM game_categories WHERE excluded = 0
        UNION
        SELECT t.game_id, s.category_id
        FROM game_tags t JOIN category_sources s ON s.source = t.source AND s.name = t.name
        WHERE NOT EXISTS (SELECT 1 FROM game_categories x
            WHERE x.game_id = t.game_id AND x.category_id = s.category_id AND x.excluded = 1);
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
    fn moves_imported_category_links_to_store_tags() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (4);",
        )
        .unwrap();
        let tx = conn.transaction().unwrap();
        for step in &MIGRATIONS[..4] {
            tx.execute_batch(step).unwrap();
        }
        tx.execute_batch(
            "INSERT INTO games (id, name, sorting_name, search_key, source, date_added) VALUES
                 ('g', 'G', 'G', 'g', 'steam', '2026-01-01T00:00:00Z');
             INSERT INTO categories (id, name, source) VALUES
                 ('rpg', 'RPG', 'steam'), ('mine', 'Backlog', NULL), ('coop', 'Co-op', NULL);
             INSERT INTO game_categories (game_id, category_id, imported_from) VALUES
                 ('g', 'rpg', 'steam'), ('g', 'mine', NULL), ('g', 'coop', 'steam');",
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(migrate(&mut conn).unwrap(), LATEST_VERSION);
        let rows = |sql: &str| -> Vec<String> {
            conn.prepare(sql)
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        };
        assert_eq!(
            rows("SELECT name FROM categories ORDER BY position"),
            vec!["Backlog", "Co-op", "RPG"]
        );
        assert_eq!(
            rows("SELECT category_id FROM category_sources ORDER BY name"),
            vec!["coop", "rpg"]
        );
        assert_eq!(
            rows("SELECT name FROM game_tags ORDER BY name"),
            vec!["Co-op", "RPG"]
        );
        assert_eq!(
            rows("SELECT category_id FROM game_categories"),
            vec!["mine"]
        );
        assert_eq!(
            rows("SELECT category_id FROM game_category_links ORDER BY category_id"),
            vec!["coop", "mine", "rpg"]
        );
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
