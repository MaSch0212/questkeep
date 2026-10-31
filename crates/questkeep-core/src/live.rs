//! Live data for the game page: player counts, news and achievements, asked from the stores only
//! while a game's page is open and cached so reopening it doesn't ask again.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Achievements, Game, GameNews};
use crate::providers::ProviderRegistry;
use crate::settings::SettingsStore;

const PLAYERS_TTL: Duration = Duration::from_secs(10 * 60);
const NEWS_TTL: Duration = Duration::from_secs(30 * 60);
/// Achievements are fetched again after this, or after the game was played since.
const ACHIEVEMENTS_TTL: chrono::Duration = chrono::Duration::hours(1);

/// Answers kept in memory for a while, by key.
struct TtlCache<T> {
    ttl: Duration,
    entries: Mutex<HashMap<String, (Instant, T)>>,
}

impl<T: Clone> TtlCache<T> {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, key: &str, now: Instant) -> Option<T> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(key)
            .filter(|(at, _)| now.duration_since(*at) < self.ttl)
            .map(|(_, value)| value.clone())
    }

    fn put(&self, key: &str, value: T, now: Instant) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|_, (at, _)| now.duration_since(*at) < self.ttl);
        entries.insert(key.to_string(), (now, value));
    }
}

/// A game's achievements as cached on disk.
#[derive(Serialize, Deserialize)]
struct CachedAchievements {
    fetched_at: DateTime<Utc>,
    achievements: Option<Achievements>,
}

/// Asks the stores of a game's copies for live data and caches the answers.
pub struct LiveData {
    providers: ProviderRegistry,
    settings: SettingsStore,
    cache_dir: PathBuf,
    players: TtlCache<Option<u32>>,
    news: TtlCache<Option<GameNews>>,
}

impl LiveData {
    pub fn new(providers: ProviderRegistry, settings: SettingsStore, cache_dir: PathBuf) -> Self {
        Self {
            providers,
            settings,
            cache_dir,
            players: TtlCache::new(PLAYERS_TTL),
            news: TtlCache::new(NEWS_TTL),
        }
    }

