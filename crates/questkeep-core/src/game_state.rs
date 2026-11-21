//! Bringing a game's derived data up to date.
//!
//! Some files are made from a game's other data, e.g. the cover tile the tray popup reads
//! instead of the full cover. Schema migrations (`db/migrations.rs`) change the database for every
//! row at once and at open; changes to derived data can take long per game, so they run here, in
//! the background after start: `games.state_version` says how far a game is, and [`migrate`]
//! runs the steps after it for every game below [`CURRENT_VERSION`]. A game whose step failed
//! keeps its version and is tried again at the next start.
//!
//! To add a step: raise [`CURRENT_VERSION`] and add its arm to [`run_step`]. A step must be safe to
//! run on a game that already has what it makes (new games start at 0 as well), and files that are
//! written when a game's data changes (covers) must keep being written by that code path too.

use std::time::Instant;

use crate::db::{Database, StateRow};
use crate::error::Result;
use crate::images::ImageCache;

/// The state version of a game that is fully up to date.
pub const CURRENT_VERSION: u32 = 1;

/// What [`migrate`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// Games that were below [`CURRENT_VERSION`].
    pub checked: usize,
    /// Games that now have a derived file they did not have before.
    pub changed: usize,
    /// Games a step failed for (tried again at the next start).
    pub failed: usize,
}

/// After this many games got a new file, `notify` is called once, and again at the end: the
/// games played last come first, so by then what the tray shows is done.
const NOTIFY_AFTER: usize = 16;

/// Brings every game below [`CURRENT_VERSION`] up to date, one game at a time, the most recently
/// played first. `notify` is told when new files exist worth showing.
pub fn migrate(db: &Database, images: &ImageCache, notify: impl Fn()) -> Result<Report> {
    let rows = db.games_below_state(CURRENT_VERSION)?;
    let mut report = Report {
        checked: rows.len(),
        ..Report::default()
    };
    if rows.is_empty() {
        return Ok(report);
    }
    let started = Instant::now();
    for row in rows {
        let mut changed = false;
        let mut failed = false;
        for step in row.state_version + 1..=CURRENT_VERSION {
            match run_step(step, images, &row) {
                Ok(made) => changed |= made,
                Err(err) => {
                    tracing::warn!(%err, game = %row.id, step, "game state step failed");
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            report.failed += 1;
            continue;
        }
        db.set_state_version(row.id, CURRENT_VERSION)?;
        report.changed += usize::from(changed);
        if changed && report.changed == NOTIFY_AFTER {
            notify();
        }
    }
    if report.changed > 0 {
        notify();
    }
    tracing::info!(
        checked = report.checked,
        changed = report.changed,
        failed = report.failed,
        ms = started.elapsed().as_millis() as u64,
        "game states brought up to date"
    );
    Ok(report)
}

/// Runs one step for one game; returns whether it made a file.
fn run_step(step: u32, images: &ImageCache, row: &StateRow) -> Result<bool> {
    match step {
        // 1: the cover tile for the tray popup.
        1 => {
            let Some(cover) = row.cover_path.as_deref() else {
                return Ok(false);
            };
            if images.tile_path(row.id).is_some() || images.resolve(cover).is_none() {
                return Ok(false);
            }
            images.refresh_tile(row.id, cover)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use uuid::Uuid;

    use super::*;
    use crate::db::NewGameRecord;
    use crate::paths::DataPaths;

    fn game(db: &Database, name: &str) -> Uuid {
        db.insert_game(
            &NewGameRecord {
                name: name.into(),
                source: "manual".into(),
                ..Default::default()
            },
            chrono::Utc::now(),
        )
        .unwrap()
        .id
    }

    #[test]
    fn catches_up_games_once_and_keeps_failed_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let images = ImageCache::new(paths.clone());
        let db = Arc::new(Database::open_in_memory().unwrap());
        let with_cover = game(&db, "with cover");
        let broken_cover = game(&db, "broken cover");
        let no_cover = game(&db, "no cover");

        let png = tmp.path().join("cover.png");
        image::RgbaImage::from_pixel(120, 80, image::Rgba([200, 30, 30, 255]))
            .save(&png)
            .unwrap();
        // Written by hand, so no tile is made when the cover is stored.
        for (id, bytes) in [
            (with_cover, std::fs::read(&png).unwrap()),
            (broken_cover, b"not an image".to_vec()),
        ] {
            let relative = format!("images/covers/{id}.png");
            std::fs::create_dir_all(paths.absolute("images/covers")).unwrap();
            std::fs::write(paths.absolute(&relative), bytes).unwrap();
            db.set_cover_path(id, Some(&relative)).unwrap();
        }
        assert!(images.tile_path(with_cover).is_none());

        let notified = std::cell::Cell::new(0);
        let report = migrate(&db, &images, || notified.set(notified.get() + 1)).unwrap();
        assert_eq!(notified.get(), 1, "told once at the end");
        assert_eq!(
            report,
            Report {
                checked: 3,
                changed: 1,
                failed: 0
            }
        );
        assert!(images.tile_path(with_cover).is_some());
        assert!(images.tile_path(broken_cover).is_none());
        assert!(images.tile_path(no_cover).is_none());
        // Everything is at the current version now: nothing left to do.
        assert!(db.games_below_state(CURRENT_VERSION).unwrap().is_empty());
        assert_eq!(migrate(&db, &images, || {}).unwrap(), Report::default());
    }
}
