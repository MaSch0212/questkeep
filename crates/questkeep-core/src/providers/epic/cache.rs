//! On-disk cache of Epic catalog answers (`bulk/items`), shared by the account and the
//! account-free lookup. One small JSON file per catalog item id under `<dir>/epic-catalog/`, so a
//! corrupt or half-written file only costs one item and entries are replaced independently.
//!
//! Fresh entries are used instead of a network request. A found item is kept for [`ITEM_TTL_SECS`]
//! (store listings change rarely); "Epic does not know this item" is kept for the shorter
//! [`UNKNOWN_TTL_SECS`] since unlisted items can appear later. Unreadable, expired, future-dated or
//! foreign-version files count as a miss and are overwritten by the next fetch. Writes go to a
//! temporary file that is renamed over the target, so readers never see a partial file. All I/O
//! errors are logged and otherwise ignored: the cache is an optimisation, never a failure.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::web::CatalogItem;
use crate::auth::Clock;

/// How long a found catalog item stays fresh: 7 days.
pub const ITEM_TTL_SECS: i64 = 7 * 24 * 3600;
/// How long "unknown item" stays fresh: 1 day.
pub const UNKNOWN_TTL_SECS: i64 = 24 * 3600;
/// Entries stamped further in the future than this (clock changes) are discarded.
const MAX_FUTURE_SKEW_SECS: i64 = 24 * 3600;
/// Bump when the stored `CatalogItem` layout changes incompatibly; older files are then ignored.
const FORMAT_VERSION: u32 = 1;
const SUBDIR: &str = "epic-catalog";
const ACCOUNT_SUBDIR: &str = "epic-catalog-account";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    version: u32,
    /// Unix time the answer was fetched at.
    fetched_at: i64,
    /// `None` = Epic does not know the item.
    item: Option<CatalogItem>,
}

pub struct CatalogDiskCache {
    dir: PathBuf,
    clock: Clock,
    counter: AtomicU64,
}

impl CatalogDiskCache {
    /// A cache below `base` (typically the app's data directory); the folder is created on the
    /// first write.
    pub fn new(base: impl Into<PathBuf>, clock: Clock) -> Self {
        Self::in_subdir(base, SUBDIR, clock)
    }

    /// Same, for answers fetched with a user token. Kept apart from the account-free ones because
    /// the two tokens may see different items (an "unknown" for one is not one for the other).
    pub fn for_account(base: impl Into<PathBuf>, clock: Clock) -> Self {
        Self::in_subdir(base, ACCOUNT_SUBDIR, clock)
    }

    fn in_subdir(base: impl Into<PathBuf>, subdir: &str, clock: Clock) -> Self {
        Self {
            dir: base.into().join(subdir),
            clock,
            counter: AtomicU64::new(0),
        }
    }

    /// Catalog item ids are 32 hex characters; anything else could escape the folder, so it is
    /// neither read nor written.
    fn file(&self, id: &str) -> Option<PathBuf> {
        let ok = !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        ok.then(|| self.dir.join(format!("{id}.json")))
    }

    /// The cached answer: `None` = not cached (or stale or unreadable), `Some(None)` = Epic does
    /// not know the item.
    pub fn get(&self, id: &str) -> Option<Option<Arc<CatalogItem>>> {
        let text = std::fs::read_to_string(self.file(id)?).ok()?;
        let entry: Entry = serde_json::from_str(&text).ok()?;
        let age = (self.clock)() - entry.fetched_at;
        let ttl = if entry.item.is_some() {
            ITEM_TTL_SECS
        } else {
            UNKNOWN_TTL_SECS
        };
        if entry.version != FORMAT_VERSION || age >= ttl || age < -MAX_FUTURE_SKEW_SECS {
            return None;
        }
        Some(entry.item.map(Arc::new))
    }

    /// Stores an answer (`None` = unknown item). Failures are only logged.
    pub fn put(&self, id: &str, item: Option<&CatalogItem>) {
        let Some(target) = self.file(id) else {
            return;
        };
        let entry = Entry {
            version: FORMAT_VERSION,
            fetched_at: (self.clock)(),
            item: item.cloned(),
        };
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&self.dir)?;
            let tmp = self.dir.join(format!(
                "{id}.{}-{}.tmp",
                std::process::id(),
                self.counter.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::write(&tmp, serde_json::to_vec(&entry)?)?;
            std::fs::rename(&tmp, &target).inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })
        })();
        if let Err(err) = result {
            tracing::debug!(id, %err, "could not write the Epic catalog cache");
        }
    }
}

