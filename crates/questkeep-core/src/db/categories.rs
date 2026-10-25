//! Categories (user tags) and their n:m links to games.

use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::Category;

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

/// The category with this name, created (with `source` as its origin) when missing.
fn ensure_category(tx: &Transaction<'_>, name: &str, source: Option<&str>) -> Result<String> {
    if let Some(id) = id_by_name(tx, name)? {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO categories (id, name, source) VALUES (?1, ?2, ?3)",
        params![id, name, source],
    )?;
    Ok(id)
}

impl Database {
    /// Every category with its game count, ordered by name.
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT c.id, c.name, c.source, \
             (SELECT COUNT(*) FROM game_categories gc WHERE gc.category_id = c.id) \
             FROM categories c ORDER BY c.name COLLATE NOCASE, c.id",
        )?;
        let categories = stmt
            .query_map([], |r| {
                Ok(Category {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    source: r.get(2)?,
                    game_count: u32::try_from(r.get::<_, i64>(3)?).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(categories)
    }

    fn category(&self, id: &str) -> Result<Category> {
        self.list_categories()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(not_found)
    }

    /// Creates a category, or returns the existing one with the same name (ignoring case).
    pub fn create_category(&self, name: &str) -> Result<Category> {
        let name = clean_name(name)?;
        let id = {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let id = ensure_category(&tx, &name, None)?;
            tx.commit()?;
            id
        };
        self.category(&id)
    }

    /// Renames a category. It becomes the user's own: its links no longer follow store imports
    /// (a store that still has the old name brings that name back as a new category).
    pub fn rename_category(&self, id: &str, name: &str) -> Result<Category> {
        let name = clean_name(name)?;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            if let Some(other) = id_by_name(&tx, &name)?
                && other != id
            {
                return Err(CoreError::Invalid(format!(
                    "A category named “{name}” already exists."
                )));
            }
            let changed = tx.execute(
                "UPDATE categories SET name = ?2, source = NULL WHERE id = ?1",
                params![id, name],
            )?;
            if changed == 0 {
                return Err(not_found());
            }
            tx.execute(
                "UPDATE game_categories SET imported_from = NULL WHERE category_id = ?1",
                [id],
            )?;
            tx.commit()?;
        }
        self.category(id)
    }

    /// Deletes a category and its links; the games stay.
    pub fn delete_category(&self, id: &str) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM categories WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(not_found());
        }
        Ok(())
    }

    /// Gives a game exactly these categories. Links it already has stay as they are (also
    /// imported ones); new ones are the user's.
    pub fn set_game_categories(&self, game_id: Uuid, category_ids: &[String]) -> Result<()> {
        let game = game_id.to_string();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM games WHERE id = ?1)",
            [&game],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(CoreError::GameNotFound(game_id));
        }
        let wanted: HashSet<&str> = category_ids.iter().map(String::as_str).collect();
        let current: Vec<String> = tx
            .prepare("SELECT category_id FROM game_categories WHERE game_id = ?1")?
            .query_map([&game], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in &current {
            if !wanted.contains(id.as_str()) {
                tx.execute(
                    "DELETE FROM game_categories WHERE game_id = ?1 AND category_id = ?2",
                    params![game, id],
                )?;
            }
        }
        for id in wanted {
            let known: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM categories WHERE id = ?1)",
                [id],
                |r| r.get(0),
            )?;
            if !known {
                return Err(not_found());
            }
            tx.execute(
                "INSERT INTO game_categories (game_id, category_id) VALUES (?1, ?2) \
                 ON CONFLICT DO NOTHING",
                params![game, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Replaces the category links a store imported with `by_game` (store game id -> category
    /// names). Categories are matched by name and created when missing; links the user set are
    /// kept, and categories this store created that no game has anymore are removed. Returns the
    /// games whose categories changed.
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

        let mut ids_by_name: HashMap<String, String> = HashMap::new();
        let mut wanted: HashSet<(String, String)> = HashSet::new();
        for (store_id, names) in by_game {
            let Some(game) = games.get(store_id) else {
                continue; // not in the library (e.g. an owned game the scan skipped)
            };
            for name in names {
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                let key = name.to_lowercase();
                let category = match ids_by_name.get(&key) {
                    Some(id) => id.clone(),
                    None => {
                        let id = ensure_category(&tx, name, Some(source))?;
                        ids_by_name.insert(key, id.clone());
                        id
                    }
                };
                wanted.insert((game.clone(), category));
            }
        }

        let current: HashSet<(String, String)> = tx
            .prepare("SELECT game_id, category_id FROM game_categories WHERE imported_from = ?1")?
            .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let mut changed: HashSet<String> = HashSet::new();
        for (game, category) in current.difference(&wanted) {
            tx.execute(
                "DELETE FROM game_categories \
                 WHERE game_id = ?1 AND category_id = ?2 AND imported_from = ?3",
                params![game, category, source],
            )?;
            changed.insert(game.clone());
        }
        for (game, category) in wanted.difference(&current) {
            let added = tx.execute(
                "INSERT INTO game_categories (game_id, category_id, imported_from) \
                 VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
                params![game, category, source],
            )?;
            if added > 0 {
                changed.insert(game.clone());
            }
        }
        tx.execute(
            "DELETE FROM categories WHERE source = ?1 AND NOT EXISTS \
             (SELECT 1 FROM game_categories gc WHERE gc.category_id = categories.id)",
            [source],
        )?;
        tx.commit()?;

        let mut ids: Vec<Uuid> = changed
            .iter()
            .filter_map(|id| Uuid::parse_str(id).ok())
            .collect();
        ids.sort();
        Ok(ids)
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
    fn create_rename_delete_and_counts() {
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
        let listed = db.list_categories().unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|c| (c.name.as_str(), c.game_count))
                .collect::<Vec<_>>(),
            vec![("Couch co-op", 1), ("RPG", 1)]
        );

        db.delete_category(&rpg.id).unwrap();
        assert_eq!(names(&db, game.id), vec!["Couch co-op"]);
        assert!(db.delete_category(&rpg.id).is_err());
        assert!(db.set_game_categories(game.id, &["nope".into()]).is_err());
        // Deleting the game removes its links.
        db.delete_game(game.id).unwrap();
        assert_eq!(db.list_categories().unwrap()[0].game_count, 0);
    }

    #[test]
    fn filters_by_any_category_or_none() {
        let db = db();
        let a = store_game(&db, "steam", "1");
        let b = store_game(&db, "steam", "2");
        let c = store_game(&db, "steam", "3");
        let rpg = db.create_category("RPG").unwrap();
        let coop = db.create_category("Co-op").unwrap();
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
    fn store_imports_replace_only_their_own_links() {
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
        let steam_made = db.list_categories().unwrap();
        assert_eq!(
            steam_made
                .iter()
                .find(|c| c.name == "Puzzle")
                .unwrap()
                .source
                .as_deref(),
            Some("steam")
        );
        assert!(!steam_made.iter().any(|c| c.name == "Not in the library"));

        // Same data again: nothing changes.
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

        // The user keeps "Co-op" on Portal by renaming it; Steam drops co-op and roguelike.
        let coop = steam_made.iter().find(|c| c.name == "Co-op").unwrap();
        db.rename_category(&coop.id, "Two players").unwrap();
        db.sync_imported_categories("steam", &map(&[("400", &["Puzzle"])]))
            .unwrap();
        assert_eq!(
            names(&db, portal),
            vec!["Favorites of 2024", "Puzzle", "Two players"]
        );
        assert!(names(&db, hades).is_empty());
        let left: Vec<String> = db
            .list_categories()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(left, vec!["Favorites of 2024", "Puzzle", "Two players"]);

        // A user category with a store's name gets that store's links without becoming the store's.
        db.sync_imported_categories("steam", &map(&[("1145360", &["favorites OF 2024"])]))
            .unwrap();
        assert_eq!(names(&db, hades), vec!["Favorites of 2024"]);
        assert_eq!(
            db.list_categories()
                .unwrap()
                .iter()
                .find(|c| c.id == mine.id)
                .unwrap()
                .source,
            None
        );
    }
}
