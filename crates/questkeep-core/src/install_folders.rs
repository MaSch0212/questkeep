//! Notices installs and uninstalls done in the store clients. The folders that installed store
//! games sit in (`steamapps\common`, the Epic games folder, ...) are watched, not recursively. A
//! new folder there is checked every [`CHECK_EVERY`] until its store reports a game installed in
//! it (`Library::poll_install_folders`); a removed install folder syncs its store once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// How often a new folder is checked. A check reads the store's installed games, which takes a
/// few seconds for some stores (Xbox), so not as often as a watched install from Questkeep.
pub const CHECK_EVERY: Duration = Duration::from_secs(30);

/// How long a new folder is checked: big downloads take hours, and a folder that never becomes a
/// game (a mod tool, a backup) must not be checked forever.
pub const CHECK_FOR: Duration = Duration::from_secs(12 * 60 * 60);

/// A folder path compared the way the file system does (case-insensitive on Windows, either
/// slash, no trailing separator).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key(String);

impl Key {
    fn of(path: &Path) -> Self {
        let text = path.to_string_lossy();
        let text = if cfg!(windows) {
            text.replace('/', "\\").to_lowercase()
        } else {
            text.into_owned()
        };
        let trimmed = text.trim_end_matches(['\\', '/']);
        Key(if trimmed.is_empty() {
            text
        } else {
            trimmed.to_string()
        })
    }

    /// `self` is `other` or a folder inside it.
    fn within(&self, other: &Key) -> bool {
        self.0 == other.0
            || self
                .0
                .strip_prefix(&other.0)
                .is_some_and(|rest| rest.starts_with(['\\', '/']))
    }
}

#[derive(Debug, Clone)]
struct NewFolder {
    path: PathBuf,
    key: Key,
    stores: BTreeSet<String>,
    since: Instant,
    checked: Instant,
}

/// What a poll should do now.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DueChecks {
    /// Stores to sync because one of their install folders disappeared.
    pub removed: BTreeSet<String>,
    /// New folders to check, per store.
    pub new_folders: BTreeMap<String, Vec<PathBuf>>,
}

#[derive(Debug, Default)]
struct State {
    /// Watched folder → the stores with installed games in it.
    parents: BTreeMap<Key, (PathBuf, BTreeSet<String>)>,
    /// Install folder → its store.
    installs: BTreeMap<Key, String>,
    new_folders: Vec<NewFolder>,
    removed: BTreeSet<String>,
}

impl State {
    fn added(&mut self, path: &Path, now: Instant) {
        let key = Key::of(path);
        if self.installs.contains_key(&key) || self.new_folders.iter().any(|f| f.key == key) {
            return;
        }
        let Some((_, stores)) = path.parent().and_then(|p| self.parents.get(&Key::of(p))) else {
            return;
        };
        if !path.is_dir() {
            return;
        }
        tracing::info!(path = %path.display(), ?stores, "new folder next to installed games");
        self.new_folders.push(NewFolder {
            path: path.to_path_buf(),
            key,
            stores: stores.clone(),
            since: now,
            // The store just created it: the first check waits a round.
            checked: now,
        });
    }

    fn gone(&mut self, path: &Path) {
        let key = Key::of(path);
        self.new_folders.retain(|f| f.key != key);
        if let Some(store) = self.installs.get(&key) {
            tracing::info!(path = %path.display(), store, "an install folder was removed");
            self.removed.insert(store.clone());
        }
    }

    fn on_event(&mut self, kind: &EventKind, paths: &[PathBuf], now: Instant) {
        match kind {
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                paths.iter().for_each(|p| self.added(p, now));
            }
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                paths.iter().for_each(|p| self.gone(p));
            }
            // A rename reported as one event: from, then to.
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if let [from, to] = paths {
                    self.gone(from);
                    self.added(to, now);
                }
            }
            _ => {}
        }
    }

    fn due(&mut self, now: Instant) -> DueChecks {
        self.new_folders
            .retain(|f| now.duration_since(f.since) < CHECK_FOR && f.path.is_dir());
        let mut due = DueChecks {
            removed: std::mem::take(&mut self.removed),
            new_folders: BTreeMap::new(),
        };
        for folder in &mut self.new_folders {
            if now.duration_since(folder.checked) < CHECK_EVERY {
                continue;
            }
            folder.checked = now;
            for store in &folder.stores {
                due.new_folders
                    .entry(store.clone())
                    .or_default()
                    .push(folder.path.clone());
            }
        }
        due
    }
}

