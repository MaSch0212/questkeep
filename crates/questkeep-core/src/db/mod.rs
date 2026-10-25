//! SQLite persistence: connection handling, migrations and repositories.

mod categories;
mod games;
pub mod migrations;
mod sessions;
mod views;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;

use crate::error::Result;

pub use games::{GameMissingMetadata, NewGameRecord};
pub use sessions::OpenSession;

/// The library database. One connection guarded by a mutex: SQLite serializes writes anyway and
/// every query here takes well under a millisecond for realistic library sizes.
#[derive(Debug)]
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Opens (or creates) the database file and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    /// In-memory database for tests and tooling.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Locks the connection. A poisoned lock is recovered: SQLite rolls back any transaction that
    /// was open when the panicking thread dropped it, so the data stays consistent.
    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
