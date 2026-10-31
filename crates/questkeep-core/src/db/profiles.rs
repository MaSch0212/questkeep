//! Launch profiles (see [`crate::model::LaunchProfile`]): the stored part. A store copy's own
//! profile has a row only once the user changed it; its id is the copy's id.

use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::{PlayAction, sorting_name};

/// A stored profile row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRow {
    pub id: Uuid,
    /// Set for own profiles, `None` for a copy's profile.
    pub entry_id: Option<Uuid>,
    pub copy_id: Option<Uuid>,
    pub name: Option<String>,
    pub args: Option<String>,
    pub action: Option<PlayAction>,
    pub position: i64,
}

fn parse_id(id: Option<String>) -> Option<Uuid> {
    id.and_then(|id| Uuid::parse_str(&id).ok())
}

impl Database {
    /// The stored profiles of an entry: its own and the changed profiles of its copies, by
    /// position.
    pub fn profile_rows(&self, entry_id: Uuid) -> Result<Vec<ProfileRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, entry_id, copy_id, name, args, action, position FROM launch_profiles \
             WHERE entry_id = ?1 OR (entry_id IS NULL AND id IN \
                (SELECT id FROM games WHERE id = ?1 OR merged_into = ?1)) \
             ORDER BY position, rowid",
        )?;
        let rows = stmt
            .query_map([entry_id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, entry, copy, name, args, action, position)| {
                Some(ProfileRow {
                    id: Uuid::parse_str(&id).ok()?,
                    entry_id: parse_id(entry),
                    copy_id: parse_id(copy),
                    name,
                    args,
                    action: action.and_then(|a| serde_json::from_str(&a).ok()),
                    position,
                })
            })
            .collect())
    }

    /// Writes a profile row (insert or replace).
    pub fn save_profile(&self, row: &ProfileRow) -> Result<()> {
        let action = row.action.as_ref().map(serde_json::to_string).transpose()?;
        self.conn().execute(
            "INSERT INTO launch_profiles (id, entry_id, copy_id, name, args, action, position) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT (id) DO UPDATE SET \
             entry_id = excluded.entry_id, copy_id = excluded.copy_id, name = excluded.name, \
             args = excluded.args, action = excluded.action, position = excluded.position",
            params![
                row.id.to_string(),
                row.entry_id.map(|id| id.to_string()),
                row.copy_id.map(|id| id.to_string()),
                row.name,
                row.args,
                action,
                row.position
            ],
        )?;
        Ok(())
    }

    /// Deletes an own profile; a copy's profile only loses its changes.
    pub fn delete_profile(&self, id: Uuid) -> Result<()> {
        self.conn().execute(
            "DELETE FROM launch_profiles WHERE id = ?1",
            [id.to_string()],
        )?;
        self.conn().execute(
            "UPDATE games SET default_profile = NULL WHERE default_profile = ?1",
            [id.to_string()],
        )?;
        Ok(())
    }

    /// The profile the user made the entry's default, if any.
    pub fn default_profile(&self, entry_id: Uuid) -> Result<Option<Uuid>> {
        let id: Option<Option<String>> = self
            .conn()
            .query_row(
                "SELECT default_profile FROM games WHERE id = ?1",
                [entry_id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            None => Err(CoreError::GameNotFound(entry_id)),
            Some(id) => Ok(parse_id(id)),
        }
    }

    pub fn set_default_profile(&self, entry_id: Uuid, profile: Option<Uuid>) -> Result<()> {
        self.conn().execute(
            "UPDATE games SET default_profile = ?2 WHERE id = ?1",
            params![entry_id.to_string(), profile.map(|p| p.to_string())],
        )?;
        Ok(())
    }

    /// The user's own sorting name of a game row.
    pub fn user_sorting_name(&self, id: Uuid) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT sorting_name_user FROM games WHERE id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Sets (or with `None` drops) the user's own sorting name.
    pub fn set_user_sorting_name(&self, id: Uuid, name: Option<&str>) -> Result<()> {
        let name = name.map(str::trim).filter(|n| !n.is_empty());
        let conn = self.conn();
        let game_name: String = conn
            .query_row(
                "SELECT name FROM games WHERE id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(CoreError::GameNotFound(id))?;
        let shown = name.map_or_else(|| sorting_name(&game_name), str::to_string);
        conn.execute(
            "UPDATE games SET sorting_name_user = ?2, sorting_name = ?3 WHERE id = ?1",
            params![id.to_string(), name, shown],
        )?;
        Ok(())
    }

    /// Sets the user's own score (0-100) and notes of an entry.
    pub fn set_user_score_and_notes(
        &self,
        entry_id: Uuid,
        score: Option<u8>,
        notes: Option<&str>,
    ) -> Result<()> {
        if score.is_some_and(|s| s > 100) {
            return Err(CoreError::Invalid("A score goes from 0 to 100.".into()));
        }
        let notes = notes.map(str::trim).filter(|n| !n.is_empty());
        self.conn().execute(
            "UPDATE games SET user_score = ?2, notes = ?3 WHERE id = ?1",
            params![entry_id.to_string(), score, notes],
        )?;
        Ok(())
    }
}
