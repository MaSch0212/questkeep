//! Metadata per source and the values the library shows (see [`crate::model::resolve`]), list
//! values for filtering, and the user's completion statuses.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::{
    CompletionSettings, CompletionStatus, GameMetadata, ListField, MetadataField, MetadataPriority,
    Resolved, SourceRecord, UserEdits, ValueCount, list_of, resolve, value_key,
};

/// `app_state` key set when the shown values must be computed again for every entry.
const RESOLVE_KEY: &str = "metadata.resolve";
const NEW_GAME_KEY: &str = "completion.newGame";
const FIRST_START_KEY: &str = "completion.firstStart";

/// The shown values beyond the plain columns, stored as JSON in `games.details`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Details {
    pub meta: GameMetadata,
    /// Field key -> source id.
    pub sources: BTreeMap<String, String>,
}

/// The year of a `YYYY[-MM[-DD]]` date.
fn year_of(date: Option<&str>) -> Option<i64> {
    date?.get(..4)?.parse().ok()
}

fn as_i64(v: Option<u64>) -> Option<i64> {
    v.and_then(|v| i64::try_from(v).ok())
}

impl Database {
    /// Stores what `source` returned for a game row (`None`: it answered without data). Store
    /// sources also mark the row's metadata as fetched. Does not change the shown values; call
    /// [`resolve_entry`](Self::resolve_entry) afterwards.
    pub fn set_source_metadata(
        &self,
        game_id: Uuid,
        source: &str,
        data: Option<&GameMetadata>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let json = data.map(serde_json::to_string).transpose()?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO metadata_sources (game_id, source, data, checked_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (game_id, source) DO UPDATE SET data = excluded.data, \
             checked_at = excluded.checked_at",
            params![game_id.to_string(), source, json, now],
        )?;
        conn.execute(
            "UPDATE games SET metadata_updated_at = ?3 WHERE id = ?1 AND source = ?2",
            params![game_id.to_string(), source, now],
        )?;
        Ok(())
    }

    /// When `source` was last asked about a game row, if ever, and whether it had data.
    pub fn source_checked(
        &self,
        game_id: Uuid,
        source: &str,
    ) -> Result<Option<(DateTime<Utc>, bool)>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT checked_at, data IS NOT NULL FROM metadata_sources \
                 WHERE game_id = ?1 AND source = ?2",
                params![game_id.to_string(), source],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// One source's data for a game row.
    pub fn source_metadata(&self, game_id: Uuid, source: &str) -> Result<Option<GameMetadata>> {
        let json: Option<Option<String>> = self
            .conn()
            .query_row(
                "SELECT data FROM metadata_sources WHERE game_id = ?1 AND source = ?2",
                params![game_id.to_string(), source],
                |r| r.get(0),
            )
            .optional()?;
        Ok(json.flatten().and_then(|j| serde_json::from_str(&j).ok()))
    }

