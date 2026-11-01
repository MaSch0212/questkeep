//! Store accounts seen signed in, in `app_state`, so a sign-in that later ends (an expired
//! session) can be pointed out until the user signs in again or dismisses it.

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use super::Database;
use crate::error::Result;

/// `app_state` key prefix; the provider id follows.
const PREFIX: &str = "account.";

/// What is remembered of an account that was signed in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownAccount {
    pub display_name: Option<String>,
    /// The user dismissed the warning that it is signed out.
    #[serde(default)]
    pub dismissed: bool,
}

impl Database {
    /// The account as last seen signed in, if it ever was (and was not signed out on purpose).
    pub fn known_account(&self, provider: &str) -> Result<Option<KnownAccount>> {
        let value: Option<String> = self
            .conn()
            .query_row(
                "SELECT value FROM app_state WHERE key = ?1",
                [format!("{PREFIX}{provider}")],
                |r| r.get(0),
            )
            .optional()?;
        // An unreadable entry still says the account was signed in.
        Ok(value.map(|v| serde_json::from_str(&v).unwrap_or_default()))
    }

    pub fn set_known_account(&self, provider: &str, account: &KnownAccount) -> Result<()> {
        let value = serde_json::to_string(account)
            .map_err(|e| crate::error::CoreError::Invalid(e.to_string()))?;
        self.conn().execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
            [format!("{PREFIX}{provider}"), value],
        )?;
        Ok(())
    }

    /// Forgets the account (signed out on purpose: nothing to warn about).
    pub fn forget_known_account(&self, provider: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM app_state WHERE key = ?1",
            [format!("{PREFIX}{provider}")],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_accounts_round_trip_and_can_be_forgotten() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.known_account("steam").unwrap(), None);
        let account = KnownAccount {
            display_name: Some("Ada".into()),
            dismissed: true,
        };
        db.set_known_account("steam", &account).unwrap();
        assert_eq!(db.known_account("steam").unwrap(), Some(account));
        db.forget_known_account("steam").unwrap();
        assert_eq!(db.known_account("steam").unwrap(), None);
    }
}
