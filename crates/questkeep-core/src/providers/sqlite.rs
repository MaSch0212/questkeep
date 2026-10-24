//! Reading the SQLite databases of other programs (Lutris, GOG Galaxy) without disturbing them.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use super::ProviderError;

/// Runs `read` on the database at `path`, opened read-only. When that fails (the owner holds a
/// lock, or a WAL database's shared-memory file can't be opened read-only), it reads a temporary
/// copy instead; the `-wal` file is copied too, so recent changes the owner has not
/// checkpointed yet are not lost. `what` names the database in error messages.
pub(super) fn read_foreign<T>(
    path: &Path,
    what: &str,
    read: fn(&Connection) -> rusqlite::Result<T>,
) -> Result<T, ProviderError> {
    let first = match Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .and_then(|conn| read(&conn))
    {
        Ok(value) => return Ok(value),
        Err(e) => e,
    };
    tracing::debug!(
        "reading {} directly failed ({first}), trying a copy",
        path.display()
    );
    let result = read_copy(path, read);
    result.map_err(|e| {
        ProviderError::Parse(format!(
            "could not read the {what} {}: {first}; copy: {e}",
            path.display()
        ))
    })
}

fn read_copy<T>(path: &Path, read: fn(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
    let dir = std::env::temp_dir().join(format!("questkeep-db-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let copy = dir.join("copy.db");
        std::fs::copy(path, &copy).map_err(|e| e.to_string())?;
        let mut wal = path.as_os_str().to_owned();
        wal.push("-wal");
        if Path::new(&wal).is_file() {
            std::fs::copy(&wal, dir.join("copy.db-wal")).map_err(|e| e.to_string())?;
        }
        // Our own copy: opened writable so SQLite can replay the WAL into it.
        let conn = Connection::open(&copy).map_err(|e| e.to_string())?;
        read(&conn).map_err(|e| e.to_string())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(conn: &Connection) -> rusqlite::Result<i64> {
        conn.query_row("SELECT count(*) FROM t", [], |r| r.get(0))
    }

    #[test]
    fn reads_directly_and_reports_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES (1), (2);")
            .unwrap();
        drop(conn);
        assert_eq!(read_foreign(&path, "test database", count).unwrap(), 2);
        let err = read_foreign(&tmp.path().join("nope.db"), "test database", count).unwrap_err();
        assert!(err.to_string().contains("test database"));
    }

    #[test]
    fn a_copy_keeps_uncheckpointed_wal_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("w.db");
        let writer = Connection::open(&path).unwrap();
        writer
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; \
                 CREATE TABLE t(x); INSERT INTO t VALUES (1), (2), (3);",
            )
            .unwrap();
        // The writer stays open, so the rows live only in the -wal file.
        assert_eq!(read_copy(&path, count).unwrap(), 3);
        drop(writer);
    }
}
