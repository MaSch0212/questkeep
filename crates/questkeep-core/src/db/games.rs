//! Games repository.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Row, ToSql, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::{
    Game, GameQuery, GameUpdate, HiddenFilter, ImportedGame, PlayAction, ScanSummary, SortField,
    sorting_name,
};

const COLUMNS: &str = "id, name, sorting_name, source, source_game_id, installed, install_dir, \
    play_action, playtime_seconds, play_count, last_played, date_added, favorite, hidden, \
    cover_path, icon_path, extra";

/// `extra` key set when the user renamed a store game, so re-scans keep their name.
const USER_RENAMED: &str = "userRenamed";

/// Data needed to insert a new game.
#[derive(Debug, Clone, Default)]
pub struct NewGameRecord {
    pub name: String,
    pub source: String,
    pub source_game_id: Option<String>,
    pub install_dir: Option<String>,
    pub play_action: Option<PlayAction>,
    pub cover_path: Option<String>,
}

fn search_key(name: &str) -> String {
    name.to_lowercase()
}

fn row_to_game(row: &Row<'_>) -> rusqlite::Result<Game> {
    let id: String = row.get("id")?;
    let play_action: Option<String> = row.get("play_action")?;
    let extra: String = row.get("extra")?;
    let as_u64 = |v: i64| u64::try_from(v).unwrap_or_default();
    Ok(Game {
        id: Uuid::parse_str(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        name: row.get("name")?,
        sorting_name: row.get("sorting_name")?,
        source: row.get("source")?,
        source_game_id: row.get("source_game_id")?,
        installed: row.get("installed")?,
        install_dir: row.get("install_dir")?,
        // A malformed action must not make the whole library unreadable.
        play_action: play_action.and_then(|json| serde_json::from_str(&json).ok()),
        playtime_seconds: as_u64(row.get("playtime_seconds")?),
        play_count: as_u64(row.get("play_count")?),
        last_played: row.get("last_played")?,
        date_added: row.get("date_added")?,
        favorite: row.get("favorite")?,
        hidden: row.get("hidden")?,
        cover_path: row.get("cover_path")?,
        icon_path: row.get("icon_path")?,
        extra: serde_json::from_str(&extra).unwrap_or_else(|_| serde_json::json!({})),
    })
}

fn action_json(action: Option<&PlayAction>) -> Result<Option<String>> {
    Ok(action.map(serde_json::to_string).transpose()?)
}

impl Database {
    /// Inserts a new game and returns it.
    pub fn insert_game(&self, record: &NewGameRecord, now: DateTime<Utc>) -> Result<Game> {
        let id = Uuid::new_v4();
        self.conn().execute(
            "INSERT INTO games (id, name, sorting_name, search_key, source, source_game_id, \
             install_dir, play_action, date_added, cover_path) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id.to_string(),
                record.name,
                sorting_name(&record.name),
                search_key(&record.name),
                record.source,
                record.source_game_id,
                record.install_dir,
                action_json(record.play_action.as_ref())?,
                now,
                record.cover_path,
            ],
        )?;
        self.get_game(id)
    }

    pub fn get_game(&self, id: Uuid) -> Result<Game> {
        self.find_game(id)?.ok_or(CoreError::GameNotFound(id))
    }

    pub fn find_game(&self, id: Uuid) -> Result<Option<Game>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM games WHERE id = ?1"),
                [id.to_string()],
                row_to_game,
            )
            .optional()?)
    }

    pub fn count_games(&self) -> Result<u64> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or_default())
    }

    /// Lists games matching `query`, sorted as requested.
    pub fn list_games(&self, query: &GameQuery) -> Result<Vec<Game>> {
        let mut clauses: Vec<String> = Vec::new();
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(search) = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            args.push(Box::new(search_key(search)));
            clauses.push(format!("instr(search_key, ?{}) > 0", args.len()));
        }
        if !query.sources.is_empty() {
            let mut placeholders = Vec::with_capacity(query.sources.len());
            for source in &query.sources {
                args.push(Box::new(source.clone()));
                placeholders.push(format!("?{}", args.len()));
            }
            clauses.push(format!("source IN ({})", placeholders.join(", ")));
        }
        if query.favorites_only {
            clauses.push("favorite = 1".into());
        }
        match query.hidden {
            HiddenFilter::Visible => clauses.push("hidden = 0".into()),
            HiddenFilter::Hidden => clauses.push("hidden = 1".into()),
            HiddenFilter::All => {}
        }
        if query.installed_only {
            clauses.push("installed = 1".into());
        }

        let dir = if query.descending { "DESC" } else { "ASC" };
        let by_name = "sorting_name COLLATE NOCASE ASC, id ASC";
        let order = match query.sort {
            SortField::Name => format!("sorting_name COLLATE NOCASE {dir}, id {dir}"),
            // Never-played games go last in both directions.
            SortField::LastPlayed => {
                format!("last_played IS NULL ASC, last_played {dir}, {by_name}")
            }
            SortField::Playtime => format!("playtime_seconds {dir}, {by_name}"),
            SortField::DateAdded => format!("date_added {dir}, {by_name}"),
        };
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        let sql = format!("SELECT {COLUMNS} FROM games {where_sql} ORDER BY {order}");

        let conn = self.conn();
        let mut stmt = conn.prepare_cached(&sql)?;
        let params: Vec<&dyn ToSql> = args.iter().map(|a| a.as_ref()).collect();
        let games = stmt
            .query_map(params.as_slice(), row_to_game)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(games)
    }

    /// Applies a partial update and returns the updated game.
    pub fn update_game(&self, id: Uuid, update: &GameUpdate) -> Result<Game> {
        let current = self.get_game(id)?;
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            if let Some(name) = update.name.as_deref().map(str::trim) {
                if name.is_empty() {
                    return Err(CoreError::Invalid("The name must not be empty.".into()));
                }
                if name != current.name {
                    let mut extra = current.extra.clone();
                    if let Some(obj) = extra.as_object_mut() {
                        obj.insert(USER_RENAMED.into(), serde_json::Value::Bool(true));
                    }
                    tx.execute(
                        "UPDATE games SET name = ?2, sorting_name = ?3, search_key = ?4, extra = ?5 \
                         WHERE id = ?1",
                        params![
                            id.to_string(),
                            name,
                            sorting_name(name),
                            search_key(name),
                            extra.to_string()
                        ],
                    )?;
                }
            }
            if let Some(favorite) = update.favorite {
                tx.execute(
                    "UPDATE games SET favorite = ?2 WHERE id = ?1",
                    params![id.to_string(), favorite],
                )?;
            }
            if let Some(hidden) = update.hidden {
                tx.execute(
                    "UPDATE games SET hidden = ?2 WHERE id = ?1",
                    params![id.to_string(), hidden],
                )?;
            }
            if let Some(action) = &update.play_action {
                if action.target.trim().is_empty() {
                    return Err(CoreError::Invalid(
                        "The launch target must not be empty.".into(),
                    ));
                }
                tx.execute(
                    "UPDATE games SET play_action = ?2 WHERE id = ?1",
                    params![id.to_string(), action_json(Some(action))?],
                )?;
            }
            tx.commit()?;
        }
        self.get_game(id)
    }

    /// Sets (or clears) the cover path, stored relative to the data dir.
    pub fn set_cover_path(&self, id: Uuid, relative: Option<&str>) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE games SET cover_path = ?2 WHERE id = ?1",
            params![id.to_string(), relative],
        )?;
        if changed == 0 {
            return Err(CoreError::GameNotFound(id));
        }
        Ok(())
    }

    pub fn delete_game(&self, id: Uuid) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM games WHERE id = ?1", [id.to_string()])?;
        if changed == 0 {
            return Err(CoreError::GameNotFound(id));
        }
        Ok(())
    }

    /// Merges a provider scan into the library, keyed by `(source, source_game_id)`.
    ///
    /// - unknown games are inserted;
    /// - known games get store-owned fields refreshed (install dir, action, installed flag, and
    ///   the name unless the user renamed it); user data (playtime, favorite, ...) is untouched;
    /// - known games missing from the scan are marked as not installed, never deleted.
    pub fn upsert_imported(
        &self,
        source: &str,
        games: &[ImportedGame],
        now: DateTime<Utc>,
    ) -> Result<ScanSummary> {
        struct Existing {
            id: String,
            name: String,
            installed: bool,
            install_dir: Option<String>,
            play_action: Option<String>,
            user_renamed: bool,
        }

        let mut summary = ScanSummary::default();
        let mut conn = self.conn();
        let tx = conn.transaction()?;

        let mut existing: HashMap<String, Existing> = HashMap::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, name, installed, install_dir, play_action, extra, source_game_id \
                 FROM games WHERE source = ?1 AND source_game_id IS NOT NULL",
            )?;
            let rows = stmt.query_map([source], |r| {
                let extra: String = r.get(5)?;
                let user_renamed = serde_json::from_str::<serde_json::Value>(&extra)
                    .ok()
                    .and_then(|v| v.get(USER_RENAMED).and_then(serde_json::Value::as_bool))
                    .unwrap_or(false);
                Ok((
                    r.get::<_, String>(6)?,
                    Existing {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        installed: r.get(2)?,
                        install_dir: r.get(3)?,
                        play_action: r.get(4)?,
                        user_renamed,
                    },
                ))
            })?;
            for row in rows {
                let (key, value) = row?;
                existing.insert(key, value);
            }
        }

        let mut seen: HashSet<&str> = HashSet::new();
        for game in games {
            if !seen.insert(game.source_game_id.as_str()) {
                continue; // duplicate within one scan: first one wins
            }
            let action = action_json(game.play_action.as_ref())?;
            match existing.get(&game.source_game_id) {
                Some(row) => {
                    let name_changed = !row.user_renamed && row.name != game.name;
                    let changed = name_changed
                        || !row.installed
                        || row.install_dir != game.install_dir
                        || row.play_action != action;
                    if changed {
                        let name = if row.user_renamed {
                            &row.name
                        } else {
                            &game.name
                        };
                        tx.execute(
                            "UPDATE games SET name = ?2, sorting_name = ?3, search_key = ?4, \
                             installed = 1, install_dir = ?5, play_action = ?6 WHERE id = ?1",
                            params![
                                row.id,
                                name,
                                sorting_name(name),
                                search_key(name),
                                game.install_dir,
                                action
                            ],
                        )?;
                        summary.updated += 1;
                        if let Ok(id) = Uuid::parse_str(&row.id) {
                            summary.changed_ids.push(id);
                        }
                    }
                }
                None => {
                    let id = Uuid::new_v4();
                    tx.execute(
                        "INSERT INTO games (id, name, sorting_name, search_key, source, \
                         source_game_id, install_dir, play_action, date_added) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![
                            id.to_string(),
                            game.name,
                            sorting_name(&game.name),
                            search_key(&game.name),
                            source,
                            game.source_game_id,
                            game.install_dir,
                            action,
                            now
                        ],
                    )?;
                    summary.added += 1;
                    summary.changed_ids.push(id);
                }
            }
        }

        for (key, row) in &existing {
            if row.installed && !seen.contains(key.as_str()) {
                tx.execute("UPDATE games SET installed = 0 WHERE id = ?1", [&row.id])?;
                summary.uninstalled += 1;
                if let Ok(id) = Uuid::parse_str(&row.id) {
                    summary.changed_ids.push(id);
                }
            }
        }

        tx.commit()?;
        Ok(summary)
    }

    /// Bulk insert for the dev seed; one transaction for speed.
    pub fn insert_many(&self, records: &[NewGameRecord], now: DateTime<Utc>) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO games (id, name, sorting_name, search_key, source, source_game_id, \
                 install_dir, play_action, date_added, playtime_seconds, last_played, favorite) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for (i, r) in records.iter().enumerate() {
                // Deterministic variety so sorting by playtime / last played is meaningful.
                let i = i as i64;
                let playtime = (i * 7919) % 360_000;
                let last_played = (i % 3 != 0).then(|| now - chrono::Duration::hours(i % 5000));
                stmt.execute(params![
                    Uuid::new_v4().to_string(),
                    r.name,
                    sorting_name(&r.name),
                    search_key(&r.name),
                    r.source,
                    r.source_game_id,
                    r.install_dir,
                    action_json(r.play_action.as_ref())?,
                    now - chrono::Duration::minutes(i),
                    playtime,
                    last_played,
                    i % 17 == 0,
                ])?;
            }
        }
        tx.commit()?;
        Ok(records.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ActionKind;

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn manual(name: &str) -> NewGameRecord {
        NewGameRecord {
            name: name.into(),
            source: "manual".into(),
            play_action: Some(PlayAction::file(format!("C:/Games/{name}.exe"))),
            ..Default::default()
        }
    }

    fn imported(id: &str, name: &str) -> ImportedGame {
        ImportedGame {
            source_game_id: id.into(),
            name: name.into(),
            install_dir: Some(format!("/steam/common/{name}")),
            play_action: Some(PlayAction::uri(format!("steam://rungameid/{id}"))),
        }
    }

    fn names(games: &[Game]) -> Vec<&str> {
        games.iter().map(|g| g.name.as_str()).collect()
    }

    #[test]
    fn insert_and_get_roundtrip() {
        let db = db();
        let now = Utc::now();
        let game = db.insert_game(&manual("The Witness"), now).unwrap();
        assert_eq!(game.sorting_name, "Witness");
        assert_eq!(game.source, "manual");
        assert!(game.installed);
        assert_eq!(game.playtime_seconds, 0);
        assert_eq!(game.play_action.as_ref().unwrap().kind, ActionKind::File);
        assert_eq!(db.get_game(game.id).unwrap(), game);
    }

    #[test]
    fn get_missing_game_is_not_found() {
        let err = db().get_game(Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, CoreError::GameNotFound(_)));
    }

    #[test]
    fn search_is_case_insensitive_substring_including_unicode() {
        let db = db();
        let now = Utc::now();
        for n in ["Ökoland", "Portal 2", "Hollow Knight", "PORTAL"] {
            db.insert_game(&manual(n), now).unwrap();
        }
        let q = |s: &str| GameQuery {
            search: Some(s.into()),
            ..Default::default()
        };
        assert_eq!(
            names(&db.list_games(&q("portal")).unwrap()),
            ["PORTAL", "Portal 2"]
        );
        assert_eq!(names(&db.list_games(&q("ÖKO")).unwrap()), ["Ökoland"]);
        assert_eq!(names(&db.list_games(&q("  ")).unwrap()).len(), 4);
        assert!(db.list_games(&q("zzz")).unwrap().is_empty());
    }

    #[test]
    fn sorts_by_name_using_sorting_name() {
        let db = db();
        let now = Utc::now();
        for n in ["The Zoo", "Game 10", "Game 9", "apple"] {
            db.insert_game(&manual(n), now).unwrap();
        }
        let asc = db.list_games(&GameQuery::default()).unwrap();
        assert_eq!(names(&asc), ["apple", "Game 9", "Game 10", "The Zoo"]);
        let desc = db
            .list_games(&GameQuery {
                descending: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(names(&desc), ["The Zoo", "Game 10", "Game 9", "apple"]);
    }

    #[test]
    fn sorts_by_last_played_with_never_played_last() {
        let db = db();
        let now = Utc::now();
        let a = db.insert_game(&manual("A"), now).unwrap();
        let b = db.insert_game(&manual("B"), now).unwrap();
        db.insert_game(&manual("C"), now).unwrap();
        let s1 = db.begin_session(a.id, now).unwrap();
        db.finish_session(s1, now + chrono::Duration::hours(1), 60)
            .unwrap();
        let s2 = db.begin_session(b.id, now).unwrap();
        db.finish_session(s2, now + chrono::Duration::hours(2), 30)
            .unwrap();

        let query = |descending| GameQuery {
            sort: SortField::LastPlayed,
            descending,
            ..Default::default()
        };
        assert_eq!(
            names(&db.list_games(&query(true)).unwrap()),
            ["B", "A", "C"]
        );
        assert_eq!(
            names(&db.list_games(&query(false)).unwrap()),
            ["A", "B", "C"]
        );

        let by_playtime = db
            .list_games(&GameQuery {
                sort: SortField::Playtime,
                descending: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(names(&by_playtime), ["A", "B", "C"]);
    }

    #[test]
    fn sorts_by_date_added() {
        let db = db();
        let now = Utc::now();
        db.insert_game(&manual("Old"), now - chrono::Duration::days(2))
            .unwrap();
        db.insert_game(&manual("New"), now).unwrap();
        let q = GameQuery {
            sort: SortField::DateAdded,
            descending: true,
            ..Default::default()
        };
        assert_eq!(names(&db.list_games(&q).unwrap()), ["New", "Old"]);
    }

    #[test]
    fn filters_by_source_favorite_hidden_installed() {
        let db = db();
        let now = Utc::now();
        let fav = db.insert_game(&manual("Fav"), now).unwrap();
        let hid = db.insert_game(&manual("Hid"), now).unwrap();
        db.insert_game(&manual("Plain"), now).unwrap();
        db.upsert_imported("steam", &[imported("1", "SteamGame")], now)
            .unwrap();
        db.update_game(
            fav.id,
            &GameUpdate {
                favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        db.update_game(
            hid.id,
            &GameUpdate {
                hidden: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        let all_visible = db.list_games(&GameQuery::default()).unwrap();
        assert_eq!(names(&all_visible), ["Fav", "Plain", "SteamGame"]);

        let steam = db
            .list_games(&GameQuery {
                sources: vec!["steam".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(names(&steam), ["SteamGame"]);

        let favs = db
            .list_games(&GameQuery {
                favorites_only: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(names(&favs), ["Fav"]);

        let hidden = db
            .list_games(&GameQuery {
                hidden: HiddenFilter::Hidden,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(names(&hidden), ["Hid"]);

        let everything = db
            .list_games(&GameQuery {
                hidden: HiddenFilter::All,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(everything.len(), 4);

        db.upsert_imported("steam", &[], now).unwrap();
        let installed = db
            .list_games(&GameQuery {
                installed_only: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(names(&installed), ["Fav", "Plain"]);
    }

    #[test]
    fn rename_updates_sorting_and_search() {
        let db = db();
        let g = db.insert_game(&manual("Foo"), Utc::now()).unwrap();
        let g = db
            .update_game(
                g.id,
                &GameUpdate {
                    name: Some("  The Bar 3 ".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(g.name, "The Bar 3");
        assert_eq!(g.sorting_name, "Bar 03");
        let found = db
            .list_games(&GameQuery {
                search: Some("bar".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.len(), 1);
        let empty = db.update_game(
            g.id,
            &GameUpdate {
                name: Some(" ".into()),
                ..Default::default()
            },
        );
        assert!(matches!(empty, Err(CoreError::Invalid(_))));
    }

    #[test]
    fn rescan_updates_without_duplicates_and_keeps_user_data() {
        let db = db();
        let now = Utc::now();
        let first = db
            .upsert_imported(
                "steam",
                &[imported("10", "Alpha"), imported("20", "Beta")],
                now,
            )
            .unwrap();
        assert_eq!((first.added, first.updated, first.uninstalled), (2, 0, 0));

        // User data on Alpha.
        let alpha = db
            .list_games(&GameQuery {
                search: Some("alpha".into()),
                ..Default::default()
            })
            .unwrap()
            .remove(0);
        db.update_game(
            alpha.id,
            &GameUpdate {
                favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let s = db.begin_session(alpha.id, now).unwrap();
        db.finish_session(s, now, 120).unwrap();

        // Identical rescan: nothing changes.
        let same = db
            .upsert_imported(
                "steam",
                &[imported("10", "Alpha"), imported("20", "Beta")],
                now,
            )
            .unwrap();
        assert_eq!((same.added, same.updated, same.uninstalled), (0, 0, 0));
        assert!(same.changed_ids.is_empty());

        // Rescan with a moved install dir, Beta gone, Gamma new, and a duplicate entry.
        let mut moved = imported("10", "Alpha");
        moved.install_dir = Some("/other/Alpha".into());
        let next = db
            .upsert_imported(
                "steam",
                &[moved.clone(), imported("30", "Gamma"), moved],
                now,
            )
            .unwrap();
        assert_eq!((next.added, next.updated, next.uninstalled), (1, 1, 1));
        assert_eq!(db.count_games().unwrap(), 3);

        let alpha = db.get_game(alpha.id).unwrap();
        assert_eq!(alpha.install_dir.as_deref(), Some("/other/Alpha"));
        assert!(alpha.favorite);
        assert_eq!(alpha.playtime_seconds, 120);

        let all = db
            .list_games(&GameQuery {
                hidden: HiddenFilter::All,
                ..Default::default()
            })
            .unwrap();
        let beta = all.iter().find(|g| g.name == "Beta").unwrap();
        assert!(!beta.installed);

        // Beta reinstalled: flips back to installed, still one row.
        let back = db
            .upsert_imported(
                "steam",
                &[imported("10", "Alpha"), imported("20", "Beta")],
                now,
            )
            .unwrap();
        assert_eq!(back.added, 0);
        assert!(db.get_game(beta.id).unwrap().installed);
    }

    #[test]
    fn rescan_keeps_user_renamed_names() {
        let db = db();
        let now = Utc::now();
        db.upsert_imported("steam", &[imported("1", "Store Name")], now)
            .unwrap();
        let g = db.list_games(&GameQuery::default()).unwrap().remove(0);
        db.update_game(
            g.id,
            &GameUpdate {
                name: Some("My Name".into()),
                ..Default::default()
            },
        )
        .unwrap();
        db.upsert_imported("steam", &[imported("1", "Store Name v2")], now)
            .unwrap();
        assert_eq!(db.get_game(g.id).unwrap().name, "My Name");
    }

    #[test]
    fn sources_are_independent() {
        let db = db();
        let now = Utc::now();
        db.upsert_imported("steam", &[imported("1", "A")], now)
            .unwrap();
        db.upsert_imported("epic", &[imported("1", "A")], now)
            .unwrap();
        assert_eq!(db.count_games().unwrap(), 2);
        // An empty epic scan does not touch steam games.
        let s = db.upsert_imported("epic", &[], now).unwrap();
        assert_eq!(s.uninstalled, 1);
        let steam = db
            .list_games(&GameQuery {
                sources: vec!["steam".into()],
                installed_only: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(steam.len(), 1);
    }

    #[test]
    fn delete_removes_game_and_sessions() {
        let db = db();
        let now = Utc::now();
        let g = db.insert_game(&manual("Gone"), now).unwrap();
        db.begin_session(g.id, now).unwrap();
        db.delete_game(g.id).unwrap();
        assert!(db.find_game(g.id).unwrap().is_none());
        assert!(db.open_sessions().unwrap().is_empty());
        assert!(matches!(
            db.delete_game(g.id),
            Err(CoreError::GameNotFound(_))
        ));
    }

    #[test]
    fn malformed_action_json_does_not_break_listing() {
        let db = db();
        let g = db.insert_game(&manual("X"), Utc::now()).unwrap();
        db.conn()
            .execute(
                "UPDATE games SET play_action = '{broken', extra = 'nope' WHERE id = ?1",
                [g.id.to_string()],
            )
            .unwrap();
        let g = db.get_game(g.id).unwrap();
        assert!(g.play_action.is_none());
        assert_eq!(g.extra, serde_json::json!({}));
    }
}