/// The watched folders and what was noticed in them.
#[derive(Default)]
pub struct InstallFolders {
    state: Arc<Mutex<State>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl InstallFolders {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Watches the folders these installed games are in: `(store, install folder)` pairs. Folders
    /// already watched stay watched; ones without installed games any more are dropped.
    pub fn watch(&self, installed: &[(String, String)]) {
        let mut parents: BTreeMap<Key, (PathBuf, BTreeSet<String>)> = BTreeMap::new();
        let mut installs = BTreeMap::new();
        for (store, dir) in installed {
            let dir = Path::new(dir);
            installs.insert(Key::of(dir), store.clone());
            if let Some(parent) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
                parents
                    .entry(Key::of(parent))
                    .or_insert_with(|| (parent.to_path_buf(), BTreeSet::new()))
                    .1
                    .insert(store.clone());
            }
        }
        let mut watcher = self.watcher.lock().unwrap_or_else(PoisonError::into_inner);
        if watcher.is_none() {
            let state = self.state.clone();
            let created = notify::recommended_watcher(
                move |event: notify::Result<notify::Event>| match event {
                    Ok(event) => state
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .on_event(&event.kind, &event.paths, Instant::now()),
                    Err(err) => tracing::debug!(%err, "install folder watch error"),
                },
            );
            match created {
                Ok(created) => *watcher = Some(created),
                Err(err) => {
                    tracing::warn!(%err, "could not watch the install folders");
                    return;
                }
            }
        }
        let Some(watcher) = watcher.as_mut() else {
            return;
        };
        let mut state = self.state();
        let old: Vec<(Key, PathBuf)> = state
            .parents
            .iter()
            .map(|(k, (p, _))| (k.clone(), p.clone()))
            .collect();
        for (key, path) in &old {
            if !parents.contains_key(key)
                && let Err(err) = watcher.unwatch(path)
            {
                tracing::debug!(path = %path.display(), %err, "could not stop watching a folder");
            }
        }
        let mut watched = BTreeMap::new();
        for (key, (path, stores)) in parents {
            let known = old.iter().any(|(k, _)| *k == key);
            if !known && let Err(err) = watcher.watch(&path, RecursiveMode::NonRecursive) {
                // Missing drives, folders without access (WindowsApps): nothing to notice there.
                tracing::debug!(path = %path.display(), %err, "could not watch a folder");
                continue;
            }
            watched.insert(key, (path, stores));
        }
        tracing::debug!(folders = watched.len(), "watching install folders");
        state.parents = watched;
        state.installs = installs;
        // A new folder that became a known install is done.
        let installs = &state.installs;
        let done: Vec<Key> = state
            .new_folders
            .iter()
            .filter(|f| installs.contains_key(&f.key))
            .map(|f| f.key.clone())
            .collect();
        state.new_folders.retain(|f| !done.contains(&f.key));
    }

    /// Stops watching and forgets what was noticed (the setting was turned off).
    pub fn stop(&self) {
        *self.watcher.lock().unwrap_or_else(PoisonError::into_inner) = None;
        *self.state() = State::default();
    }

    /// What to check now; new folders are handed out once every [`CHECK_EVERY`].
    pub fn due(&self, now: Instant) -> DueChecks {
        self.state().due(now)
    }

    /// Stops checking a new folder (a game is installed in it).
    pub fn finish(&self, path: &Path) {
        let key = Key::of(path);
        self.state().new_folders.retain(|f| f.key != key);
    }

    /// Whether `dir` (a game's install folder) is `folder` or inside it.
    pub fn contains(folder: &Path, dir: &Path) -> bool {
        Key::of(dir).within(&Key::of(folder))
    }

    #[cfg(test)]
    pub(crate) fn notice(&self, kind: EventKind, paths: &[PathBuf], now: Instant) {
        self.state().on_event(&kind, paths, now);
    }