impl std::fmt::Debug for CatalogDiskCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogDiskCache")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicI64;

    use super::*;
    use crate::providers::epic::web::parse_catalog;

    const CATALOG: &str = include_str!("../../../tests/fixtures/epic/catalog_bulk.json");

    fn cache_at(base: &std::path::Path, now: &Arc<AtomicI64>) -> CatalogDiskCache {
        let now = now.clone();
        CatalogDiskCache::new(base, Arc::new(move || now.load(Ordering::SeqCst)))
    }

    fn sugar() -> CatalogItem {
        parse_catalog(CATALOG)
            .unwrap()
            .remove("sugar-item")
            .unwrap()
    }

    #[test]
    fn round_trips_items_and_unknowns() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Arc::new(AtomicI64::new(1_000));
        let cache = cache_at(tmp.path(), &now);
        assert!(cache.get("sugar-item").is_none());
        cache.put("sugar-item", Some(&sugar()));
        cache.put("gone", None);
        let item = cache.get("sugar-item").unwrap().unwrap();
        assert_eq!(*item, sugar());
        assert_eq!(item.metadata().developers, ["Sugar Studio"]);
        assert!(cache.get("gone").unwrap().is_none());
        // A second cache over the same folder (next app start) sees the entries.
        assert!(cache_at(tmp.path(), &now).get("sugar-item").is_some());
        // No temporary files are left behind.
        let leftovers = std::fs::read_dir(tmp.path().join(SUBDIR))
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn account_answers_are_kept_apart() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Arc::new(AtomicI64::new(1_000));
        let public = cache_at(tmp.path(), &now);
        let account = {
            let now = now.clone();
            CatalogDiskCache::for_account(tmp.path(), Arc::new(move || now.load(Ordering::SeqCst)))
        };
        public.put("x", None);
        assert!(account.get("x").is_none());
        account.put("x", Some(&sugar()));
        assert!(public.get("x").unwrap().is_none());
        assert!(account.get("x").unwrap().is_some());
    }

    #[test]
    fn entries_expire() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Arc::new(AtomicI64::new(1_000));
        let cache = cache_at(tmp.path(), &now);
        cache.put("sugar-item", Some(&sugar()));
        cache.put("gone", None);
        now.store(1_000 + UNKNOWN_TTL_SECS - 1, Ordering::SeqCst);
        assert!(cache.get("gone").is_some());
        now.store(1_000 + UNKNOWN_TTL_SECS, Ordering::SeqCst);
        assert!(cache.get("gone").is_none(), "unknown answers expire sooner");
        assert!(cache.get("sugar-item").is_some());
        now.store(1_000 + ITEM_TTL_SECS - 1, Ordering::SeqCst);
        assert!(cache.get("sugar-item").is_some());
        now.store(1_000 + ITEM_TTL_SECS, Ordering::SeqCst);
        assert!(cache.get("sugar-item").is_none());
        // Entries from the far future (clock was wrong) are not trusted either.
        now.store(1_000, Ordering::SeqCst);
        cache.put("later", None);
        now.store(1_000 - MAX_FUTURE_SKEW_SECS - 1, Ordering::SeqCst);
        assert!(cache.get("later").is_none());
    }

    #[test]
    fn corrupt_and_foreign_files_are_ignored_and_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Arc::new(AtomicI64::new(1_000));
        let cache = cache_at(tmp.path(), &now);
        let dir = tmp.path().join(SUBDIR);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), "{ not json").unwrap();
        std::fs::write(dir.join("b.json"), "").unwrap();
        std::fs::write(
            dir.join("c.json"),
            r#"{"version":99,"fetchedAt":1000,"item":null}"#,
        )
        .unwrap();
        std::fs::write(dir.join("d.json"), r#"{"version":1,"fetchedAt":"x"}"#).unwrap();
        for id in ["a", "b", "c", "d"] {
            assert!(cache.get(id).is_none(), "{id}");
        }
        cache.put("a", None);
        assert!(cache.get("a").unwrap().is_none());
    }

    #[test]
    fn unsafe_ids_are_never_touched() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Arc::new(AtomicI64::new(1_000));
        let cache = cache_at(tmp.path(), &now);
        for id in ["", "../x", "a/b", "a\\b", "a.b", &"x".repeat(65)] {
            cache.put(id, None);
            assert!(cache.get(id).is_none(), "{id}");
        }
        assert!(!tmp.path().join(SUBDIR).exists());
        assert!(!tmp.path().join("x.json").exists());
    }

    #[test]
    fn unwritable_location_is_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        // The base is a file, so the folder cannot be created.
        let blocker = tmp.path().join("file");
        std::fs::write(&blocker, "x").unwrap();
        let now = Arc::new(AtomicI64::new(1_000));
        let cache = cache_at(&blocker, &now);
        cache.put("id", None);
        assert!(cache.get("id").is_none());
    }
}