    /// `(store, store game id)` of each copy a registered store knows, in play order.
    fn copies<'a>(&self, game: &'a Game) -> Vec<(&'a str, &'a str)> {
        let mut out: Vec<(&str, &str)> = game
            .copies
            .iter()
            .filter_map(|c| Some((c.source.as_str(), c.source_game_id.as_deref()?)))
            .collect();
        if out.is_empty()
            && let Some(id) = game.source_game_id.as_deref()
        {
            out.push((game.source.as_str(), id));
        }
        out.retain(|(source, _)| self.providers.get(source).is_some());
        out
    }

    /// The first answer a copy's store gives; failures are logged and the next copy is asked.
    fn first<T>(
        &self,
        game: &Game,
        what: &str,
        ask: impl Fn(&str, &str) -> Result<Option<T>, questkeep_store_api::ProviderError>,
    ) -> Option<T> {
        for (source, id) in self.copies(game) {
            match ask(source, id) {
                Ok(Some(value)) => return Some(value),
                Ok(None) => {}
                Err(err) => tracing::info!(%err, source, game = %game.id, "no {what}"),
            }
        }
        None
    }

    /// How many people play the game right now, if a store can tell.
    pub fn player_count(&self, game: &Game) -> Option<u32> {
        let key = game.id.to_string();
        let now = Instant::now();
        if let Some(cached) = self.players.get(&key, now) {
            return cached;
        }
        let count = self.first(game, "player count", |source, id| {
            self.providers
                .get(source)
                .map_or(Ok(None), |p| p.player_count(id))
        });
        self.players.put(&key, count, now);
        count
    }

    /// The game's latest news, if a store has a feed.
    pub fn news(&self, game: &Game) -> Option<GameNews> {
        let key = game.id.to_string();
        let now = Instant::now();
        if let Some(cached) = self.news.get(&key, now) {
            return cached;
        }
        let news = self
            .first(game, "news", |source, id| {
                self.providers.get(source).map_or(Ok(None), |p| p.news(id))
            })
            .filter(|n| !n.items.is_empty());
        self.news.put(&key, news.clone(), now);
        news
    }

    fn achievements_file(&self, id: Uuid) -> PathBuf {
        self.cache_dir
            .join("achievements")
            .join(format!("{id}.json"))
    }

    fn cached_achievements(&self, id: Uuid) -> Option<CachedAchievements> {
        let text = std::fs::read_to_string(self.achievements_file(id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The user's achievements in the game. Cached on disk; asked again when the cache is older
    /// than an hour or the game was played after it was fetched.
    pub fn achievements(&self, game: &Game) -> Option<Achievements> {
        let now = Utc::now();
        if let Some(cached) = self.cached_achievements(game.id) {
            let played_since = game.last_played.is_some_and(|p| p > cached.fetched_at);
            if now - cached.fetched_at < ACHIEVEMENTS_TTL && !played_since {
                return cached.achievements;
            }
        }
        let settings = self.settings.load();
        let mut failed = false;
        let mut found = None;
        for (source, id) in self.copies(game) {
            let Some(provider) = self.providers.get(source) else {
                continue;
            };
            match provider.achievements(settings.provider_config(source), id) {
                Ok(Some(a)) if a.total > 0 => {
                    found = Some(a);
                    break;
                }
                Ok(_) => {}
                Err(err) => {
                    failed = true;
                    tracing::info!(%err, source, game = %game.id, "no achievements");
                }
            }
        }
        // A failure (offline) keeps the old answer and is asked again next time.
        if found.is_none() && failed {
            return self
                .cached_achievements(game.id)
                .and_then(|c| c.achievements);
        }
        self.store_achievements(game.id, now, found.as_ref());
        found
    }

    fn store_achievements(&self, id: Uuid, at: DateTime<Utc>, achievements: Option<&Achievements>) {
        let path = self.achievements_file(id);
        let entry = CachedAchievements {
            fetched_at: at,
            achievements: achievements.cloned(),
        };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let json = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
                std::fs::write(&path, json)
            });
        if let Err(err) = written {
            tracing::warn!(%err, path = %path.display(), "could not cache achievements");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use questkeep_store_api::model::{Achievement, ImportedGame};
    use questkeep_store_api::{LibraryProvider, ProviderError};

    use super::*;
    use questkeep_store_api::auth::MemoryTokenStore;

    use crate::db::{Database, NewGameRecord};
    use crate::model::GameCopy;

    /// A store that answers live data for game id "yes" and counts its calls.
    struct Live {
        id: &'static str,
        calls: AtomicU32,
        fail: bool,
    }

    impl Live {
        fn new(id: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id,
                calls: AtomicU32::new(0),
                fail: false,
            })
        }
    }

    impl LibraryProvider for Live {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        fn icon_svg(&self) -> &'static str {
            ""
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(Vec::new())
        }
        fn player_count(&self, id: &str) -> Result<Option<u32>, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(ProviderError::Network("offline".into()));
            }
            Ok((id == "yes").then_some(42))
        }
        fn achievements(
            &self,
            _: &serde_json::Value,
            id: &str,
        ) -> Result<Option<Achievements>, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(ProviderError::Network("offline".into()));
            }
            Ok((id == "yes").then(|| Achievements {
                total: 3,
                unlocked: vec![Achievement {
                    name: "First".into(),
                    description: None,
                    icon: None,
                    unlocked_at: Some(1),
                    percent: Some(50.0),
                }],
            }))
        }
    }

    fn copy(source: &str, id: &str) -> GameCopy {
        GameCopy {
            id: Uuid::new_v4(),
            source: source.into(),
            source_game_id: Some(id.into()),
            installed: false,
            playtime_seconds: 0,
            last_played: None,
        }
    }

    fn game(copies: Vec<GameCopy>) -> Game {
        let db = Database::open_in_memory().unwrap();
        let record = NewGameRecord {
            name: "Test".into(),
            source: "manual".into(),
            ..Default::default()
        };
        let mut game = db.insert_game(&record, Utc::now()).unwrap();
        game.copies = copies;
        game
    }

    fn live(tmp: &std::path::Path, providers: Vec<Arc<dyn LibraryProvider>>) -> LiveData {
        let mut registry = ProviderRegistry::new();
        for p in providers {
            registry.register(p);
        }
        let settings = SettingsStore::new(
            tmp.join("settings.json"),
            Arc::new(MemoryTokenStore::default()),
            Vec::new(),
        );
        LiveData::new(registry, settings, tmp.join("cache"))
    }

    #[test]
    fn asks_copies_in_order_and_caches_the_answer() {
        let tmp = tempfile::tempdir().unwrap();
        let a = Live::new("a");
        let b = Live::new("b");
        let data = live(tmp.path(), vec![a.clone(), b.clone()]);
        let g = game(vec![
            copy("a", "no"),
            copy("unknown", "yes"),
            copy("b", "yes"),
        ]);
        assert_eq!(data.player_count(&g), Some(42));
        assert_eq!(data.player_count(&g), Some(42));
        assert_eq!(a.calls.load(Ordering::SeqCst), 1);
        assert_eq!(b.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn achievements_are_cached_on_disk_until_played_again() {
        let tmp = tempfile::tempdir().unwrap();
        let a = Live::new("a");
        let data = live(tmp.path(), vec![a.clone()]);
        let mut g = game(vec![copy("a", "yes")]);
        assert_eq!(data.achievements(&g).map(|x| x.total), Some(3));
        assert_eq!(data.achievements(&g).map(|x| x.total), Some(3));
        assert_eq!(a.calls.load(Ordering::SeqCst), 1);
        g.last_played = Some(Utc::now() + chrono::Duration::seconds(5));
        assert!(data.achievements(&g).is_some());
        assert_eq!(a.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_failure_keeps_the_cached_achievements() {
        let tmp = tempfile::tempdir().unwrap();
        let ok = Live::new("a");
        let g = game(vec![copy("a", "yes")]);
        assert!(live(tmp.path(), vec![ok]).achievements(&g).is_some());
        let failing = Arc::new(Live {
            id: "a",
            calls: AtomicU32::new(0),
            fail: true,
        });
        let data = live(tmp.path(), vec![failing]);
        // Make the cache stale.
        let file = data.achievements_file(g.id);
        let mut cached: CachedAchievements =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        cached.fetched_at -= chrono::Duration::hours(2);
        std::fs::write(&file, serde_json::to_string(&cached).unwrap()).unwrap();
        assert_eq!(data.achievements(&g).map(|x| x.total), Some(3));
    }
}
