//! Reading the SQLite databases of other programs (Lutris, GOG Galaxy) without disturbing them.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::ProviderError;

/// Runs `read` on the database at `path`, opened read-only. When that fails (the owner holds a
/// lock, or a WAL database's shared-memory file can't be opened read-only), it reads a temporary
/// copy instead; the `-wal` file is copied too, so recent changes the owner has not
/// checkpointed yet are not lost. `what` names the database in error messages.
pub fn read_foreign<T>(
    path: &Path,
    what: &str,
    read: impl Fn(&Connection) -> rusqlite::Result<T>,
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
    let result = read_copy(path, &read);
    result.map_err(|e| {
        ProviderError::Parse(format!(
            "could not read the {what} {}: {first}; copy: {e}",
            path.display()
        ))
    })
}

fn read_copy<T>(
    path: &Path,
    read: impl Fn(&Connection) -> rusqlite::Result<T>,
) -> Result<T, String> {
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

/// The most rows [`query_rows`] answers with: a plugin cannot make the app hold a whole huge
/// table.
pub const MAX_QUERY_ROWS: usize = 200_000;

/// The answer of [`query_rows`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct QueryRows {
    pub columns: Vec<String>,
    /// Numbers, text and null; a blob is `{"blob": "<hex>"}`.
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// Runs one read-only statement on another program's database, like [`read_foreign`] (a locked
/// database is read from a copy). `params` are bound as text. Statements that would change
/// something, and several statements at once, are refused.
pub fn query_rows(
    path: &Path,
    what: &str,
    sql: &str,
    params: &[String],
) -> Result<QueryRows, ProviderError> {
    read_foreign(path, what, |conn| run_query(conn, sql, params))
}

fn run_query(conn: &Connection, sql: &str, params: &[String]) -> rusqlite::Result<QueryRows> {
    use serde_json::Value;
    // Only reading statements: `ATTACH` and some `PRAGMA`s count as read-only to SQLite but can
    // create files or reach other databases.
    let first_word: String = sql
        .trim_start()
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    if !["select", "with", "values"].contains(&first_word.to_ascii_lowercase().as_str()) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let mut stmt = conn.prepare(sql)?;
    if !stmt.readonly() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let columns: Vec<String> = stmt.column_names().map(|c| c.to_string()).collect();
    let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        if out.len() >= MAX_QUERY_ROWS {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let mut values = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            values.push(match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(n) => Value::from(n),
                rusqlite::types::ValueRef::Real(f) => {
                    serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
                }
                rusqlite::types::ValueRef::Text(t) => {
                    Value::String(String::from_utf8_lossy(t).into_owned())
                }
                rusqlite::types::ValueRef::Blob(b) => {
                    let hex: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
                    serde_json::json!({ "blob": hex })
                }
            });
        }
        out.push(values);
    }
    Ok(QueryRows { columns, rows: out })
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

    #[test]
    fn query_rows_answers_json_values_and_binds_text_parameters() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("q.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE t(id INTEGER, name TEXT, score REAL, data BLOB, note TEXT);
             INSERT INTO t VALUES (1, 'one', 1.5, x'0aff', NULL), (2, 'two', 2.0, NULL, 'n');",
        )
        .unwrap();
        drop(conn);
        let all = query_rows(&path, "test", "SELECT * FROM t ORDER BY id", &[]).unwrap();
        assert_eq!(all.columns, ["id", "name", "score", "data", "note"]);
        assert_eq!(all.rows[0][0], 1);
        assert_eq!(all.rows[0][1], "one");
        assert_eq!(all.rows[0][2], 1.5);
        assert_eq!(all.rows[0][3], serde_json::json!({ "blob": "0aff" }));
        assert!(all.rows[0][4].is_null());
        let one = query_rows(
            &path,
            "test",
            "SELECT name FROM t WHERE name = ?",
            &["two".to_string()],
        )
        .unwrap();
        assert_eq!(one.rows, vec![vec![serde_json::json!("two")]]);
    }

    #[test]
    fn query_rows_refuses_statements_that_write() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("q.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES (1);")
            .unwrap();
        for sql in [
            "DELETE FROM t",
            "INSERT INTO t VALUES (2)",
            "DROP TABLE t",
            "CREATE TABLE u(y)",
            "ATTACH DATABASE 'x.db' AS other",
            "PRAGMA journal_mode = DELETE",
        ] {
            assert!(query_rows(&path, "test", sql, &[]).is_err(), "{sql}");
        }
        // Nothing changed.
        let rows = query_rows(&path, "test", "SELECT count(*) FROM t", &[]).unwrap();
        assert_eq!(rows.rows[0][0], 1);
    }
}
