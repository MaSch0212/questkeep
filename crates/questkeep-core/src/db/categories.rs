//! Categories (user tags), their order, their n:m links to games and the store categories
//! mapped to them.
//!
//! A game's categories (`game_category_links`) are its store's tags mapped through
//! `category_sources`, plus the user's own links, minus the imported ones the user took off.

use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::{Category, CategorySource};

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::Invalid(
            "The category name must not be empty.".into(),
        ));
    }
    Ok(name.to_string())
}

fn not_found() -> CoreError {
    CoreError::Invalid("That category no longer exists.".into())
}

/// The id of the category with this name (case-insensitive), if any.
fn id_by_name(tx: &Transaction<'_>, name: &str) -> Result<Option<String>> {
    Ok(tx
        .query_row(
            "SELECT id FROM categories WHERE name = ?1 COLLATE NOCASE",
            [name],
            |r| r.get(0),
        )
        .optional()?)
}

fn exists(tx: &Transaction<'_>, id: &str) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM categories WHERE id = ?1)",
        [id],
        |r| r.get(0),
    )?)
}

/// Inserts a category at `position`, moving the ones from there on down.
fn insert_category(tx: &Transaction<'_>, name: &str, position: i64) -> Result<String> {
    tx.execute(
        "UPDATE categories SET position = position + 1 WHERE position >= ?1",
        [position],
    )?;
    let id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO categories (id, name, position) VALUES (?1, ?2, ?3)",
        params![id, name, position],
    )?;
    Ok(id)
}

/// The category with this name, created at the end of the list when missing.
fn ensure_category(tx: &Transaction<'_>, name: &str) -> Result<String> {
    if let Some(id) = id_by_name(tx, name)? {
        return Ok(id);
    }
    let end: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM categories",
        [],
        |r| r.get(0),
    )?;
    insert_category(tx, name, end)
}

