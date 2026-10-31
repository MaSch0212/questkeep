//! One library entry for a game owned in several stores: combining entries, separating copies
//! again, the copies' play order, and finding likely duplicates.

use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use super::Database;
use super::categories::{game_categories, set_categories};
use crate::error::{CoreError, Result};
use crate::model::{CombinePlan, Game, GameQuery, HiddenFilter, KindFilter, UserEdits};

/// The entry a game row belongs to: itself, or the entry it was combined into.
fn entry_of(tx: &Transaction<'_>, id: Uuid) -> Result<String> {
    tx.query_row(
        "SELECT COALESCE(merged_into, id) FROM games WHERE id = ?1",
        [id.to_string()],
        |r| r.get(0),
    )
    .optional()?
    .ok_or(CoreError::GameNotFound(id))
}

/// An entry's copies (its own row included) in play order.
fn copies_of(tx: &Transaction<'_>, entry: &str) -> Result<Vec<String>> {
    Ok(tx
        .prepare(
            "SELECT id FROM games WHERE id = ?1 OR merged_into = ?1 \
             ORDER BY copy_position, merged_into IS NOT NULL, date_added, id",
        )?
        .query_map([entry], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn number_copies(tx: &Transaction<'_>, ids: &[String]) -> Result<()> {
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE games SET copy_position = ?2 WHERE id = ?1",
            params![id, i64::try_from(position).unwrap_or(i64::MAX)],
        )?;
    }
    Ok(())
}

/// Moves the user's own data of entry `from` (favorite, categories) over to entry `to`.
fn move_user_data(tx: &Transaction<'_>, from: &str, to: &str) -> Result<()> {
    tx.execute(
        "UPDATE games SET favorite = 1 WHERE id = ?2 \
         AND (SELECT favorite FROM games WHERE id = ?1) = 1",
        [from, to],
    )?;
    // Categories either one has stay: drop the exclusions that would hide them.
    tx.execute(
        "DELETE FROM game_categories WHERE excluded = 1 AND \
         ((game_id = ?1 AND category_id IN \
           (SELECT category_id FROM game_category_links WHERE game_id = ?2)) OR \
          (game_id = ?2 AND category_id IN \
           (SELECT category_id FROM game_category_links WHERE game_id = ?1)))",
        [from, to],
    )?;
    tx.execute(
        "UPDATE OR IGNORE game_categories SET game_id = ?2 WHERE game_id = ?1",
        [from, to],
    )?;
    tx.execute("DELETE FROM game_categories WHERE game_id = ?1", [from])?;
    // Own launch profiles and the chosen default (the entry's wins), and the user's metadata.
    tx.execute(
        "UPDATE launch_profiles SET entry_id = ?2 WHERE entry_id = ?1",
        [from, to],
    )?;
    tx.execute(
        "UPDATE games SET default_profile = (SELECT default_profile FROM games WHERE id = ?1) \
         WHERE id = ?2 AND default_profile IS NULL",
        [from, to],
    )?;
    tx.execute(
        "UPDATE games SET default_profile = NULL WHERE id = ?1",
        [from],
    )?;
    tx.execute(
        "UPDATE OR IGNORE metadata_sources SET game_id = ?2 WHERE game_id = ?1 AND source = 'user'",
        [from, to],
    )?;
    Ok(())
}

fn dismiss(tx: &Transaction<'_>, a: &str, b: &str) -> Result<()> {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    tx.execute(
        "INSERT OR IGNORE INTO duplicate_dismissals (game_id, other_id) VALUES (?1, ?2)",
        [a, b],
    )?;
    Ok(())
}

/// Name key for finding the same game in several stores: case, symbols and spacing ignored,
/// so "Rocket League®" and "rocket league" match.
pub fn duplicate_key(name: &str) -> String {
    name.replace('&', " and ")
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn parse_ids(ids: &[String]) -> Vec<Uuid> {
    ids.iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect()
}

/// What combining changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combined {
    /// The entry the games are now part of.
    pub entry: Uuid,
    /// Entries that are now copies of `entry` and left the library list.
    pub absorbed: Vec<Uuid>,
}