    /// Every source's data for a library entry: its own row and the copies combined into it.
    pub fn source_records(&self, entry_id: Uuid) -> Result<Vec<SourceRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT s.source, s.data FROM metadata_sources s JOIN games g ON g.id = s.game_id \
             WHERE s.data IS NOT NULL AND s.source <> 'user' AND (g.id = ?1 OR g.merged_into = ?1) \
             ORDER BY g.copy_position, g.merged_into IS NOT NULL, g.date_added",
        )?;
        let rows = stmt.query_map(params![entry_id.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (source, json) = row?;
            match serde_json::from_str(&json) {
                Ok(data) => records.push(SourceRecord { source, data }),
                Err(err) => tracing::warn!(game = %entry_id, source, %err, "bad metadata record"),
            }
        }
        Ok(records)
    }

    /// The user's edits of a library entry. Edits of copies combined into it (made before they
    /// were combined) count too, after the entry's own.
    pub fn user_edits(&self, entry_id: Uuid) -> Result<UserEdits> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT s.data FROM metadata_sources s JOIN games g ON g.id = s.game_id \
             WHERE s.data IS NOT NULL AND s.source = 'user' AND (g.id = ?1 OR g.merged_into = ?1) \
             ORDER BY g.merged_into IS NOT NULL, g.copy_position, g.date_added",
        )?;
        let rows = stmt
            .query_map(params![entry_id.to_string()], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut merged = UserEdits::default();
        for json in rows {
            let edits: UserEdits = match serde_json::from_str(&json) {
                Ok(edits) => edits,
                Err(err) => {
                    tracing::warn!(game = %entry_id, %err, "bad user metadata");
                    continue;
                }
            };
            merged.merge(edits);
        }
        Ok(merged)
    }

    /// Replaces the user's edits of a library entry (edits of its copies are folded in).
    pub fn set_user_edits(&self, entry_id: Uuid, edits: &UserEdits) -> Result<()> {
        let json = serde_json::to_string(edits)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM metadata_sources WHERE source = 'user' AND game_id IN \
             (SELECT id FROM games WHERE id = ?1 OR merged_into = ?1)",
            [entry_id.to_string()],
        )?;
        if !edits.is_empty() {
            tx.execute(
                "INSERT INTO metadata_sources (game_id, source, data, checked_at) \
                 VALUES (?1, 'user', ?2, ?3)",
                params![entry_id.to_string(), json, Utc::now()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The library entry a game row belongs to (itself, or the entry it is combined into).
    pub fn entry_of(&self, game_id: Uuid) -> Result<Uuid> {
        let entry: Option<String> = self
            .conn()
            .query_row(
                "SELECT COALESCE(merged_into, id) FROM games WHERE id = ?1",
                params![game_id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        entry
            .and_then(|e| Uuid::parse_str(&e).ok())
            .ok_or(CoreError::GameNotFound(game_id))
    }

    /// Computes and stores the shown values of a library entry from its sources.
    pub fn resolve_entry(
        &self,
        entry_id: Uuid,
        priority: &MetadataPriority,
        sources: &[String],
    ) -> Result<()> {
        let records = self.source_records(entry_id)?;
        let user = self.user_edits(entry_id)?;
        let resolved = resolve(&records, Some(&user), priority, sources);
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        store_resolved(&tx, entry_id, &resolved)?;
        tx.commit()?;
        Ok(())
    }

    /// Computes the shown values of every entry again (after the priority changed or the
    /// migration asked for it). Returns how many entries were updated.
    pub fn resolve_all(&self, priority: &MetadataPriority, sources: &[String]) -> Result<usize> {
        let ids = self.entry_ids()?;
        let mut records_by_entry = Vec::with_capacity(ids.len());
        for id in &ids {
            records_by_entry.push((*id, self.source_records(*id)?, self.user_edits(*id)?));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (id, records, user) in &records_by_entry {
            store_resolved(&tx, *id, &resolve(records, Some(user), priority, sources))?;
        }
        tx.execute("DELETE FROM app_state WHERE key = ?1", [RESOLVE_KEY])?;
        prune_values(&tx)?;
        tx.commit()?;
        Ok(ids.len())
    }

    /// Whether [`resolve_all`](Self::resolve_all) is due (a migration asked for it).
    pub fn resolve_pending(&self) -> Result<bool> {
        Ok(self
            .conn()
            .query_row(
                "SELECT 1 FROM app_state WHERE key = ?1",
                [RESOLVE_KEY],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Ids of all library entries.
    /// Recomputes every automatic sorting name with `prefixes` as the ignored leading words
    /// (after they changed); names the user typed stay. Returns the entries whose sorting name
    /// changed.
    pub fn recompute_sorting_names(&self, prefixes: &[String]) -> Result<Vec<Uuid>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let rows: Vec<(String, String, String, Option<String>)> = tx
            .prepare(
                "SELECT COALESCE(merged_into, id), id, name, sorting_name FROM games                  WHERE sorting_name_user IS NULL",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut changed = Vec::new();
        for (entry, id, name, current) in rows {
            let wanted = crate::model::sorting_name_with(&name, prefixes);
            if current.as_deref() == Some(wanted.as_str()) {
                continue;
            }
            tx.execute(
                "UPDATE games SET sorting_name = ?2 WHERE id = ?1",
                rusqlite::params![id, wanted],
            )?;
            if let Ok(entry) = Uuid::parse_str(&entry)
                && !changed.contains(&entry)
            {
                changed.push(entry);
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    pub fn entry_ids(&self) -> Result<Vec<Uuid>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached("SELECT id FROM games WHERE merged_into IS NULL")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|id| id.ok().and_then(|id| Uuid::parse_str(&id).ok()))
            .collect();
        Ok(ids)
    }

    /// The values of a list field with how many (visible and hidden) entries have each, sorted
    /// by name.
    pub fn list_values(&self, field: ListField) -> Result<Vec<ValueCount>> {
        let conn = self.conn();
        let sql = match field.metadata_field() {
            Some(_) => {
                "SELECT v.key, v.name, COUNT(*) FROM meta_values v \
                 JOIN game_meta_values gv ON gv.value_id = v.id \
                 JOIN games g ON g.id = gv.game_id AND g.merged_into IS NULL \
                 WHERE v.field = ?1 GROUP BY v.id ORDER BY v.name COLLATE NOCASE"
            }
            None => match field {
                ListField::Source => {
                    "SELECT source, source, COUNT(DISTINCT COALESCE(merged_into, id)) FROM games \
                     WHERE ?1 = ?1 GROUP BY source ORDER BY source"
                }
                ListField::Category => {
                    "SELECT c.id, c.name, COUNT(l.game_id) FROM categories c \
                     LEFT JOIN game_category_links l ON l.category_id = c.id \
                     WHERE ?1 = ?1 GROUP BY c.id ORDER BY c.position"
                }
                _ => {
                    "SELECT s.id, s.name, COUNT(g.id) FROM completion_statuses s \
                     LEFT JOIN games g ON g.completion_status = s.id AND g.merged_into IS NULL \
                     WHERE ?1 = ?1 GROUP BY s.id ORDER BY s.position"
                }
            },
        };
        let key = field
            .metadata_field()
            .map(MetadataField::key)
            .unwrap_or_default();
        let mut stmt = conn.prepare_cached(sql)?;
        let values = stmt
            .query_map([key], |r| {
                Ok(ValueCount {
                    key: r.get(0)?,
                    name: r.get(1)?,
                    count: u32::try_from(r.get::<_, i64>(2)?).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(values)
    }

    /// The completion status list and its rules.
    pub fn completion_settings(&self) -> Result<CompletionSettings> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT s.id, s.name, (SELECT COUNT(*) FROM games g \
             WHERE g.completion_status = s.id AND g.merged_into IS NULL) \
             FROM completion_statuses s ORDER BY s.position",
        )?;
        let statuses = stmt
            .query_map([], |r| {
                Ok(CompletionStatus {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    game_count: u32::try_from(r.get::<_, i64>(2)?).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let state = |key: &str| -> Result<Option<String>> {
            Ok(conn
                .query_row("SELECT value FROM app_state WHERE key = ?1", [key], |r| {
                    r.get(0)
                })
                .optional()?)
        };
        Ok(CompletionSettings {
            statuses,
            new_game: state(NEW_GAME_KEY)?,
            first_start: state(FIRST_START_KEY)?,
        })
    }

    /// Replaces the status list and rules. Statuses missing from the new list are deleted; their
    /// games get `replacements[id]` (or none). New statuses get an id from their name.
    pub fn set_completion_settings(
        &self,
        settings: &CompletionSettings,
        replacements: &BTreeMap<String, Option<String>>,
    ) -> Result<CompletionSettings> {
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let mut ids = Vec::new();
            for (position, status) in settings.statuses.iter().enumerate() {
                let name = status.name.trim();
                if name.is_empty() {
                    return Err(CoreError::Invalid(
                        "A completion status needs a name.".into(),
                    ));
                }
                let id = if status.id.is_empty() {
                    unique_status_id(&tx, name)?
                } else {
                    status.id.clone()
                };
                tx.execute(
                    "INSERT INTO completion_statuses (id, name, position) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (id) DO UPDATE SET name = excluded.name, \
                     position = excluded.position",
                    params![id, name, position as i64],
                )?;
                ids.push(id);
            }
            let removed: Vec<String> = {
                let mut stmt = tx.prepare("SELECT id FROM completion_statuses")?;
                stmt.query_map([], |r| r.get::<_, String>(0))?
                    .filter_map(|id| id.ok())
                    .filter(|id| !ids.contains(id))
                    .collect()
            };
            for id in &removed {
                let replacement = replacements
                    .get(id)
                    .cloned()
                    .flatten()
                    .filter(|r| ids.contains(r));
                tx.execute(
                    "UPDATE games SET completion_status = ?2 WHERE completion_status = ?1",
                    params![id, replacement],
                )?;
                tx.execute("DELETE FROM completion_statuses WHERE id = ?1", [id])?;
            }
            let valid = |v: &Option<String>| v.clone().filter(|v| ids.contains(v));
            for (key, value) in [
                (NEW_GAME_KEY, valid(&settings.new_game)),
                (FIRST_START_KEY, valid(&settings.first_start)),
            ] {
                match value {
                    Some(v) => tx.execute(
                        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
                        params![key, v],
                    )?,
                    None => tx.execute("DELETE FROM app_state WHERE key = ?1", [key])?,
                };
            }
            tx.commit()?;
        }
        self.completion_settings()
    }

    /// Sets the completion status of library entries (`None` clears it). Unknown statuses are
    /// rejected.
    pub fn set_completion_status(&self, entry_ids: &[Uuid], status: Option<&str>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if let Some(status) = status {
            let known = tx
                .query_row(
                    "SELECT 1 FROM completion_statuses WHERE id = ?1",
                    [status],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !known {
                return Err(CoreError::Invalid(format!(
                    "Unknown completion status \"{status}\"."
                )));
            }
        }
        for id in entry_ids {
            tx.execute(
                "UPDATE games SET completion_status = ?2 \
                 WHERE id = (SELECT COALESCE(merged_into, id) FROM games WHERE id = ?1)",
                params![id.to_string(), status],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Applies the "first start" rule to an entry that was just started: while it still has the
    /// new-game status, it gets the first-start status. Returns whether it changed.
    pub fn apply_first_start(&self, game_id: Uuid) -> Result<bool> {
        let changed = self.conn().execute(
            "UPDATE games SET completion_status = \
                (SELECT value FROM app_state WHERE key = 'completion.firstStart') \
             WHERE id = (SELECT COALESCE(merged_into, id) FROM games WHERE id = ?1) \
               AND COALESCE(kind_user, kind) = 'game' \
               AND EXISTS (SELECT 1 FROM app_state WHERE key = 'completion.firstStart') \
               AND completion_status IS (SELECT value FROM app_state WHERE key = 'completion.newGame') \
               AND completion_status IS NOT \
                (SELECT value FROM app_state WHERE key = 'completion.firstStart')",
            params![game_id.to_string()],
        )?;
        Ok(changed > 0)
    }
}

/// An id for a new status made from its name, unique among the existing ones.
fn unique_status_id(tx: &Transaction<'_>, name: &str) -> Result<String> {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let base = if base.is_empty() {
        "status".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while tx
        .query_row(
            "SELECT 1 FROM completion_statuses WHERE id = ?1",
            [&id],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        id = format!("{base}-{n}");
        n += 1;
    }
    Ok(id)
}

/// Writes the shown values of an entry: the plain columns (kept for the existing queries), the
/// details JSON and the list value links.
fn store_resolved(tx: &Transaction<'_>, entry_id: Uuid, resolved: &Resolved) -> Result<()> {
    let meta = &resolved.data;
    let id = entry_id.to_string();
    let details = serde_json::to_string(&Details {
        meta: meta.clone(),
        sources: resolved.sources.clone(),
    })?;
    tx.execute(
        "UPDATE games SET description = ?2, release_date = ?3, developers = ?4, publishers = ?5, \
         genres = ?6, details = ?7, release_year = ?8, critic_score = ?9, community_score = ?10, \
         install_size = ?11, ttb_main = ?12 WHERE id = ?1",
        params![
            id,
            meta.description,
            meta.release_date,
            serde_json::to_string(&meta.developers)?,
            serde_json::to_string(&meta.publishers)?,
            serde_json::to_string(&meta.genres)?,
            details,
            year_of(meta.release_date.as_deref()),
            meta.critic_score.as_ref().map(|s| i64::from(s.value)),
            meta.community_score.as_ref().map(|s| i64::from(s.value)),
            as_i64(meta.install_size),
            as_i64(meta.time_to_beat.as_ref().and_then(|t| t.main)),
        ],
    )?;
    tx.execute("DELETE FROM game_meta_values WHERE game_id = ?1", [&id])?;
    let mut find = tx.prepare_cached("SELECT id FROM meta_values WHERE field = ?1 AND key = ?2")?;
    let mut insert_value =
        tx.prepare_cached("INSERT INTO meta_values (field, key, name) VALUES (?1, ?2, ?3)")?;
    let mut link = tx.prepare_cached(
        "INSERT OR IGNORE INTO game_meta_values (game_id, value_id, position) VALUES (?1, ?2, ?3)",
    )?;
    for field in MetadataField::LISTS {
        for (position, name) in list_of(meta, field).iter().enumerate() {
            let key = value_key(name);
            if key.is_empty() {
                continue;
            }
            let value_id: i64 = match find
                .query_row(params![field.key(), key], |r| r.get(0))
                .optional()?
            {
                Some(v) => v,
                None => {
                    insert_value.execute(params![field.key(), key, name])?;
                    tx.last_insert_rowid()
                }
            };
            link.execute(params![id, value_id, position as i64])?;
        }
    }
    Ok(())
}

/// Deletes list values no entry uses anymore.
fn prune_values(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        "DELETE FROM meta_values WHERE id NOT IN (SELECT value_id FROM game_meta_values)",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewGameRecord;
    use crate::model::{GameLink, GameQuery, Score};

    #[test]
    fn recomputes_automatic_sorting_names_only() {
        use crate::model::ImportedGame;
        let db = Database::open_in_memory().unwrap();
        let now = chrono::Utc::now();
        db.upsert_imported(
            "steam",
            &[
                ImportedGame::installed("1", "Die Siedler"),
                ImportedGame::installed("2", "Der Planer"),
            ],
            now,
        )
        .unwrap();
        let games = db.list_games(&crate::model::GameQuery::default()).unwrap();
        let planer = games.iter().find(|g| g.name == "Der Planer").unwrap().id;
        db.set_user_sorting_name(planer, Some("Mein Planer"))
            .unwrap();
        let changed = db
            .recompute_sorting_names(&["Die".to_string(), "Der".to_string()])
            .unwrap();
        assert_eq!(changed.len(), 1);
        let sorted = db.list_games(&crate::model::GameQuery::default()).unwrap();
        let by_name = |n: &str| {
            sorted
                .iter()
                .find(|g| g.name == n)
                .unwrap()
                .sorting_name
                .clone()
        };
        assert_eq!(by_name("Die Siedler"), "Siedler");
        assert_eq!(by_name("Der Planer"), "Mein Planer");
    }

    fn db_with(name: &str, source: &str) -> (Database, Uuid) {
        let db = Database::open_in_memory().unwrap();
        let game = db
            .insert_game(
                &NewGameRecord {
                    name: name.into(),
                    source: source.into(),
                    source_game_id: Some("1".into()),
                    ..NewGameRecord::default()
                },
                Utc::now(),
            )
            .unwrap();
        (db, game.id)
    }

    fn sources() -> Vec<String> {
        vec!["igdb".into(), "steam".into()]
    }

    #[test]
    fn stores_sources_and_shows_the_winning_values() {
        let (db, id) = db_with("Hades", "steam");
        let steam = GameMetadata {
            description: Some("From Steam".into()),
            tags: vec!["Roguelike".into(), "Co-op".into()],
            features: vec!["Steam Cloud".into()],
            community_score: Some(Score {
                value: 96,
                source: "Steam reviews".into(),
            }),
            links: vec![GameLink::new(
                "Steam",
                "https://store.steampowered.com/app/1",
            )],
            ..GameMetadata::default()
        };
        let igdb = GameMetadata {
            description: Some("From IGDB".into()),
            release_date: Some("2020-09-17".into()),
            series: vec!["Hades".into()],
            ..GameMetadata::default()
        };
        db.set_source_metadata(id, "steam", Some(&steam), Utc::now())
            .unwrap();
        db.set_source_metadata(id, "igdb", Some(&igdb), Utc::now())
            .unwrap();
        db.resolve_entry(id, &MetadataPriority::default(), &sources())
            .unwrap();
        let game = db.get_entry(id).unwrap();
        assert_eq!(game.description.as_deref(), Some("From IGDB"));
        assert_eq!(game.tags, ["Roguelike", "Co-op"]);
        assert_eq!(game.features, ["Cloud saves"]);
        assert_eq!(game.series, ["Hades"]);
        assert_eq!(game.community_score.as_ref().map(|s| s.value), Some(96));
        assert_eq!(game.field_sources["description"], "igdb");
        assert_eq!(game.links.len(), 1);
        assert!(
            game.metadata_updated_at.is_some(),
            "the store source marks it fetched"
        );

        let tags = db.list_values(ListField::Tag).unwrap();
        assert_eq!(
            tags.iter()
                .map(|v| (v.key.as_str(), v.count))
                .collect::<Vec<_>>(),
            [("coop", 1), ("roguelike", 1)]
        );
        // A different order changes the shown value without a new fetch.
        let steam_first = MetadataPriority {
            order: vec!["steam".into()],
            ..MetadataPriority::default()
        };
        db.resolve_all(&steam_first, &sources()).unwrap();
        let game = db.get_entry(id).unwrap();
        assert_eq!(game.description.as_deref(), Some("From Steam"));
    }

    #[test]
    fn migration_keeps_previous_data_until_resolved() {
        let (db, id) = db_with("Old", "steam");
        // As written before per-source storage existed.
        db.conn()
            .execute(
                "INSERT INTO metadata_sources (game_id, source, data, checked_at) \
                 VALUES (?1, 'previous', '{\"description\":\"old\",\"genres\":[\"RPG\"]}', '2026-01-01T00:00:00Z')",
                [id.to_string()],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('metadata.resolve', '1')",
                [],
            )
            .unwrap();
        assert!(db.resolve_pending().unwrap());
        db.resolve_all(&MetadataPriority::default(), &sources())
            .unwrap();
        assert!(!db.resolve_pending().unwrap());
        let game = db.get_entry(id).unwrap();
        assert_eq!(game.genres, ["RPG"]);
        assert_eq!(game.field_sources["genres"], "previous");
        let query = GameQuery::default();
        assert_eq!(db.count_matching(&query).unwrap(), 1);
    }

    #[test]
    fn filters_with_groups_lists_and_ranges_and_sorts_by_metadata() {
        use crate::model::{
            FilterGroup, FilterNode, ListFilter, ListMode, MatchMode, NO_VALUE, RangeField,
            RangeFilter, SortField,
        };
        let db = Database::open_in_memory().unwrap();
        let mut ids = Vec::new();
        for (name, tags, score, year) in [
            ("A", vec!["Co-op", "Roguelike"], Some(90), "2020"),
            ("B", vec!["Co-op"], Some(70), "2015"),
            ("C", vec!["Puzzle"], None, "2010"),
            ("D", vec![], Some(80), "2022"),
        ] {
            let game = db
                .insert_game(
                    &NewGameRecord {
                        name: name.into(),
                        source: "manual".into(),
                        ..NewGameRecord::default()
                    },
                    Utc::now(),
                )
                .unwrap();
            let meta = GameMetadata {
                tags: tags.into_iter().map(String::from).collect(),
                critic_score: score.map(|value| Score {
                    value,
                    source: "Metacritic".into(),
                }),
                release_date: Some(year.into()),
                ..GameMetadata::default()
            };
            db.set_source_metadata(game.id, "igdb", Some(&meta), Utc::now())
                .unwrap();
            db.resolve_entry(game.id, &MetadataPriority::default(), &sources())
                .unwrap();
            ids.push(game.id);
        }
        let names = |query: &GameQuery| -> Vec<String> {
            db.list_games(query)
                .unwrap()
                .into_iter()
                .map(|g| g.name)
                .collect()
        };
        let tags = |mode, values: &[&str]| {
            FilterNode::List(ListFilter {
                field: ListField::Tag,
                mode,
                values: values.iter().map(|v| v.to_string()).collect(),
            })
        };
        let with = |mode, items| GameQuery {
            filter: FilterGroup { mode, items },
            ..GameQuery::default()
        };
        assert_eq!(
            names(&with(
                MatchMode::All,
                vec![tags(ListMode::Any, &["coop", "puzzle"])]
            )),
            ["A", "B", "C"]
        );
        assert_eq!(
            names(&with(
                MatchMode::All,
                vec![tags(ListMode::All, &["coop", "roguelike"])]
            )),
            ["A"]
        );
        assert_eq!(
            names(&with(MatchMode::All, vec![tags(ListMode::None, &["coop"])])),
            ["C", "D"]
        );
        assert_eq!(
            names(&with(
                MatchMode::All,
                vec![tags(ListMode::Any, &[NO_VALUE])]
            )),
            ["D"]
        );
        let score = |min, max| {
            FilterNode::Range(RangeFilter {
                field: RangeField::CriticScore,
                min,
                max,
            })
        };
        assert_eq!(
            names(&with(MatchMode::All, vec![score(Some(75.0), None)])),
            ["A", "D"]
        );
        // Puzzle or (co-op and score 80+): A (co-op, 90) and C (puzzle).
        let nested = with(
            MatchMode::Any,
            vec![
                tags(ListMode::Any, &["puzzle"]),
                FilterNode::Group(FilterGroup {
                    mode: MatchMode::All,
                    items: vec![tags(ListMode::Any, &["coop"]), score(Some(80.0), None)],
                }),
            ],
        );
        assert_eq!(names(&nested), ["A", "C"]);
        let year = with(
            MatchMode::All,
            vec![FilterNode::Range(RangeFilter {
                field: RangeField::ReleaseYear,
                min: Some(2014.0),
                max: Some(2020.0),
            })],
        );
        assert_eq!(names(&year), ["A", "B"]);
        // Sorting by score puts games without one last in both directions.
        let mut by_score = GameQuery {
            sort: SortField::CriticScore,
            ..GameQuery::default()
        };
        assert_eq!(names(&by_score), ["B", "D", "A", "C"]);
        by_score.descending = true;
        assert_eq!(names(&by_score), ["A", "D", "B", "C"]);
        db.set_completion_status(&[ids[2]], Some("beaten")).unwrap();
        let status = with(
            MatchMode::All,
            vec![FilterNode::List(ListFilter {
                field: ListField::CompletionStatus,
                mode: ListMode::Any,
                values: vec!["beaten".into()],
            })],
        );
        assert_eq!(names(&status), ["C"]);
    }

    #[test]
    fn completion_statuses_rules_and_removal() {
        let (db, id) = db_with("New", "manual");
        let settings = db.completion_settings().unwrap();
        assert_eq!(
            settings
                .statuses
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["Not Played", "Played", "Beaten"]
        );
        assert_eq!(
            db.get_entry(id).unwrap().completion_status.as_deref(),
            Some("not-played")
        );
        assert!(db.apply_first_start(id).unwrap());
        assert_eq!(
            db.get_entry(id).unwrap().completion_status.as_deref(),
            Some("played")
        );
        assert!(!db.apply_first_start(id).unwrap(), "only once");

        let mut edited = settings.clone();
        edited.statuses.retain(|s| s.id != "played");
        edited.statuses.push(CompletionStatus {
            id: String::new(),
            name: "On Hold".into(),
            game_count: 0,
        });
        edited.first_start = Some("played".into());
        let result = db
            .set_completion_settings(
                &edited,
                &BTreeMap::from([("played".to_string(), Some("beaten".to_string()))]),
            )
            .unwrap();
        assert_eq!(
            result
                .statuses
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["not-played", "beaten", "on-hold"]
        );
        assert_eq!(
            result.first_start, None,
            "a deleted status can't be the rule"
        );
        assert_eq!(
            db.get_entry(id).unwrap().completion_status.as_deref(),
            Some("beaten")
        );
        db.set_completion_status(&[id], Some("on-hold")).unwrap();
        assert!(db.set_completion_status(&[id], Some("nope")).is_err());
        db.set_completion_status(&[id], None).unwrap();
        assert_eq!(db.get_entry(id).unwrap().completion_status, None);
    }
}
