//! Game scripts (see [`crate::scripts`]): the rows, their order and the per-game opt-out. The
//! text of inline scripts is kept in files, not here.

use rusqlite::{OptionalExtension, ToSql, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::{GameQuery, Script, ScriptInput, ScriptKind, ScriptSlot, ScriptSpec};

const COLUMNS: &str = "id, name, kind, enabled, game_id, filter, position, spec";

fn row_to_script(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<Script>> {
    let id: String = row.get(0)?;
    let kind: String = row.get(2)?;
    let game_id: Option<String> = row.get(4)?;
    let filter: Option<String> = row.get(5)?;
    let spec: String = row.get(7)?;
    let (Ok(id), Some(kind)) = (Uuid::parse_str(&id), ScriptKind::parse(&kind)) else {
        return Ok(None);
    };
    Ok(Some(Script {
        id,
        name: row.get(1)?,
        kind,
        enabled: row.get(3)?,
        game_id: game_id.and_then(|g| Uuid::parse_str(&g).ok()),
        filter: filter.and_then(|f| serde_json::from_str::<GameQuery>(&f).ok()),
        spec: serde_json::from_str::<ScriptSpec>(&spec).unwrap_or_default(),
        position: row.get(6)?,
    }))
}

/// Where the games' own scripts run when nothing was chosen: last before the game, first after.
fn default_slot(kind: ScriptKind) -> i64 {
    match kind {
        ScriptKind::Post => -1,
        ScriptKind::Pre | ScriptKind::Action => i64::MAX,
    }
}

impl Database {
    fn scripts_where(&self, sql_where: &str, args: &[&dyn ToSql]) -> Result<Vec<Script>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM scripts {sql_where} ORDER BY kind, position, name"
        ))?;
        let rows = stmt
            .query_map(args, row_to_script)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().flatten().collect())
    }

    /// Global and filtered scripts, by kind and order.
    pub fn global_scripts(&self) -> Result<Vec<Script>> {
        self.scripts_where("WHERE game_id IS NULL", &[])
    }

    /// The own scripts of the entry `entry` belongs to.
    pub fn own_scripts(&self, entry: Uuid) -> Result<Vec<Script>> {
        let entry = self.entry_id(entry)?;
        self.scripts_where("WHERE game_id = ?1", &[&entry])
    }

    pub fn get_script(&self, id: Uuid) -> Result<Script> {
        self.scripts_where("WHERE id = ?1", &[&id.to_string()])?
            .pop()
            .ok_or_else(|| CoreError::Invalid("That script no longer exists.".into()))
    }

    /// Every script id (to clean up files of deleted scripts).
    pub fn script_ids(&self) -> Result<Vec<Uuid>> {
        let conn = self.conn();
        let ids: Vec<String> = conn
            .prepare("SELECT id FROM scripts")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids.iter().filter_map(|i| Uuid::parse_str(i).ok()).collect())
    }

    /// The id of the library entry a game (or copy) belongs to, as stored.
    fn entry_id(&self, id: Uuid) -> Result<String> {
        self.conn()
            .query_row(
                "SELECT COALESCE(merged_into, id) FROM games WHERE id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(CoreError::GameNotFound(id))
    }

    /// Creates (`input.id` `None`, placed last in its list) or replaces a script. A game's
    /// script is stored on the game's library entry. The inline text is not stored here.
    pub fn save_script(&self, input: &ScriptInput) -> Result<Script> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(CoreError::Invalid("Give the script a name.".into()));
        }
        let game_id = input.game_id.map(|g| self.entry_id(g)).transpose()?;
        let filter = match (&game_id, &input.filter) {
            (None, Some(filter)) => Some(serde_json::to_string(filter)?),
            _ => None,
        };
        let spec = ScriptSpec {
            content: None,
            ..input.spec.clone()
        };
        let spec = serde_json::to_string(&spec)?;
        let id = input.id.unwrap_or_else(Uuid::new_v4);
        let conn = self.conn();
        let existing: Option<(Option<String>, String)> = conn
            .query_row(
                "SELECT game_id, kind FROM scripts WHERE id = ?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        // Moving to another list (kind or owner) places it last there.
        let keeps_place = existing
            .as_ref()
            .is_some_and(|(game, kind)| *game == game_id && kind == input.kind.as_str());
        if keeps_place {
            conn.execute(
                "UPDATE scripts SET name = ?2, enabled = ?3, filter = ?4, spec = ?5 WHERE id = ?1",
                params![id.to_string(), name, input.enabled, filter, spec],
            )?;
        } else {
            let last: i64 = conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM scripts \
                 WHERE kind = ?1 AND game_id IS ?2",
                params![input.kind.as_str(), game_id],
                |r| r.get(0),
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO scripts (id, name, kind, enabled, game_id, filter, \
                 position, spec) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id.to_string(),
                    name,
                    input.kind.as_str(),
                    input.enabled,
                    game_id,
                    filter,
                    last + 1,
                    spec
                ],
            )?;
        }
        drop(conn);
        self.get_script(id)
    }

    pub fn set_script_enabled(&self, id: Uuid, enabled: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE scripts SET enabled = ?2 WHERE id = ?1",
            params![id.to_string(), enabled],
        )?;
        Ok(())
    }

    pub fn delete_script(&self, id: Uuid) -> Result<()> {
        self.conn()
            .execute("DELETE FROM scripts WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    /// The global order of `kind`: the rows in the order given, `GameScripts` where the games'
    /// own scripts run. Scripts not listed keep their place after the listed ones.
    pub fn set_global_order(&self, kind: ScriptKind, order: &[ScriptSlot]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut slot_seen = false;
        for (i, slot) in order.iter().enumerate() {
            let position = i64::try_from(i).unwrap_or(i64::MAX);
            match slot {
                ScriptSlot::Script { id } => {
                    tx.execute(
                        "UPDATE scripts SET position = ?2 WHERE id = ?1 AND game_id IS NULL",
                        params![id.to_string(), position],
                    )?;
                }
                ScriptSlot::GameScripts => {
                    slot_seen = true;
                    tx.execute(
                        "INSERT OR REPLACE INTO script_slots (kind, position) VALUES (?1, ?2)",
                        params![kind.as_str(), position],
                    )?;
                }
            }
        }
        if !slot_seen {
            tx.execute("DELETE FROM script_slots WHERE kind = ?1", [kind.as_str()])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The order of a game's own scripts of one kind.
    pub fn set_own_order(&self, ids: &[Uuid]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE scripts SET position = ?2 WHERE id = ?1",
                params![id.to_string(), i64::try_from(i).unwrap_or(i64::MAX)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Where the games' own scripts run in the global order of `kind` (a position among the
    /// global scripts' positions).
    pub fn game_scripts_slot(&self, kind: ScriptKind) -> Result<i64> {
        Ok(self
            .conn()
            .query_row(
                "SELECT position FROM script_slots WHERE kind = ?1",
                [kind.as_str()],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| default_slot(kind)))
    }

    /// The global order of `kind` with the games' own scripts' place.
    pub fn global_order(&self, kind: ScriptKind) -> Result<Vec<ScriptSlot>> {
        let slot = self.game_scripts_slot(kind)?;
        let mut rows: Vec<(i64, u8, ScriptSlot)> = self
            .global_scripts()?
            .into_iter()
            .filter(|s| s.kind == kind)
            .map(|s| (s.position, 1, ScriptSlot::Script { id: s.id }))
            .collect();
        // At an equal position the games' scripts come first (they were placed there).
        rows.push((slot, 0, ScriptSlot::GameScripts));
        rows.sort_by_key(|(position, tie, _)| (*position, *tie));
        Ok(rows.into_iter().map(|(_, _, slot)| slot).collect())
    }

    /// Whether global and filtered scripts run for the entry of `id`.
    pub fn runs_global_scripts(&self, id: Uuid) -> Result<bool> {
        let entry = self.entry_id(id)?;
        let opted_out: Option<String> = self
            .conn()
            .query_row(
                "SELECT game_id FROM script_opt_outs WHERE game_id = ?1",
                [&entry],
                |r| r.get(0),
            )
            .optional()?;
        Ok(opted_out.is_none())
    }

    pub fn set_runs_global_scripts(&self, id: Uuid, run: bool) -> Result<()> {
        let entry = self.entry_id(id)?;
        let conn = self.conn();
        if run {
            conn.execute("DELETE FROM script_opt_outs WHERE game_id = ?1", [&entry])?;
        } else {
            conn.execute(
                "INSERT OR IGNORE INTO script_opt_outs (game_id) VALUES (?1)",
                [&entry],
            )?;
        }
        Ok(())
    }

    /// How many entries have own scripts.
    /// How many games have own scripts of `kind`.
    pub fn games_with_scripts(&self, kind: ScriptKind) -> Result<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(DISTINCT game_id) FROM scripts WHERE game_id IS NOT NULL AND kind = ?1",
            [kind.as_str()],
            |r| r.get(0),
        )?;
        Ok(u32::try_from(n).unwrap_or_default())
    }

    /// Whether the library entry of `id` matches `query` (a filtered script's filter).
    pub fn entry_matches(&self, query: &GameQuery, id: Uuid) -> Result<bool> {
        let entry = self.entry_id(id)?;
        let (where_sql, mut args) = super::games::query_filter(query);
        args.push(Box::new(entry));
        let join = if where_sql.is_empty() { "WHERE" } else { "AND" };
        let sql = format!(
            "SELECT 1 FROM library_entries AS games {where_sql} {join} games.id = ?{}",
            args.len()
        );
        let params: Vec<&dyn ToSql> = args.iter().map(AsRef::as_ref).collect();
        let found: Option<i64> = self
            .conn()
            .query_row(&sql, params.as_slice(), |r| r.get(0))
            .optional()?;
        Ok(found.is_some())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::db::NewGameRecord;
    use crate::model::{ScriptSource, TriState};

    fn input(name: &str, kind: ScriptKind, game: Option<Uuid>) -> ScriptInput {
        ScriptInput {
            id: None,
            name: name.into(),
            kind,
            enabled: true,
            game_id: game,
            filter: None,
            spec: ScriptSpec {
                source: ScriptSource::Program,
                program: "eq.exe".into(),
                content: Some("not stored".into()),
                ..ScriptSpec::default()
            },
        }
    }

    fn game(db: &Database, name: &str, source: &str) -> Uuid {
        db.insert_game(
            &NewGameRecord {
                name: name.into(),
                source: source.into(),
                ..Default::default()
            },
            Utc::now(),
        )
        .unwrap()
        .id
    }

    #[test]
    fn saves_orders_and_deletes_scripts() {
        let db = Database::open_in_memory().unwrap();
        let a = db.save_script(&input("A", ScriptKind::Pre, None)).unwrap();
        let b = db.save_script(&input("B", ScriptKind::Pre, None)).unwrap();
        assert_eq!((a.position, b.position), (0, 1));
        assert_eq!(a.spec.content, None, "inline text lives in files");
        assert_eq!(
            db.global_order(ScriptKind::Pre).unwrap(),
            [
                ScriptSlot::Script { id: a.id },
                ScriptSlot::Script { id: b.id },
                ScriptSlot::GameScripts
            ]
        );
        assert_eq!(
            db.global_order(ScriptKind::Post).unwrap(),
            [ScriptSlot::GameScripts]
        );

        let order = [
            ScriptSlot::Script { id: b.id },
            ScriptSlot::GameScripts,
            ScriptSlot::Script { id: a.id },
        ];
        db.set_global_order(ScriptKind::Pre, &order).unwrap();
        assert_eq!(db.global_order(ScriptKind::Pre).unwrap(), order);

        let mut renamed = input("A2", ScriptKind::Pre, None);
        renamed.id = Some(a.id);
        // Saving keeps its place.
        assert_eq!(db.save_script(&renamed).unwrap().position, 2);
        db.set_script_enabled(a.id, false).unwrap();
        assert!(!db.get_script(a.id).unwrap().enabled);
        db.delete_script(a.id).unwrap();
        assert_eq!(db.global_scripts().unwrap().len(), 1);
        assert!(db.save_script(&input(" ", ScriptKind::Pre, None)).is_err());
    }

    #[test]
    fn own_scripts_follow_the_entry_and_filters_match_entries() {
        let db = Database::open_in_memory().unwrap();
        let steam = game(&db, "Portal", "steam");
        let gog = game(&db, "Portal", "gog");
        let own = db
            .save_script(&input("Mods", ScriptKind::Action, Some(gog)))
            .unwrap();
        assert_eq!(db.own_scripts(gog).unwrap(), std::slice::from_ref(&own));
        assert_eq!(db.games_with_scripts(ScriptKind::Action).unwrap(), 1);
        assert_eq!(db.games_with_scripts(ScriptKind::Pre).unwrap(), 0);

        db.combine_games(steam, &[gog]).unwrap();
        // Combining moves the copy's scripts onto the entry.
        assert_eq!(db.own_scripts(steam).unwrap().len(), 1);
        assert_eq!(
            db.own_scripts(gog).unwrap().len(),
            1,
            "a copy finds its entry's"
        );

        assert!(db.runs_global_scripts(gog).unwrap());
        db.set_runs_global_scripts(gog, false).unwrap();
        assert!(!db.runs_global_scripts(steam).unwrap());

        let favorites = GameQuery {
            favorite: TriState::Yes,
            ..GameQuery::default()
        };
        assert!(!db.entry_matches(&favorites, gog).unwrap());
        assert!(db.entry_matches(&GameQuery::default(), gog).unwrap());
    }
}
