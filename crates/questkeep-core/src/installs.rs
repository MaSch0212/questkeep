//! Installs handed to a store client, watched until the game is fully installed. The library
//! polls only the stores with a pending watch (`Library::poll_installs`), then syncs that store
//! and announces the game (`CoreEvent::InstallFinished`).

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long an install is watched: big downloads take hours, but not days.
pub const WATCH_FOR: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone)]
struct Watch {
    source: String,
    source_game_id: String,
    since: Instant,
}

/// The pending watches (one per store game).
#[derive(Debug, Default)]
pub struct InstallWatches {
    pending: Mutex<Vec<Watch>>,
}

impl InstallWatches {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Watch>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Watches a store game (again from now, if it already was).
    pub fn add(&self, source: &str, source_game_id: &str, now: Instant) {
        let mut pending = self.lock();
        pending.retain(|w| !(w.source == source && w.source_game_id == source_game_id));
        pending.push(Watch {
            source: source.to_string(),
            source_game_id: source_game_id.to_string(),
            since: now,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Drops watches older than [`WATCH_FOR`], then lists the rest per store.
    pub fn due(&self, now: Instant) -> BTreeMap<String, Vec<String>> {
        let mut pending = self.lock();
        pending.retain(|w| now.duration_since(w.since) < WATCH_FOR);
        let mut by_store: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for watch in pending.iter() {
            by_store
                .entry(watch.source.clone())
                .or_default()
                .push(watch.source_game_id.clone());
        }
        by_store
    }

    /// Stops watching a store game.
    pub fn finish(&self, source: &str, source_game_id: &str) {
        self.lock()
            .retain(|w| !(w.source == source && w.source_game_id == source_game_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_per_store_without_duplicates() {
        let watches = InstallWatches::default();
        let now = Instant::now();
        watches.add("steam", "440", now);
        watches.add("steam", "440", now);
        watches.add("steam", "620", now);
        watches.add("gog", "1", now);
        let due = watches.due(now);
        assert_eq!(due["steam"], vec!["440", "620"]);
        assert_eq!(due["gog"], vec!["1"]);
        watches.finish("steam", "440");
        assert_eq!(watches.due(now)["steam"], vec!["620"]);
    }

    #[test]
    fn old_watches_expire() {
        let watches = InstallWatches::default();
        let start = Instant::now();
        watches.add("steam", "440", start);
        assert!(watches.due(start + WATCH_FOR).is_empty());
        assert!(watches.is_empty());
    }
}