impl Database {
    /// Combines the entries of `others` into the entry of `into`: their copies join its
    /// copies (after them), favorites and categories carry over, and name, cover and details
    /// stay `into`'s.
    pub fn combine_games(&self, into: Uuid, others: &[Uuid]) -> Result<Combined> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let entry = entry_of(&tx, into)?;
        let mut absorbed: Vec<String> = Vec::new();
        for other in others {
            let other = entry_of(&tx, *other)?;
            if other != entry && !absorbed.contains(&other) {
                absorbed.push(other);
            }
        }
        if absorbed.is_empty() {
            return Err(CoreError::Invalid(
                "Pick another game to combine it with.".into(),
            ));
        }
        let mut order = copies_of(&tx, &entry)?;
        for other in &absorbed {
            order.extend(copies_of(&tx, other)?);
            move_user_data(&tx, other, &entry)?;
            tx.execute(
                "UPDATE games SET merged_into = ?2 WHERE id = ?1 OR merged_into = ?1",
                [other, &entry],
            )?;
        }
        number_copies(&tx, &order)?;
        tx.commit()?;
        Ok(Combined {
            entry: Uuid::parse_str(&entry).map_err(|_| CoreError::GameNotFound(into))?,
            absorbed: parse_ids(&absorbed),
        })
    }

    /// Combines several entries into `plan.base` keeping what the plan chose (the combine
    /// screen), all in one transaction. `edits` are the user's edited details the combined
    /// entry keeps (the caller folds the chosen links into them). Images are left to the caller.
    pub fn combine_with_plan(&self, plan: &CombinePlan, edits: &UserEdits) -> Result<Combined> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let entries = &plan.entries;
        let distinct: HashSet<&String> = entries.iter().collect();
        if distinct.len() < 2 || distinct.len() != entries.len() {
            return Err(CoreError::Invalid(
                "Pick at least two different games to combine.".into(),
            ));
        }
        for id in entries {
            let uuid = Uuid::parse_str(id)
                .map_err(|_| CoreError::Invalid("That game no longer exists.".into()))?;
            if &entry_of(&tx, uuid)? != id {
                return Err(CoreError::Invalid(
                    "One of the games is already part of another game.".into(),
                ));
            }
        }
        let singles = [
            &plan.base,
            &plan.name_from,
            &plan.images_from,
            &plan.details_from,
            &plan.status_from,
            &plan.score_from,
            &plan.profile_from,
        ];
        let lists = [
            &plan.categories_from,
            &plan.notes_from,
            &plan.links_from,
            &plan.profiles_from,
        ];
        if singles.iter().any(|id| !distinct.contains(id))
            || lists
                .iter()
                .any(|ids| ids.iter().any(|id| !distinct.contains(id)))
        {
            return Err(CoreError::Invalid(
                "The choices refer to a game that is not being combined.".into(),
            ));
        }

        // What the chosen entries hold before their rows move.
        let (name, sorting, search, sorting_user): (String, String, String, Option<String>) = tx
            .query_row(
                "SELECT name, sorting_name, search_key, sorting_name_user FROM games WHERE id = ?1",
                [&plan.name_from],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let value = |column: &str, id: &str| -> Result<Option<rusqlite::types::Value>> {
            Ok(tx.query_row(
                &format!("SELECT {column} FROM games WHERE id = ?1"),
                [id],
                |r| r.get(0),
            )?)
        };
        let status = value("completion_status", &plan.status_from)?;
        let score = value("user_score", &plan.score_from)?;
        let profile: Option<String> = tx.query_row(
            "SELECT default_profile FROM games WHERE id = ?1",
            [&plan.profile_from],
            |r| r.get(0),
        )?;
        let mut notes = Vec::new();
        for id in &plan.notes_from {
            let note: Option<String> =
                tx.query_row("SELECT notes FROM games WHERE id = ?1", [id], |r| r.get(0))?;
            if let Some(note) = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
                notes.push(note);
            }
        }
        let mut categories: HashSet<String> = HashSet::new();
        for id in &plan.categories_from {
            categories.extend(game_categories(&tx, id)?);
        }
        let mut dropped_profiles: Vec<String> = Vec::new();
        for id in entries.iter().filter(|id| !plan.profiles_from.contains(id)) {
            let ids: Vec<String> = tx
                .prepare("SELECT id FROM launch_profiles WHERE entry_id = ?1")?
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            dropped_profiles.extend(ids);
        }

        // The base's copies first, then the others' in plan order.
        let base = &plan.base;
        let mut order = copies_of(&tx, base)?;
        let mut absorbed: Vec<String> = Vec::new();
        for other in entries.iter().filter(|id| *id != base) {
            order.extend(copies_of(&tx, other)?);
            move_user_data(&tx, other, base)?;
            tx.execute(
                "UPDATE games SET merged_into = ?2 WHERE id = ?1 OR merged_into = ?1",
                [other, base],
            )?;
            absorbed.push(other.clone());
        }
        number_copies(&tx, &order)?;

        // The choices.
        for id in &dropped_profiles {
            tx.execute("DELETE FROM launch_profiles WHERE id = ?1", [id])?;
        }
        let profile = profile.filter(|p| !dropped_profiles.contains(p));
        let notes = (!notes.is_empty()).then(|| notes.join("\n\n"));
        tx.execute(
            "UPDATE games SET favorite = ?2, completion_status = ?3, user_score = ?4, notes = ?5, \
             default_profile = ?6 WHERE id = ?1",
            params![base, plan.favorite, status, score, notes, profile],
        )?;
        if &plan.name_from != base {
            tx.execute(
                "UPDATE games SET name = ?2, sorting_name = ?3, search_key = ?4, \
                 sorting_name_user = ?5, extra = json_set(COALESCE(extra, '{}'), ?6, json('true')) \
                 WHERE id = ?1",
                params![
                    base,
                    name,
                    sorting,
                    search,
                    sorting_user,
                    format!("$.{}", super::games::USER_RENAMED)
                ],
            )?;
        }
        let entry = Uuid::parse_str(base)
            .map_err(|_| CoreError::Invalid("That game no longer exists.".into()))?;
        set_categories(&tx, entry, &categories.into_iter().collect::<Vec<_>>())?;
        tx.execute(
            "DELETE FROM metadata_sources WHERE source = 'user' AND game_id IN \
             (SELECT id FROM games WHERE id = ?1 OR merged_into = ?1)",
            [base],
        )?;
        if !edits.is_empty() {
            tx.execute(
                "INSERT INTO metadata_sources (game_id, source, data, checked_at) \
                 VALUES (?1, 'user', ?2, ?3)",
                params![base, serde_json::to_string(edits)?, chrono::Utc::now()],
            )?;
        }
        tx.commit()?;
        Ok(Combined {
            entry,
            absorbed: parse_ids(&absorbed),
        })
    }

    /// Takes one store's copy out of its entry again, as an entry of its own. When it is the
    /// entry's own row, the next copy becomes the entry and keeps the favorite and categories.
    /// The two are not suggested as duplicates again. Returns the remaining entry.
    pub fn separate_copy(&self, copy: Uuid) -> Result<Uuid> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let entry = entry_of(&tx, copy)?;
        let copies = copies_of(&tx, &entry)?;
        if copies.len() < 2 {
            return Err(CoreError::Invalid(
                "This game isn't combined with another store's copy.".into(),
            ));
        }
        let copy = copy.to_string();
        let remaining_entry = if copy == entry {
            let next = copies
                .iter()
                .find(|id| **id != entry)
                .cloned()
                .unwrap_or_default();
            tx.execute(
                "UPDATE games SET merged_into = ?2 WHERE merged_into = ?1 AND id != ?2",
                [&entry, &next],
            )?;
            tx.execute("UPDATE games SET merged_into = NULL WHERE id = ?1", [&next])?;
            tx.execute(
                "UPDATE games SET hidden = (SELECT hidden FROM games WHERE id = ?1) WHERE id = ?2",
                [&entry, &next],
            )?;
            move_user_data(&tx, &entry, &next)?;
            tx.execute("UPDATE games SET favorite = 0 WHERE id = ?1", [&entry])?;
            next
        } else {
            tx.execute("UPDATE games SET merged_into = NULL WHERE id = ?1", [&copy])?;
            entry.clone()
        };
        tx.execute("UPDATE games SET copy_position = 0 WHERE id = ?1", [&copy])?;
        // Own profiles based on the copy go with it.
        tx.execute(
            "UPDATE launch_profiles SET entry_id = ?1 WHERE copy_id = ?1 AND entry_id IS NOT NULL",
            [&copy],
        )?;
        let rest: Vec<String> = copies.into_iter().filter(|id| *id != copy).collect();
        number_copies(&tx, &rest)?;
        for other in &rest {
            dismiss(&tx, &copy, other)?;
        }
        tx.commit()?;
        Uuid::parse_str(&remaining_entry).map_err(|_| CoreError::Invalid("Corrupt game id.".into()))
    }

    /// Makes `copy` the first of its entry's copies: Play starts it when it is installed.
    pub fn prefer_copy(&self, copy: Uuid) -> Result<Uuid> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let entry = entry_of(&tx, copy)?;
        let copy = copy.to_string();
        let mut order = copies_of(&tx, &entry)?;
        order.retain(|id| *id != copy);
        order.insert(0, copy);
        number_copies(&tx, &order)?;
        tx.commit()?;
        Uuid::parse_str(&entry).map_err(|_| CoreError::Invalid("Corrupt game id.".into()))
    }

    /// Remembers that these entries are different games, so they aren't suggested again.
    pub fn dismiss_duplicates(&self, ids: &[Uuid]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let ids: Vec<String> = ids.iter().map(Uuid::to_string).collect();
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                if a != b {
                    dismiss(&tx, a, b)?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Entries that look like the same game in different stores (same name, ignoring case and
    /// symbols), grouped. Each group is ordered with the best entry to keep first (one with a
    /// cover, then the most played, then the oldest). Entries never share a store within a
    /// group, and pairs the user dismissed or separated stay apart.
    pub fn find_duplicates(&self) -> Result<Vec<Vec<Game>>> {
        let entries = self.list_games(&GameQuery {
            hidden: HiddenFilter::All,
            kind: KindFilter::Both,
            ..GameQuery::default()
        })?;
        let dismissed: HashSet<(String, String)> = {
            let conn = self.conn();
            let mut stmt = conn.prepare("SELECT game_id, other_id FROM duplicate_dismissals")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        let apart = |a: &Game, b: &Game| {
            a.copies.iter().any(|x| {
                b.copies.iter().any(|y| {
                    let (x, y) = (x.id.to_string(), y.id.to_string());
                    let pair = if x <= y { (x, y) } else { (y, x) };
                    dismissed.contains(&pair)
                })
            })
        };

        let mut by_key: HashMap<String, Vec<Game>> = HashMap::new();
        for game in entries {
            let key = duplicate_key(&game.name);
            if !key.is_empty() {
                by_key.entry(key).or_default().push(game);
            }
        }
        let mut groups: Vec<Vec<Game>> = Vec::new();
        for (_, mut candidates) in by_key {
            if candidates.len() < 2 {
                continue;
            }
            candidates.sort_by(|a, b| {
                b.cover_path
                    .is_some()
                    .cmp(&a.cover_path.is_some())
                    .then(b.playtime_seconds.cmp(&a.playtime_seconds))
                    .then(a.date_added.cmp(&b.date_added))
                    .then(a.id.cmp(&b.id))
            });
            let mut formed: Vec<Vec<Game>> = Vec::new();
            for game in candidates {
                let fits = |group: &Vec<Game>| {
                    group.iter().all(|other| {
                        !apart(&game, other)
                            && !other
                                .copies
                                .iter()
                                .any(|c| game.copies.iter().any(|d| d.source == c.source))
                    })
                };
                match formed.iter_mut().find(|group| fits(group)) {
                    Some(group) => group.push(game),
                    None => formed.push(vec![game]),
                }
            }
            groups.extend(formed.into_iter().filter(|group| group.len() > 1));
        }
        groups.sort_by_key(|group| {
            group
                .first()
                .map(|g| g.sorting_name.to_lowercase())
                .unwrap_or_default()
        });
        Ok(groups)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::model::ImportedGame;

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn add(db: &Database, source: &str, id: &str, name: &str, installed: bool) -> Uuid {
        let mut game = ImportedGame::installed(id, name);
        game.installed = installed;
        game.playtime_seconds = Some(60);
        db.upsert_imported(source, &[game], Utc::now()).unwrap();
        db.list_games(&GameQuery {
            hidden: HiddenFilter::All,
            ..GameQuery::default()
        })
        .unwrap()
        .into_iter()
        .flat_map(|g| g.copies)
        .find(|c| c.source == source && c.source_game_id.as_deref() == Some(id))
        .unwrap()
        .id
    }

    fn all(db: &Database) -> Vec<Game> {
        db.list_games(&GameQuery::default()).unwrap()
    }

    #[test]
    fn keys_ignore_case_and_symbols() {
        assert_eq!(
            duplicate_key("Rocket League®"),
            duplicate_key("rocket league")
        );
        assert_eq!(
            duplicate_key("Ori & the Blind Forest"),
            "oriandtheblindforest"
        );
        assert_eq!(
            duplicate_key("DOOM: Eternal"),
            duplicate_key("Doom Eternal")
        );
        assert_ne!(duplicate_key("Doom"), duplicate_key("Doom II"));
        assert_eq!(duplicate_key("™"), "");
    }

    #[test]
    fn combined_games_show_once_with_every_store() {
        let db = db();
        let steam = add(&db, "steam", "252950", "Rocket League", false);
        let epic = add(&db, "epic", "sugar", "Rocket League®", true);
        add(&db, "steam", "1", "Other", true);
        let tag = db.create_category("Cars").unwrap();
        db.set_game_categories(epic, std::slice::from_ref(&tag.id))
            .unwrap();
        db.update_game(
            epic,
            &crate::model::GameUpdate {
                favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(db.combine_games(steam, &[steam]).is_err());
        let combined = db.combine_games(steam, &[epic]).unwrap();
        assert_eq!(combined.entry, steam);
        assert_eq!(combined.absorbed, vec![epic]);

        let games = all(&db);
        assert_eq!(games.len(), 2);
        let entry = db.get_entry(epic).unwrap();
        assert_eq!(entry.id, steam);
        assert_eq!(entry.name, "Rocket League");
        assert!(entry.installed, "installed in one of its stores");
        assert!(entry.favorite);
        assert_eq!(entry.playtime_seconds, 120);
        assert_eq!(entry.categories, vec![tag.id.clone()]);
        assert_eq!(
            entry
                .copies
                .iter()
                .map(|c| (c.source.as_str(), c.installed))
                .collect::<Vec<_>>(),
            vec![("steam", false), ("epic", true)]
        );
        // Filters see every store; a single copy is still readable as itself.
        let query = |sources: &[&str]| GameQuery {
            sources: sources.iter().map(|s| (*s).to_string()).collect(),
            installed: crate::model::TriState::Yes,
            ..GameQuery::default()
        };
        assert_eq!(db.list_games(&query(&["epic"])).unwrap().len(), 1);
        assert_eq!(db.count_matching(&query(&["steam"])).unwrap(), 2);
        assert_eq!(db.get_game(epic).unwrap().source, "epic");
        assert_eq!(db.count_games().unwrap(), 2);

        // Play order: Epic first.
        assert_eq!(db.prefer_copy(epic).unwrap(), steam);
        let order: Vec<Uuid> = db
            .get_entry(steam)
            .unwrap()
            .copies
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(order, vec![epic, steam]);
    }

    #[test]
    fn combining_entries_merges_their_copies_and_separating_undoes_it() {
        let db = db();
        let steam = add(&db, "steam", "1", "Hades", true);
        let epic = add(&db, "epic", "2", "Hades", true);
        let gog = add(&db, "gog", "3", "Hades", false);
        db.combine_games(epic, &[gog]).unwrap();
        db.combine_games(steam, &[gog]).unwrap();
        let entry = db.get_entry(gog).unwrap();
        assert_eq!(entry.id, steam);
        assert_eq!(entry.copies.len(), 3);
        assert_eq!(all(&db).len(), 1);

        assert_eq!(db.separate_copy(gog).unwrap(), steam);
        assert_eq!(all(&db).len(), 2);
        assert_eq!(db.get_entry(steam).unwrap().copies.len(), 2);

        // Separating the entry's own row: Epic's copy carries on as the entry.
        let fav = db.create_category("Fav").unwrap();
        db.set_game_categories(steam, std::slice::from_ref(&fav.id))
            .unwrap();
        assert_eq!(db.separate_copy(steam).unwrap(), epic);
        let rest = db.get_entry(epic).unwrap();
        assert_eq!(rest.copies.len(), 1);
        assert_eq!(rest.categories, vec![fav.id]);
        assert!(db.get_entry(steam).unwrap().categories.is_empty());
        assert!(db.separate_copy(steam).is_err());

        // Separated pairs aren't suggested again.
        assert!(db.find_duplicates().unwrap().is_empty());
    }

    #[test]
    fn finds_the_same_game_in_different_stores() {
        let db = db();
        let steam = add(&db, "steam", "1", "Rocket League", true);
        let epic = add(&db, "epic", "2", "ROCKET LEAGUE®", false);
        // Same store twice (a playtest, say) is not a duplicate of itself.
        add(&db, "steam", "3", "Rocket League", false);
        add(&db, "gog", "4", "Celeste", true);
        let celeste_epic = add(&db, "epic", "5", "Celeste", true);
        add(&db, "xbox", "6", "Unique", true);

        let groups = db.find_duplicates().unwrap();
        let names: Vec<Vec<(String, String)>> = groups
            .iter()
            .map(|g| {
                g.iter()
                    .map(|e| (e.name.clone(), e.source.clone()))
                    .collect()
            })
            .collect();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0].len(), 2);
        assert_eq!(names[1].len(), 2);
        assert!(names[1].iter().any(|(_, s)| s == "epic"));
        assert!(names[1].iter().any(|(_, s)| s == "steam"));

        db.dismiss_duplicates(&[steam, epic]).unwrap();
        let groups = db.find_duplicates().unwrap();
        // The other Steam copy still pairs with Epic.
        assert_eq!(groups.len(), 2);
        assert!(
            groups
                .iter()
                .all(|g| !(g.iter().any(|e| e.id == steam) && g.iter().any(|e| e.id == epic)))
        );

        let celeste = groups.iter().find(|g| g[0].name == "Celeste").unwrap();
        let ids: Vec<Uuid> = celeste.iter().map(|g| g.id).collect();
        db.combine_games(ids[0], &ids[1..]).unwrap();
        assert_eq!(db.find_duplicates().unwrap().len(), 1);
        assert_eq!(db.get_entry(celeste_epic).unwrap().copies.len(), 2);
    }

    fn default_profile(db: &Database, id: Uuid) -> Option<String> {
        db.conn()
            .query_row(
                "SELECT default_profile FROM games WHERE id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .unwrap()
    }
    #[test]
    fn combining_with_a_plan_keeps_the_chosen_values() {
        let db = db();
        let a = add(&db, "steam", "1", "Hades II", true);
        let b = add(&db, "gog", "2", "HADES 2", false);
        let c = add(&db, "epic", "3", "Hades 2", false);
        let rpg = db.create_category("RPG").unwrap().id;
        let couch = db.create_category("Couch").unwrap().id;
        db.set_game_categories(a, std::slice::from_ref(&rpg))
            .unwrap();
        db.set_game_categories(b, std::slice::from_ref(&couch))
            .unwrap();
        let sql = |q: &str| db.conn().execute_batch(q).unwrap();
        sql(&format!(
            "UPDATE games SET notes = 'Moon route', user_score = 85, completion_status = 'played' \
             WHERE id = '{a}';
             UPDATE games SET notes = ' Controller works ', completion_status = 'beaten' \
             WHERE id = '{b}';
             UPDATE games SET favorite = 1 WHERE id = '{c}';
             INSERT INTO launch_profiles (id, entry_id, name, position) VALUES ('p1', '{b}', 'Vulkan', 0);
             UPDATE games SET default_profile = 'p1' WHERE id = '{b}';"
        ));
        let id = |u: Uuid| u.to_string();
        let edits = UserEdits::default();
        let mut plan = CombinePlan {
            entries: vec![id(a), id(b), id(c)],
            base: id(a),
            name_from: id(b),
            images_from: id(a),
            details_from: id(a),
            status_from: id(b),
            score_from: id(a),
            favorite: true,
            profile_from: id(b),
            categories_from: vec![id(a), id(c)],
            notes_from: vec![id(b), id(a)],
            links_from: vec![],
            profiles_from: vec![id(a), id(b), id(c)],
        };

        // Choices must refer to the games being combined.
        plan.score_from = Uuid::new_v4().to_string();
        assert!(db.combine_with_plan(&plan, &edits).is_err());
        plan.score_from = id(a);

        let combined = db.combine_with_plan(&plan, &edits).unwrap();
        assert_eq!(combined.entry, a);
        let game = db.get_game(a).unwrap();
        assert_eq!(game.name, "HADES 2");
        assert_eq!(game.completion_status.as_deref(), Some("beaten"));
        assert_eq!(game.user_score, Some(85));
        assert!(game.favorite);
        assert_eq!(
            game.notes.as_deref(),
            Some("Controller works\n\nMoon route")
        );
        assert_eq!(game.categories, vec![rpg.clone()]);
        assert_eq!(default_profile(&db, a).as_deref(), Some("p1"));
        assert_eq!(db.get_entry(a).unwrap().copies.len(), 3);
        assert_eq!(all(&db).len(), 1);
        assert_eq!(
            game.extra.get("userRenamed"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[test]
    fn a_plan_can_drop_own_launch_profiles() {
        let db = db();
        let a = add(&db, "steam", "1", "Doom", true);
        let b = add(&db, "gog", "2", "Doom", false);
        db.conn()
            .execute_batch(&format!(
                "INSERT INTO launch_profiles (id, entry_id, name, position) VALUES ('p1', '{b}', 'Mods', 0);
                 UPDATE games SET default_profile = 'p1' WHERE id = '{b}';"
            ))
            .unwrap();
        let (a, b) = (a.to_string(), b.to_string());
        let plan = CombinePlan {
            entries: vec![a.clone(), b.clone()],
            base: a.clone(),
            name_from: a.clone(),
            images_from: a.clone(),
            details_from: a.clone(),
            status_from: a.clone(),
            score_from: a.clone(),
            favorite: false,
            profile_from: b.clone(),
            categories_from: vec![],
            notes_from: vec![],
            links_from: vec![],
            profiles_from: vec![a.clone()],
        };
        db.combine_with_plan(&plan, &UserEdits::default()).unwrap();
        let profiles: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM launch_profiles", [], |r| r.get(0))
            .unwrap();
        assert_eq!(profiles, 0);
        let entry = Uuid::parse_str(&a).unwrap();
        assert_eq!(default_profile(&db, entry), None);
    }
}
