//! First-run setup flags in `app_state`.

use super::Database;
use crate::error::Result;

/// Written when the database file is created (see `migrations::migrate`).
pub(super) const FRESH: &str = "setup.fresh";
/// Written when the setup is finished or skipped.
const DONE: &str = "setup.done";
/// Written when the "no IGDB key" hint is dismissed.
const IGDB_HINT_DISMISSED: &str = "setup.igdbHintDismissed";

impl Database {
    /// Whether the first-run setup is still due: the database is fresh and setup never ended.
    pub fn setup_pending(&self) -> Result<bool> {
        Ok(self.has_state(FRESH)? && !self.has_state(DONE)?)
    }

    pub fn finish_setup(&self) -> Result<()> {
        self.set_state(DONE)
    }

    pub fn igdb_hint_dismissed(&self) -> Result<bool> {
        self.has_state(IGDB_HINT_DISMISSED)
    }

    pub fn dismiss_igdb_hint(&self) -> Result<()> {
        self.set_state(IGDB_HINT_DISMISSED)
    }

    fn has_state(&self, key: &str) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM app_state WHERE key = ?1)",
            [key],
            |r| r.get(0),
        )?)
    }

    fn set_state(&self, key: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, '1')",
            [key],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_database_needs_setup_until_it_is_finished() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.setup_pending().unwrap());
        db.finish_setup().unwrap();
        assert!(!db.setup_pending().unwrap());
    }

    #[test]
    fn an_existing_database_never_needs_setup() {
        let db = Database::open_in_memory().unwrap();
        db.conn()
            .execute("DELETE FROM app_state WHERE key = ?1", [FRESH])
            .unwrap();
        assert!(!db.setup_pending().unwrap());
    }

    #[test]
    fn the_igdb_hint_stays_dismissed() {
        let db = Database::open_in_memory().unwrap();
        assert!(!db.igdb_hint_dismissed().unwrap());
        db.dismiss_igdb_hint().unwrap();
        assert!(db.igdb_hint_dismissed().unwrap());
    }
}
