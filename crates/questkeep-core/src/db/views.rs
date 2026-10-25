//! Saved library views and their sidebar sections, plus the last sync time of each store.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

use super::Database;
use crate::error::{CoreError, Result};
use crate::model::{
    GameQuery, GroupBy, LibraryView, NewView, SectionOrder, SyncTimes, ViewLayout, ViewSection,
    ViewUpdate,
};

/// Key of the "sync all" entry in `sync_times`; provider ids never contain a colon.
const SYNC_ALL: &str = ":all";

fn layout_text(layout: ViewLayout) -> &'static str {
    match layout {
        ViewLayout::Grid => "grid",
        ViewLayout::List => "list",
        ViewLayout::Details => "details",
    }
}

fn parse_layout(text: &str) -> ViewLayout {
    match text {
        "list" => ViewLayout::List,
        "details" => ViewLayout::Details,
        _ => ViewLayout::Grid,
    }
}

fn group_text(group: GroupBy) -> &'static str {
    match group {
        GroupBy::None => "none",
        GroupBy::Category => "category",
        GroupBy::Source => "source",
    }
}

fn parse_group(text: &str) -> GroupBy {
    match text {
        "category" => GroupBy::Category,
        "source" => GroupBy::Source,
        _ => GroupBy::None,
    }
}

/// The query as stored: without the keyword filter, which is never saved.
fn query_json(query: &GameQuery) -> Result<String> {
    Ok(serde_json::to_string(&GameQuery {
        search: None,
        ..query.clone()
    })?)
}

fn clean_name(name: &str, what: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::Invalid(format!(
            "The {what} name must not be empty."
        )));
    }
    Ok(name.to_string())
}

fn not_found(what: &str) -> CoreError {
    CoreError::Invalid(format!("That {what} no longer exists."))
}

const VIEW_COLUMNS: &str = "id, name, icon, query, layout, group_by";

fn row_to_view(row: &Row<'_>) -> rusqlite::Result<LibraryView> {
    let query: String = row.get(3)?;
    let layout: String = row.get(4)?;
    let group: String = row.get(5)?;
    Ok(LibraryView {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        // A view whose query can't be read shows everything rather than failing the sidebar.
        query: serde_json::from_str(&query).unwrap_or_else(|err| {
            tracing::warn!(%err, "unreadable view query");
            GameQuery::default()
        }),
        layout: parse_layout(&layout),
        group: parse_group(&group),
    })
}

fn count_views(tx: &Transaction<'_>) -> Result<i64> {
    Ok(tx.query_row("SELECT COUNT(*) FROM views", [], |r| r.get(0))?)
}

impl Database {
    /// All sections with their views, in sidebar order.
    pub fn list_views(&self) -> Result<Vec<ViewSection>> {
        let conn = self.conn();
        let mut sections: Vec<ViewSection> = conn
            .prepare_cached("SELECT id, name FROM view_sections ORDER BY position, rowid")?
            .query_map([], |r| {
                Ok(ViewSection {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    views: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {VIEW_COLUMNS} FROM views WHERE section_id = ?1 ORDER BY position, rowid"
        ))?;
        for section in &mut sections {
            section.views = stmt
                .query_map([&section.id], row_to_view)?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(sections)
    }

    pub fn get_view(&self, id: &str) -> Result<LibraryView> {
        self.conn()
            .query_row(
                &format!("SELECT {VIEW_COLUMNS} FROM views WHERE id = ?1"),
                [id],
                row_to_view,
            )
            .optional()?
            .ok_or_else(|| not_found("view"))
    }

    /// Adds a view at the end of its section.
    pub fn create_view(&self, view: &NewView) -> Result<LibraryView> {
        let name = clean_name(&view.name, "view")?;
        let id = Uuid::new_v4().to_string();
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let section_exists: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM view_sections WHERE id = ?1)",
                [&view.section_id],
                |r| r.get(0),
            )?;
            if !section_exists {
                return Err(not_found("section"));
            }
            tx.execute(
                "INSERT INTO views (id, section_id, position, name, icon, query, layout, group_by)
                 VALUES (?1, ?2,
                     (SELECT COALESCE(MAX(position) + 1, 0) FROM views WHERE section_id = ?2),
                     ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    view.section_id,
                    name,
                    view.icon,
                    query_json(&view.query)?,
                    layout_text(view.layout),
                    group_text(view.group)
                ],
            )?;
            tx.commit()?;
        }
        self.get_view(&id)
    }