/// Games that currently have the category.
fn games_in(tx: &Transaction<'_>, category: &str) -> Result<HashSet<String>> {
    Ok(tx
        .prepare("SELECT game_id FROM game_category_links WHERE category_id = ?1")?
        .query_map([category], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Games with this store tag.
fn games_tagged(tx: &Transaction<'_>, source: &str, name: &str) -> Result<HashSet<String>> {
    Ok(tx
        .prepare("SELECT game_id FROM game_tags WHERE source = ?1 AND name = ?2")?
        .query_map([source, name], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn has_category(tx: &Transaction<'_>, game: &str, category: &str) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM game_category_links \
         WHERE game_id = ?1 AND category_id = ?2)",
        [game, category],
        |r| r.get(0),
    )?)
}

fn uuids(ids: HashSet<String>) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = ids
        .iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect();
    ids.sort();
    ids
}

impl Database {
    /// Every category in the user's order, with its game count and store mappings.
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let conn = self.conn();
        let mut sources: HashMap<String, Vec<CategorySource>> = HashMap::new();
        let mut stmt = conn.prepare_cached(
            "SELECT category_id, source, name FROM category_sources ORDER BY source, name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                CategorySource {
                    source: r.get(1)?,
                    name: r.get(2)?,
                },
            ))
        })?;
        for row in rows {
            let (id, source) = row?;
            sources.entry(id).or_default().push(source);
        }
        let mut stmt = conn.prepare_cached(
            "SELECT c.id, c.name, \
             (SELECT COUNT(*) FROM game_category_links l WHERE l.category_id = c.id) \
             FROM categories c ORDER BY c.position, c.name COLLATE NOCASE, c.id",
        )?;
        let categories = stmt
            .query_map([], |r| {
                let id: String = r.get(0)?;
                Ok(Category {
                    sources: sources.remove(&id).unwrap_or_default(),
                    id,
                    name: r.get(1)?,
                    game_count: u32::try_from(r.get::<_, i64>(2)?).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(categories)
    }

    pub(crate) fn category(&self, id: &str) -> Result<Category> {
        self.list_categories()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(not_found)
    }

    /// Creates a category at the end of the list, or returns the existing one with the same
    /// name (ignoring case).
    pub fn create_category(&self, name: &str) -> Result<Category> {
        let name = clean_name(name)?;
        let id = {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let id = ensure_category(&tx, &name)?;
            tx.commit()?;
            id
        };
        self.category(&id)
    }

    /// Renames a category. Store categories mapped to it stay mapped.
    pub fn rename_category(&self, id: &str, name: &str) -> Result<Category> {
        let name = clean_name(name)?;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            if let Some(other) = id_by_name(&tx, &name)?
                && other != id
            {
                return Err(CoreError::Invalid(format!(
                    "A category named “{name}” already exists. Merge the two instead."
                )));
            }
            let changed = tx.execute(
                "UPDATE categories SET name = ?2 WHERE id = ?1",
                params![id, name],
            )?;
            if changed == 0 {
                return Err(not_found());
            }
            tx.commit()?;
        }
        self.category(id)
    }

    /// Puts the categories in this order; ones not listed keep their order after them.
    pub fn reorder_categories(&self, ids: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let current: Vec<String> = tx
            .prepare("SELECT id FROM categories ORDER BY position, name COLLATE NOCASE, id")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let known: HashSet<&str> = current.iter().map(String::as_str).collect();
        let mut seen = HashSet::new();
        let order = ids
            .iter()
            .map(String::as_str)
            .filter(|id| known.contains(id))
            .chain(current.iter().map(String::as_str))
            .filter(|id| seen.insert(*id));
        for (position, id) in order.enumerate() {
            tx.execute(
                "UPDATE categories SET position = ?2 WHERE id = ?1",
                params![id, i64::try_from(position).unwrap_or(i64::MAX)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Merges `from` into `into`: its games and store categories move over, then `from` is
    /// deleted. Returns the games whose categories changed.
    pub fn merge_categories(&self, from: &str, into: &str) -> Result<Vec<Uuid>> {
        if from == into {
            return Err(CoreError::Invalid(
                "A category can't be merged into itself.".into(),
            ));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if !exists(&tx, from)? || !exists(&tx, into)? {
            return Err(not_found());
        }
        let changed = games_in(&tx, from)?;
        // A game in either keeps the merged category: drop exclusions that would hide it.
        tx.execute(
            "DELETE FROM game_categories WHERE excluded = 1 AND \
             ((category_id = ?1 AND game_id IN \
               (SELECT game_id FROM game_category_links WHERE category_id = ?2)) OR \
              (category_id = ?2 AND game_id IN \
               (SELECT game_id FROM game_category_links WHERE category_id = ?1)))",
            [from, into],
        )?;
        tx.execute(
            "UPDATE OR IGNORE game_categories SET category_id = ?2 WHERE category_id = ?1",
            [from, into],
        )?;
        tx.execute(
            "UPDATE category_sources SET category_id = ?2 WHERE category_id = ?1",
            [from, into],
        )?;
        tx.execute("DELETE FROM categories WHERE id = ?1", [from])?;
        tx.commit()?;
        Ok(uuids(changed))
    }

    /// Moves one store category off `id` to a category of its own name (an existing one, or a
    /// new one right after `id`): undoes a merge. Returns that category's id and the games whose
    /// categories changed.
    pub fn separate_category_source(
        &self,
        id: &str,
        source: &str,
        name: &str,
    ) -> Result<(String, Vec<Uuid>)> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let name: String = tx
            .query_row(
                "SELECT name FROM category_sources \
                 WHERE source = ?1 AND name = ?2 AND category_id = ?3",
                [source, name, id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                CoreError::Invalid("That store category is no longer part of it.".into())
            })?;
        let target = match id_by_name(&tx, &name)? {
            Some(target) if target == id => {
                return Err(CoreError::Invalid(format!(
                    "“{name}” is already this category's own name; rename it first."
                )));
            }
            Some(target) => target,
            None => {
                let position: i64 =
                    tx.query_row("SELECT position FROM categories WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })?;
                insert_category(&tx, &name, position + 1)?
            }
        };
        let changed = games_tagged(&tx, source, &name)?;
        tx.execute(
            "UPDATE category_sources SET category_id = ?3 WHERE source = ?1 AND name = ?2",
            [source, name.as_str(), target.as_str()],
        )?;
        tx.commit()?;
        Ok((target, uuids(changed)))
    }

    /// Deletes a category, its links and its store mappings; the games stay. A store that
    /// still has the category brings it back on its next sync.
    pub fn delete_category(&self, id: &str) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM categories WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(not_found());
        }
        Ok(())
    }

    /// Gives a game exactly these categories. Taking off an imported category is remembered,
    /// so the store's next sync doesn't bring it back.
    pub fn set_game_categories(&self, game_id: Uuid, category_ids: &[String]) -> Result<()> {
        let game = game_id.to_string();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let known: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM games WHERE id = ?1)",
            [&game],
            |r| r.get(0),
        )?;
        if !known {
            return Err(CoreError::GameNotFound(game_id));
        }
        let wanted: HashSet<&str> = category_ids.iter().map(String::as_str).collect();
        for id in &wanted {
            if !exists(&tx, id)? {
                return Err(not_found());
            }
        }
        let current: HashSet<String> = tx
            .prepare("SELECT category_id FROM game_category_links WHERE game_id = ?1")?
            .query_map([&game], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in current.iter().filter(|id| !wanted.contains(id.as_str())) {
            tx.execute(
                "DELETE FROM game_categories WHERE game_id = ?1 AND category_id = ?2",
                [&game, id],
            )?;
            if has_category(&tx, &game, id)? {
                tx.execute(
                    "INSERT INTO game_categories (game_id, category_id, excluded) \
                     VALUES (?1, ?2, 1)",
                    [&game, id],
                )?;
            }
        }
        for id in wanted.iter().filter(|id| !current.contains(**id)) {
            tx.execute(
                "DELETE FROM game_categories WHERE game_id = ?1 AND category_id = ?2",
                [game.as_str(), id],
            )?;
            if !has_category(&tx, &game, id)? {
                tx.execute(
                    "INSERT INTO game_categories (game_id, category_id) VALUES (?1, ?2)",
                    [game.as_str(), id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Replaces a store's tags with `by_game` (store game id -> its category names there).
    /// Tags without a mapping are mapped to the category of the same name, created at the end
    /// of the list when missing; mappings of tags no game has anymore are dropped, and so is a
    /// category left with no mapping and no games of the user's. Returns the games whose
    /// categories changed.
    pub fn sync_imported_categories(
        &self,
        source: &str,
        by_game: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<Uuid>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;

        let games: HashMap<String, String> = tx
            .prepare(
                "SELECT source_game_id, id FROM games \
                 WHERE source = ?1 AND source_game_id IS NOT NULL",
            )?
            .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        // (game, lowercase name) -> name as the store spells it
        let mut wanted: HashMap<(String, String), String> = HashMap::new();
        for (store_id, names) in by_game {
            let Some(game) = games.get(store_id) else {
                continue; // not in the library (e.g. an owned game the scan skipped)
            };
            for name in names {
                let name = name.trim();
                if !name.is_empty() {
                    wanted
                        .entry((game.clone(), name.to_lowercase()))
                        .or_insert_with(|| name.to_string());
                }
            }
        }
        let current: HashMap<(String, String), String> = tx
            .prepare("SELECT game_id, name FROM game_tags WHERE source = ?1")?
            .query_map([source], |r| {
                let game: String = r.get(0)?;
                let name: String = r.get(1)?;
                Ok(((game, name.to_lowercase()), name))
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut changed: HashSet<String> = HashSet::new();
        for ((game, key), name) in &current {
            if !wanted.contains_key(&(game.clone(), key.clone())) {
                tx.execute(
                    "DELETE FROM game_tags WHERE game_id = ?1 AND name = ?2",
                    [game, name],
                )?;
                changed.insert(game.clone());
            }
        }
        for ((game, key), name) in &wanted {
            if !current.contains_key(&(game.clone(), key.clone())) {
                tx.execute(
                    "INSERT INTO game_tags (game_id, source, name) VALUES (?1, ?2, ?3)",
                    [game, source, name],
                )?;
                changed.insert(game.clone());
            }
        }

        let mut unmapped: Vec<String> = tx
            .prepare(
                "SELECT DISTINCT name FROM game_tags t WHERE source = ?1 AND NOT EXISTS \
                 (SELECT 1 FROM category_sources s WHERE s.source = t.source AND s.name = t.name)",
            )?
            .query_map([source], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        unmapped.sort_by_key(|name| name.to_lowercase());
        for name in &unmapped {
            let category = ensure_category(&tx, name)?;
            tx.execute(
                "INSERT INTO category_sources (source, name, category_id) VALUES (?1, ?2, ?3)",
                [source, name, &category],
            )?;
            changed.extend(games_tagged(&tx, source, name)?);
        }

        let stale: Vec<(String, String)> = tx
            .prepare(
                "SELECT name, category_id FROM category_sources s WHERE source = ?1 AND NOT EXISTS \
                 (SELECT 1 FROM game_tags t WHERE t.source = s.source AND t.name = s.name)",
            )?
            .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (name, category) in &stale {
            tx.execute(
                "DELETE FROM category_sources WHERE source = ?1 AND name = ?2",
                [source, name],
            )?;
            tx.execute(
                "DELETE FROM categories WHERE id = ?1 \
                 AND NOT EXISTS (SELECT 1 FROM category_sources WHERE category_id = ?1) \
                 AND NOT EXISTS \
                 (SELECT 1 FROM game_categories WHERE category_id = ?1 AND excluded = 0)",
                [category],
            )?;
        }
        tx.commit()?;
        Ok(uuids(changed))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::db::NewGameRecord;
    use crate::model::{GameQuery, HiddenFilter, ImportedGame, NO_CATEGORY};

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn store_game(db: &Database, source: &str, id: &str) -> Uuid {
        db.upsert_imported(source, &[ImportedGame::installed(id, id)], Utc::now())
            .unwrap();
        db.list_games(&GameQuery {
            hidden: HiddenFilter::All,
            ..GameQuery::default()
        })
        .unwrap()
        .into_iter()
        .find(|g| g.source == source && g.source_game_id.as_deref() == Some(id))
        .unwrap()
        .id
    }

    fn names(db: &Database, game: Uuid) -> Vec<String> {
        let all = db.list_categories().unwrap();
        let mut names: Vec<String> = db
            .get_game(game)
            .unwrap()
            .categories
            .iter()
            .map(|id| all.iter().find(|c| &c.id == id).unwrap().name.clone())
            .collect();
        names.sort();
        names
    }

    fn listed(db: &Database) -> Vec<(String, u32)> {
        db.list_categories()
            .unwrap()
            .into_iter()
            .map(|c| (c.name, c.game_count))
            .collect()
    }

    fn by_name(db: &Database, name: &str) -> Category {
        db.list_categories()
            .unwrap()
            .into_iter()
            .find(|c| c.name == name)
            .unwrap()
    }

    fn map(entries: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(id, names)| {
                (
                    (*id).to_string(),
                    names.iter().map(|n| (*n).to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn create_rename_reorder_delete_and_counts() {
        let db = db();
        let rpg = db.create_category(" RPG ").unwrap();
        assert_eq!(rpg.name, "RPG");
        assert_eq!(db.create_category("rpg").unwrap().id, rpg.id);
        assert!(db.create_category("  ").is_err());
        let coop = db.create_category("Co-op").unwrap();
        assert!(db.rename_category(&coop.id, "RPG").is_err());
        assert_eq!(
            db.rename_category(&coop.id, "Couch co-op").unwrap().name,
            "Couch co-op"
        );

        let game = db
            .insert_game(
                &NewGameRecord {
                    name: "Hades".into(),
                    source: "manual".into(),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap();
        db.set_game_categories(game.id, &[rpg.id.clone(), coop.id.clone()])
            .unwrap();
        assert_eq!(names(&db, game.id), vec!["Couch co-op", "RPG"]);
        // Created order first, then the user's.
        assert_eq!(
            listed(&db),
            vec![("RPG".into(), 1), ("Couch co-op".into(), 1)]
        );
        let backlog = db.create_category("Backlog").unwrap();
        db.reorder_categories(&[backlog.id.clone(), "gone".into(), coop.id.clone()])
            .unwrap();
        assert_eq!(
            listed(&db),
            vec![
                ("Backlog".into(), 0),
                ("Couch co-op".into(), 1),
                ("RPG".into(), 1)
            ]
        );

        db.delete_category(&rpg.id).unwrap();
        assert_eq!(names(&db, game.id), vec!["Couch co-op"]);
        assert!(db.delete_category(&rpg.id).is_err());
        assert!(db.set_game_categories(game.id, &["nope".into()]).is_err());
        // Deleting the game removes its links.
        db.delete_game(game.id).unwrap();
        assert_eq!(by_name(&db, "Couch co-op").game_count, 0);
    }

    #[test]
    fn filters_by_any_category_or_none() {
        let db = db();
        let a = store_game(&db, "steam", "1");
        let b = store_game(&db, "steam", "2");
        let c = store_game(&db, "steam", "3");
        let rpg = db.create_category("RPG").unwrap();
        db.sync_imported_categories("steam", &map(&[("2", &["Co-op"])]))
            .unwrap();
        let coop = by_name(&db, "Co-op");
        db.set_game_categories(a, std::slice::from_ref(&rpg.id))
            .unwrap();
        db.set_game_categories(b, &[coop.id.clone(), rpg.id.clone()])
            .unwrap();
        let ids = |categories: Vec<String>| {
            let query = GameQuery {
                categories,
                ..GameQuery::default()
            };
            let mut ids: Vec<Uuid> = db
                .list_games(&query)
                .unwrap()
                .iter()
                .map(|g| g.id)
                .collect();
            ids.sort();
            assert_eq!(db.count_matching(&query).unwrap(), ids.len() as u64);
            ids
        };
        let mut ab = vec![a, b];
        ab.sort();
        assert_eq!(ids(vec![rpg.id.clone()]), ab);
        assert_eq!(ids(vec![coop.id.clone()]), vec![b]);
        assert_eq!(ids(vec![NO_CATEGORY.into()]), vec![c]);
        let mut bc = vec![b, c];
        bc.sort();
        assert_eq!(ids(vec![coop.id, NO_CATEGORY.into()]), bc);
    }

    #[test]
    fn store_imports_map_to_categories_and_keep_user_changes() {
        let db = db();
        let portal = store_game(&db, "steam", "400");
        let hades = store_game(&db, "steam", "1145360");
        let mine = db.create_category("Favorites of 2024").unwrap();
        db.set_game_categories(portal, std::slice::from_ref(&mine.id))
            .unwrap();

        let changed = db
            .sync_imported_categories(
                "steam",
                &map(&[
                    ("400", &["Puzzle", "Co-op"]),
                    ("1145360", &["roguelike", "Puzzle"]),
                    ("999", &["Not in the library"]),
                ]),
            )
            .unwrap();
        assert_eq!(changed.len(), 2);
        assert_eq!(
            names(&db, portal),
            vec!["Co-op", "Favorites of 2024", "Puzzle"]
        );
        assert_eq!(names(&db, hades), vec!["Puzzle", "roguelike"]);
        // New store categories go to the end, by name.
        assert_eq!(
            listed(&db),
            vec![
                ("Favorites of 2024".into(), 1),
                ("Co-op".into(), 1),
                ("Puzzle".into(), 2),
                ("roguelike".into(), 1)
            ]
        );
        assert_eq!(
            by_name(&db, "Puzzle").sources,
            vec![CategorySource {
                source: "steam".into(),
                name: "Puzzle".into()
            }]
        );

        // Same data again (other spelling): nothing changes.
        assert!(
            db.sync_imported_categories(
                "steam",
                &map(&[
                    ("400", &["Puzzle", "Co-op"]),
                    ("1145360", &["Roguelike", "Puzzle"])
                ]),
            )
            .unwrap()
            .is_empty()
        );

        // Renaming keeps the mapping; taking an imported category off a game sticks.
        let coop = by_name(&db, "Co-op");
        db.rename_category(&coop.id, "Two players").unwrap();
        db.set_game_categories(hades, &[]).unwrap();
        let again = map(&[
            ("400", &["Puzzle", "Co-op"]),
            ("1145360", &["Roguelike", "Puzzle"]),
        ]);
        db.sync_imported_categories("steam", &again).unwrap();
        assert_eq!(
            names(&db, portal),
            vec!["Favorites of 2024", "Puzzle", "Two players"]
        );
        assert!(names(&db, hades).is_empty());
        db.set_game_categories(hades, std::slice::from_ref(&by_name(&db, "Puzzle").id))
            .unwrap();
        assert_eq!(names(&db, hades), vec!["Puzzle"]);

        // Steam drops co-op and roguelike: their categories go, the user's stays.
        db.sync_imported_categories("steam", &map(&[("400", &["Puzzle"])]))
            .unwrap();
        assert_eq!(names(&db, portal), vec!["Favorites of 2024", "Puzzle"]);
        assert_eq!(
            listed(&db),
            vec![("Favorites of 2024".into(), 1), ("Puzzle".into(), 1)]
        );

        // A deleted category comes back with the next sync.
        db.delete_category(&by_name(&db, "Puzzle").id).unwrap();
        db.sync_imported_categories("steam", &map(&[("400", &["Puzzle"])]))
            .unwrap();
        assert_eq!(names(&db, portal), vec!["Favorites of 2024", "Puzzle"]);
    }

    #[test]
    fn merging_and_separating_store_categories() {
        let db = db();
        let witcher_gog = store_game(&db, "gog", "1");
        let hades = store_game(&db, "steam", "2");
        let celeste = store_game(&db, "steam", "3");
        db.sync_imported_categories("steam", &map(&[("2", &["Playing"]), ("3", &["Done"])]))
            .unwrap();
        db.sync_imported_categories("gog", &map(&[("1", &["Currently playing"])]))
            .unwrap();
        let playing = by_name(&db, "Playing");
        let current = by_name(&db, "Currently playing");
        let done = by_name(&db, "Done");
        // The user took Celeste out of "Done", then put it into "Playing" by hand.
        db.set_game_categories(celeste, std::slice::from_ref(&playing.id))
            .unwrap();

        assert!(db.merge_categories(&playing.id, &playing.id).is_err());
        let mut changed = db.merge_categories(&current.id, &playing.id).unwrap();
        changed.sort();
        assert_eq!(changed, vec![witcher_gog]);
        let merged = by_name(&db, "Playing");
        assert_eq!(merged.game_count, 3);
        assert_eq!(
            merged
                .sources
                .iter()
                .map(|s| (s.source.as_str(), s.name.as_str()))
                .collect::<Vec<_>>(),
            vec![("gog", "Currently playing"), ("steam", "Playing")]
        );
        // Syncs keep feeding the merged category.
        db.sync_imported_categories("gog", &map(&[("1", &["currently playing"])]))
            .unwrap();
        assert_eq!(names(&db, witcher_gog), vec!["Playing"]);

        // Merging "Done" in: Celeste's exclusion from "Done" doesn't take it out of "Playing".
        db.merge_categories(&done.id, &playing.id).unwrap();
        assert_eq!(names(&db, celeste), vec!["Playing"]);
        assert_eq!(names(&db, hades), vec!["Playing"]);

        // Separating GOG's category recreates it right after "Playing".
        assert!(
            db.separate_category_source(&playing.id, "steam", "Playing")
                .is_err()
        );
        let (separate, changed) = db
            .separate_category_source(&playing.id, "gog", "CURRENTLY PLAYING")
            .unwrap();
        assert_eq!(changed, vec![witcher_gog]);
        assert_eq!(names(&db, witcher_gog), vec!["Currently playing"]);
        assert_eq!(db.category(&separate).unwrap().name, "Currently playing");
        assert_eq!(
            listed(&db),
            vec![("Playing".into(), 2), ("Currently playing".into(), 1)]
        );
    }
}
