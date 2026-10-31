//! Copies of the library database taken before migrations change its schema, so a build that
//! migrated by mistake (or a broken migration) can be undone by restoring the copy.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{CoreError, Result};

/// Folder next to the database file that holds the backups.
pub const BACKUP_DIR: &str = "backups";

/// Pre-migration backups kept; older ones are deleted after a new one is written.
pub const KEEP_BACKUPS: usize = 5;

const PREFIX: &str = "library-";
const SUFFIX: &str = ".sqlite";

/// Writes a consistent copy of the open database (including WAL content) to
/// `<db dir>/backups/library-<UTC time>-schema<version>.sqlite`, then prunes old backups.
/// File names sort chronologically, so pruning keeps the last [`KEEP_BACKUPS`] by name.
pub fn backup_before_migration(conn: &Connection, db_path: &Path, version: u32) -> Result<PathBuf> {
    let dir = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(BACKUP_DIR);
    fs::create_dir_all(&dir).map_err(|e| backup_error(&dir, &e))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ");
    let target = dir.join(format!("{PREFIX}{stamp}-schema{version}{SUFFIX}"));
    let target_str = target.to_str().ok_or_else(|| {
        CoreError::Invalid(format!(
            "Backup path {} is not valid UTF-8.",
            target.display()
        ))
    })?;
    conn.execute("VACUUM INTO ?1", [target_str])
        .map_err(|e| backup_error(&target, &e))?;
    prune(&dir);
    Ok(target)
}

fn backup_error(path: &Path, error: &dyn std::fmt::Display) -> CoreError {
    CoreError::Invalid(format!(
        "Could not back up the library database to {} before updating it ({error}). The database was not changed.",
        path.display()
    ))
}

/// Deletes all but the newest [`KEEP_BACKUPS`] backups. Failures only log: a leftover file is
/// harmless.
fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(SUFFIX))
        })
        .collect();
    backups.sort();
    let excess = backups.len().saturating_sub(KEEP_BACKUPS);
    for old in &backups[..excess] {
        if let Err(error) = fs::remove_file(old) {
            tracing::warn!(path = %old.display(), %error, "could not delete old database backup");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::migrations::{self, LATEST_VERSION};

    fn backups(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir.join(BACKUP_DIR))
            .map(|r| {
                r.map(|e| e.unwrap().file_name().into_string().unwrap())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// Creates a database file at schema 1 holding one marker row.
    fn old_database(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (1);",
        )
        .unwrap();
        conn.execute_batch(migrations::MIGRATIONS[0]).unwrap();
        conn.execute_batch(
            "INSERT INTO games (id, name, sorting_name, search_key, source, date_added) VALUES
                 ('g', 'G', 'G', 'g', 'steam', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    #[test]
    fn opening_an_old_database_backs_it_up_first() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.sqlite");
        old_database(&path);

        Database::open(&path).unwrap();

        let names = backups(tmp.path());
        assert_eq!(names.len(), 1, "{names:?}");
        assert!(names[0].starts_with("library-") && names[0].ends_with("-schema1.sqlite"));
        let copy = Connection::open(tmp.path().join(BACKUP_DIR).join(&names[0])).unwrap();
        assert_eq!(migrations::current_version(&copy).unwrap(), 1);
        let games: i64 = copy
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
            .unwrap();
        assert_eq!(games, 1);
        let live = Connection::open(&path).unwrap();
        assert_eq!(migrations::current_version(&live).unwrap(), LATEST_VERSION);
    }

    #[test]
    fn new_and_current_databases_are_not_backed_up() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.sqlite");
        Database::open(&path).unwrap();
        Database::open(&path).unwrap();
        assert!(backups(tmp.path()).is_empty());
    }

    #[test]
    fn keeps_only_the_newest_backups() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(BACKUP_DIR);
        fs::create_dir_all(&dir).unwrap();
        for i in 0..KEEP_BACKUPS + 2 {
            fs::write(
                dir.join(format!("library-2020010{i}T000000000Z-schema1.sqlite")),
                "",
            )
            .unwrap();
        }
        fs::write(dir.join("notes.txt"), "").unwrap();
        let conn = Connection::open_in_memory().unwrap();

        let written =
            backup_before_migration(&conn, &tmp.path().join("library.sqlite"), 3).unwrap();

        let names = backups(tmp.path());
        assert_eq!(names.len(), KEEP_BACKUPS + 1, "{names:?}");
        assert!(names.contains(&"notes.txt".to_string()));
        assert!(!names.contains(&"library-20200100T000000000Z-schema1.sqlite".to_string()));
        assert!(!names.contains(&"library-20200101T000000000Z-schema1.sqlite".to_string()));
        assert!(written.exists());
    }

    #[test]
    fn a_failed_backup_leaves_the_database_unmigrated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.sqlite");
        old_database(&path);
        // A file where the backup folder should be makes the backup fail.
        fs::write(tmp.path().join(BACKUP_DIR), "").unwrap();

        assert!(Database::open(&path).is_err());

        let live = Connection::open(&path).unwrap();
        assert_eq!(migrations::current_version(&live).unwrap(), 1);
    }
}