    #[cfg(test)]
    fn set_known(&self, installed: &[(String, String)]) {
        let mut state = self.state();
        for (store, dir) in installed {
            let dir = Path::new(dir);
            state.installs.insert(Key::of(dir), store.clone());
            if let Some(parent) = dir.parent() {
                state
                    .parents
                    .entry(Key::of(parent))
                    .or_insert_with(|| (parent.to_path_buf(), BTreeSet::new()))
                    .1
                    .insert(store.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, RemoveKind};

    fn installed(store: &str, dir: &Path) -> (String, String) {
        (store.to_string(), dir.to_string_lossy().into_owned())
    }

    #[test]
    fn a_new_folder_next_to_installed_games_is_checked_every_round() {
        let tmp = tempfile::tempdir().unwrap();
        let common = tmp.path().join("common");
        let old = common.join("Old Game");
        let new = common.join("New Game");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        let folders = InstallFolders::default();
        folders.set_known(&[installed("steam", &old)]);
        let start = Instant::now();

        folders.notice(
            EventKind::Create(CreateKind::Any),
            std::slice::from_ref(&new),
            start,
        );
        // A file, an unwatched folder and a known install are not new games.
        let file = common.join("readme.txt");
        std::fs::write(&file, "x").unwrap();
        let elsewhere = tmp.path().join("Elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        folders.notice(
            EventKind::Create(CreateKind::Any),
            &[file, elsewhere, old.clone()],
            start,
        );

        assert_eq!(folders.due(start), DueChecks::default(), "waits a round");
        let later = start + CHECK_EVERY;
        assert_eq!(
            folders.due(later).new_folders,
            BTreeMap::from([("steam".to_string(), vec![new.clone()])])
        );
        assert!(folders.due(later).new_folders.is_empty(), "once per round");

        folders.finish(&new);
        assert!(folders.due(later + CHECK_EVERY).new_folders.is_empty());
    }

    #[test]
    fn checks_stop_when_the_folder_goes_or_after_a_while() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("Old");
        let a = tmp.path().join("A");
        let b = tmp.path().join("B");
        for dir in [&old, &a, &b] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let folders = InstallFolders::default();
        folders.set_known(&[installed("epic", &old)]);
        let start = Instant::now();
        folders.notice(
            EventKind::Create(CreateKind::Folder),
            &[a.clone(), b.clone()],
            start,
        );

        std::fs::remove_dir(&a).unwrap();
        let due = folders.due(start + CHECK_EVERY);
        assert_eq!(due.new_folders["epic"], vec![b.clone()], "A is gone");
        assert!(due.removed.is_empty(), "A was never an install");

        assert!(folders.due(start + CHECK_FOR).new_folders.is_empty());
    }

    #[test]
    fn a_removed_install_folder_syncs_its_store_once() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Game");
        let folders = InstallFolders::default();
        folders.set_known(&[installed("gog", &game)]);
        folders.notice(
            EventKind::Remove(RemoveKind::Any),
            std::slice::from_ref(&game),
            Instant::now(),
        );
        let now = Instant::now();
        assert_eq!(
            folders.due(now).removed,
            BTreeSet::from(["gog".to_string()])
        );
        assert!(folders.due(now).removed.is_empty());
    }

    #[test]
    fn a_folder_renamed_into_place_counts_as_new() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("Old");
        let staged = tmp.path().join("New");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        let folders = InstallFolders::default();
        folders.set_known(&[installed("ea", &old)]);
        let start = Instant::now();
        folders.notice(
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            std::slice::from_ref(&staged),
            start,
        );
        assert_eq!(
            folders.due(start + CHECK_EVERY).new_folders["ea"],
            vec![staged]
        );
    }

    #[test]
    fn install_dirs_match_inside_the_new_folder() {
        let folder = Path::new("D:/Games/Steam/steamapps/common/Hades");
        assert!(InstallFolders::contains(folder, folder));
        assert!(InstallFolders::contains(folder, &folder.join("bin")));
        assert!(!InstallFolders::contains(
            folder,
            Path::new("D:/Games/Steam/steamapps/common/Hades II")
        ));
        if cfg!(windows) {
            assert!(InstallFolders::contains(
                folder,
                Path::new(r"d:\games\steam\steamapps\common\hades\")
            ));
        }
    }

    #[test]
    fn the_real_watcher_notices_a_new_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("Old");
        std::fs::create_dir_all(&old).unwrap();
        let folders = InstallFolders::default();
        folders.watch(&[installed("steam", &old)]);
        let new = tmp.path().join("New");
        std::fs::create_dir_all(&new).unwrap();
        let start = Instant::now();
        loop {
            let due = folders.due(Instant::now() + CHECK_EVERY);
            if due.new_folders.get("steam").is_some_and(|f| f.len() == 1) {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "no event");
            std::thread::sleep(Duration::from_millis(20));
        }
        folders.stop();
    }
}