    pub fn update_view(&self, id: &str, update: &ViewUpdate) -> Result<LibraryView> {
        let current = self.get_view(id)?;
        let name = match &update.name {
            Some(name) => clean_name(name, "view")?,
            None => current.name,
        };
        let icon = update.icon.clone().unwrap_or(current.icon);
        let query = update.query.as_ref().unwrap_or(&current.query);
        let layout = update.layout.unwrap_or(current.layout);
        let group = update.group.unwrap_or(current.group);
        self.conn().execute(
            "UPDATE views SET name = ?2, icon = ?3, query = ?4, layout = ?5, group_by = ?6 \
             WHERE id = ?1",
            params![
                id,
                name,
                icon,
                query_json(query)?,
                layout_text(layout),
                group_text(group)
            ],
        )?;
        self.get_view(id)
    }

    /// Copies a view right after the original, named "<name> (copy)".
    pub fn duplicate_view(&self, id: &str) -> Result<LibraryView> {
        let copy = Uuid::new_v4().to_string();
        {
            let mut conn = self.conn();
            let tx = conn.transaction()?;
            let (section, position): (String, i64) = tx
                .query_row(
                    "SELECT section_id, position FROM views WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| not_found("view"))?;
            tx.execute(
                "UPDATE views SET position = position + 1 WHERE section_id = ?1 AND position > ?2",
                params![section, position],
            )?;
            tx.execute(
                "INSERT INTO views (id, section_id, position, name, icon, query, layout, group_by)
                 SELECT ?2, section_id, position + 1, name || ' (copy)', icon, query, layout,
                     group_by
                 FROM views WHERE id = ?1",
                params![id, copy],
            )?;
            tx.commit()?;
        }
        self.get_view(&copy)
    }

    /// Deletes a view. The last view can't be deleted: the library always has one.
    pub fn delete_view(&self, id: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if count_views(&tx)? <= 1 {
            return Err(CoreError::Invalid("The last view can't be deleted.".into()));
        }
        if tx.execute("DELETE FROM views WHERE id = ?1", [id])? == 0 {
            return Err(not_found("view"));
        }
        tx.commit()?;
        Ok(())
    }

    /// Adds an empty section at the end of the sidebar.
    pub fn create_section(&self, name: &str) -> Result<ViewSection> {
        let name = clean_name(name, "section")?;
        let id = Uuid::new_v4().to_string();
        self.conn().execute(
            "INSERT INTO view_sections (id, position, name)
             VALUES (?1, (SELECT COALESCE(MAX(position) + 1, 0) FROM view_sections), ?2)",
            params![id, name],
        )?;
        Ok(ViewSection {
            id,
            name,
            views: Vec::new(),
        })
    }

    pub fn rename_section(&self, id: &str, name: &str) -> Result<()> {
        let name = clean_name(name, "section")?;
        let changed = self.conn().execute(
            "UPDATE view_sections SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        if changed == 0 {
            return Err(not_found("section"));
        }
        Ok(())
    }

    /// Deletes a section with its views, unless that would leave the library without a view.
    pub fn delete_section(&self, id: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let inside: i64 = tx.query_row(
            "SELECT COUNT(*) FROM views WHERE section_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        if inside > 0 && inside == count_views(&tx)? {
            return Err(CoreError::Invalid(
                "This section holds every view, and the library needs at least one.".into(),
            ));
        }
        if tx.execute("DELETE FROM view_sections WHERE id = ?1", [id])? == 0 {
            return Err(not_found("section"));
        }
        tx.commit()?;
        Ok(())
    }

    /// Applies a new order of sections and views (after a drag in the sidebar). The order must
    /// name every section and every view exactly once.
    pub fn arrange_views(&self, order: &[SectionOrder]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let existing = |sql: &str| -> Result<HashSet<String>> {
            Ok(tx
                .prepare(sql)?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        };
        let sections = existing("SELECT id FROM view_sections")?;
        let views = existing("SELECT id FROM views")?;
        let given_sections: HashSet<String> = order.iter().map(|s| s.id.clone()).collect();
        let given_views: Vec<&String> = order.iter().flat_map(|s| &s.views).collect();
        let unique_views: HashSet<String> = given_views.iter().map(|v| (*v).clone()).collect();
        if given_sections != sections
            || given_sections.len() != order.len()
            || unique_views != views
            || unique_views.len() != given_views.len()
        {
            return Err(CoreError::Invalid(
                "The views changed meanwhile; please try again.".into(),
            ));
        }
        for (s_pos, section) in order.iter().enumerate() {
            tx.execute(
                "UPDATE view_sections SET position = ?2 WHERE id = ?1",
                params![section.id, s_pos as i64],
            )?;
            for (v_pos, view) in section.views.iter().enumerate() {
                tx.execute(
                    "UPDATE views SET section_id = ?2, position = ?3 WHERE id = ?1",
                    params![view, section.id, v_pos as i64],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Remembers that a store (or, with `None`, a "sync all") finished syncing at `at`.
    pub fn record_sync(&self, provider: Option<&str>, at: DateTime<Utc>) -> Result<()> {
        self.conn().execute(
            "INSERT INTO sync_times (provider, finished_at) VALUES (?1, ?2)
             ON CONFLICT (provider) DO UPDATE SET finished_at = excluded.finished_at",
            params![provider.unwrap_or(SYNC_ALL), at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn sync_times(&self) -> Result<SyncTimes> {
        let conn = self.conn();
        let rows: Vec<(String, String)> = conn
            .prepare_cached("SELECT provider, finished_at FROM sync_times")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut times = SyncTimes {
            all: None,
            providers: BTreeMap::new(),
        };
        for (provider, at) in rows {
            if provider == SYNC_ALL {
                times.all = Some(at);
            } else {
                times.providers.insert(provider, at);
            }
        }
        Ok(times)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HiddenFilter, SortField, TriState};

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn ids(sections: &[ViewSection]) -> Vec<(String, Vec<String>)> {
        sections
            .iter()
            .map(|s| (s.id.clone(), s.views.iter().map(|v| v.id.clone()).collect()))
            .collect()
    }

    fn new_view(section: &str, name: &str) -> NewView {
        NewView {
            section_id: section.into(),
            name: name.into(),
            icon: "bookmark".into(),
            query: GameQuery::default(),
            layout: ViewLayout::List,
            group: GroupBy::Category,
        }
    }

    #[test]
    fn a_new_library_has_the_default_views() {
        let sections = db().list_views().unwrap();
        assert_eq!(
            ids(&sections),
            [
                (
                    "library".to_string(),
                    vec!["all", "favorites", "installed", "recent"]
                        .into_iter()
                        .map(String::from)
                        .collect::<Vec<_>>()
                ),
                ("other".to_string(), vec!["hidden".to_string()]),
            ]
        );
        let recent = &sections[0].views[3];
        assert_eq!(recent.query.sort, SortField::LastPlayed);
        assert!(recent.query.descending);
        assert_eq!(recent.layout, ViewLayout::List);
        assert_eq!(sections[1].views[0].query.hidden, HiddenFilter::Hidden);
        assert_eq!(sections[0].views[1].query.favorite, TriState::Yes);
    }

    #[test]
    fn creating_and_updating_never_saves_the_keyword() {
        let db = db();
        let mut view = new_view("other", "  Backlog ");
        view.query.search = Some("zelda".into());
        let created = db.create_view(&view).unwrap();
        assert_eq!(created.name, "Backlog");
        assert_eq!(created.query.search, None);
        assert_eq!(db.list_views().unwrap()[1].views[1].id, created.id);

        let updated = db
            .update_view(
                &created.id,
                &ViewUpdate {
                    query: Some(GameQuery {
                        search: Some("x".into()),
                        installed: TriState::No,
                        ..Default::default()
                    }),
                    icon: Some("star".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.query.search, None);
        assert_eq!(updated.query.installed, TriState::No);
        assert_eq!(updated.icon, "star");
        assert_eq!(updated.name, "Backlog");
        assert_eq!(updated.layout, ViewLayout::List);
        assert_eq!(updated.group, GroupBy::Category);

        let ungrouped = db
            .update_view(
                &created.id,
                &ViewUpdate {
                    group: Some(GroupBy::Source),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(ungrouped.group, GroupBy::Source);
        assert_eq!(
            db.duplicate_view(&created.id).unwrap().group,
            GroupBy::Source
        );
        assert_eq!(db.get_view("all").unwrap().group, GroupBy::None);
    }

    #[test]
    fn names_must_not_be_empty_and_sections_must_exist() {
        let db = db();
        assert!(db.create_view(&new_view("library", " ")).is_err());
        assert!(db.create_view(&new_view("nope", "X")).is_err());
        assert!(db.create_section("  ").is_err());
        assert!(db.rename_section("library", "").is_err());
        assert!(
            db.update_view(
                "all",
                &ViewUpdate {
                    name: Some(" ".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn duplicate_lands_right_after_the_original() {
        let db = db();
        let copy = db.duplicate_view("favorites").unwrap();
        assert_eq!(copy.name, "Favorites (copy)");
        assert_eq!(copy.query.favorite, TriState::Yes);
        let library = &db.list_views().unwrap()[0];
        let order: Vec<&str> = library.views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(
            order,
            ["all", "favorites", copy.id.as_str(), "installed", "recent"]
        );
    }

    #[test]
    fn the_last_view_cannot_be_deleted() {
        let db = db();
        db.delete_section("other").unwrap();
        for id in ["all", "favorites", "installed"] {
            db.delete_view(id).unwrap();
        }
        assert!(db.delete_view("recent").is_err());
        assert!(db.delete_section("library").is_err());
        assert!(db.delete_view("gone").is_err());
    }

    #[test]
    fn deleting_a_section_deletes_its_views() {
        let db = db();
        let section = db.create_section("Co-op").unwrap();
        db.create_view(&new_view(&section.id, "Party")).unwrap();
        db.delete_section(&section.id).unwrap();
        let sections = db.list_views().unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections.iter().map(|s| s.views.len()).sum::<usize>(), 5);
    }

    #[test]
    fn arrange_moves_views_between_sections_and_reorders_sections() {
        let db = db();
        db.rename_section("other", "Misc").unwrap();
        let order = [
            SectionOrder {
                id: "other".into(),
                views: vec!["hidden".into(), "recent".into()],
            },
            SectionOrder {
                id: "library".into(),
                views: vec!["installed".into(), "all".into(), "favorites".into()],
            },
        ];
        db.arrange_views(&order).unwrap();
        let sections = db.list_views().unwrap();
        assert_eq!(sections[0].name, "Misc");
        assert_eq!(
            ids(&sections),
            order
                .iter()
                .map(|s| (s.id.clone(), s.views.clone()))
                .collect::<Vec<_>>()
        );
        // A new view still goes to the end of its section.
        let added = db.create_view(&new_view("other", "New")).unwrap();
        assert_eq!(db.list_views().unwrap()[0].views[2].id, added.id);
    }

    #[test]
    fn arrange_rejects_an_incomplete_or_duplicated_order() {
        let db = db();
        let missing = [SectionOrder {
            id: "library".into(),
            views: vec!["all".into()],
        }];
        assert!(db.arrange_views(&missing).is_err());
        let duplicated = [
            SectionOrder {
                id: "library".into(),
                views: vec![
                    "all".into(),
                    "favorites".into(),
                    "installed".into(),
                    "recent".into(),
                    "all".into(),
                ],
            },
            SectionOrder {
                id: "other".into(),
                views: vec!["hidden".into()],
            },
        ];
        assert!(db.arrange_views(&duplicated).is_err());
        // Nothing moved.
        assert_eq!(db.list_views().unwrap()[1].views[0].id, "hidden");
    }

    #[test]
    fn sync_times_keep_the_latest_per_store() {
        let db = db();
        assert_eq!(db.sync_times().unwrap(), SyncTimes::default());
        let early = DateTime::parse_from_rfc3339("2026-01-01T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let late = DateTime::parse_from_rfc3339("2026-01-02T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        db.record_sync(Some("steam"), early).unwrap();
        db.record_sync(Some("steam"), late).unwrap();
        db.record_sync(None, late).unwrap();
        let times = db.sync_times().unwrap();
        assert_eq!(times.providers.len(), 1);
        assert_eq!(times.providers["steam"], late.to_rfc3339());
        assert_eq!(times.all, Some(late.to_rfc3339()));
    }
}
