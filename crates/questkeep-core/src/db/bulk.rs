//! Changes to several library entries at once (multi-select), each in one transaction.

use rusqlite::params;
use uuid::Uuid;

use super::Database;
use super::categories::{game_categories, set_categories};
use crate::error::{CoreError, Result};
use crate::model::BulkChange;

impl Database {
    /// Applies `change` to every entry in `ids`; nothing changes when one of them is unknown.
    pub fn bulk_change(&self, ids: &[Uuid], change: &BulkChange) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let categories = !change.add_categories.is_empty() || !change.remove_categories.is_empty();
        for id in ids {
            let key = id.to_string();
            let found = tx.execute(
                "UPDATE games SET favorite = COALESCE(?2, favorite), \
                 hidden = COALESCE(?3, hidden) WHERE id = ?1",
                params![key, change.favorite, change.hidden],
            )?;
            if found == 0 {
                return Err(CoreError::GameNotFound(*id));
            }
            if categories {
                let mut wanted = game_categories(&tx, &key)?;
                for category in &change.remove_categories {
                    wanted.remove(category);
                }
                wanted.extend(change.add_categories.iter().cloned());
                set_categories(&tx, *id, &wanted.into_iter().collect::<Vec<_>>())?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes several entries; nothing is deleted when one of them is unknown.
    pub fn delete_games(&self, ids: &[Uuid]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for id in ids {
            if tx.execute("DELETE FROM games WHERE id = ?1", [id.to_string()])? == 0 {
                return Err(CoreError::GameNotFound(*id));
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::model::{GameQuery, HiddenFilter, ImportedGame};

    fn games(db: &Database) -> Vec<(String, bool, bool, Vec<String>)> {
        let mut list: Vec<_> = db
            .list_games(&GameQuery {
                hidden: HiddenFilter::All,
                ..GameQuery::default()
            })
            .unwrap()
            .into_iter()
            .map(|g| {
                let mut categories = g.categories.clone();
                categories.sort();
                (g.name, g.favorite, g.hidden, categories)
            })
            .collect();
        list.sort();
        list
    }

    #[test]
    fn changes_several_games_at_once_or_none() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_imported(
            "steam",
            &[
                ImportedGame::installed("1", "A"),
                ImportedGame::installed("2", "B"),
            ],
            Utc::now(),
        )
        .unwrap();
        let ids: Vec<Uuid> = db
            .list_games(&GameQuery::default())
            .unwrap()
            .iter()
            .map(|g| g.id)
            .collect();
        let rpg = db.create_category("RPG").unwrap().id;
        let coop = db.create_category("Co-op").unwrap().id;
        db.set_game_categories(ids[0], std::slice::from_ref(&coop))
            .unwrap();

        db.bulk_change(
            &ids,
            &BulkChange {
                favorite: Some(true),
                add_categories: vec![rpg.clone()],
                remove_categories: vec![coop.clone()],
                ..BulkChange::default()
            },
        )
        .unwrap();
        let mut both = vec![rpg.clone()];
        both.sort();
        assert_eq!(
            games(&db),
            vec![
                ("A".into(), true, false, both.clone()),
                ("B".into(), true, false, both.clone()),
            ]
        );

        // An unknown id rolls the whole change back.
        let mut with_missing = ids.clone();
        with_missing.push(Uuid::new_v4());
        let hide = BulkChange {
            hidden: Some(true),
            ..BulkChange::default()
        };
        assert!(db.bulk_change(&with_missing, &hide).is_err());
        assert!(games(&db).iter().all(|g| !g.2));
        assert!(db.delete_games(&with_missing).is_err());
        assert_eq!(games(&db).len(), 2);

        db.delete_games(&ids).unwrap();
        assert!(games(&db).is_empty());
    }
}
