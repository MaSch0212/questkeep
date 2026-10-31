//! Notices installed games started outside Questkeep (from a store client, a mod manager, a
//! shortcut) and records their play sessions like the trackers do.
//!
//! Built to cost next to nothing while it runs in the background: one thread wakes every few
//! seconds and lists process ids (no per-process work or allocation for processes it already
//! knows). Only a process it hasn't seen yet has its executable path looked up, once, and matched
//! against the install folders by walking up its parent folders in a hash map. When the setting
//! is off the thread blocks without a timeout, and without installed games it skips listing.
//!
//! A game is playing as soon as one of its programs that isn't ignored (crash handlers, known
//! prelaunchers, the game's own list; see [`IgnoredPrograms`]) runs. Runs shorter than
//! `min_session` (a settings tool, a launcher updating itself) are dropped when they end and leave
//! no playtime. Games a tracker already follows (started from Questkeep) are left to that tracker.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::Utc;
use uuid::Uuid;

use super::ActiveSession;
use super::process::{IgnoredPrograms, normalize_dir};
use crate::db::Database;
use crate::db::DetectionDir;
use crate::events::{CoreEvent, EventBus};
use crate::platform::{self, ProcessRef};

/// Timing knobs; tests shorten them.
#[derive(Debug, Clone, Copy)]
pub struct WatcherConfig {
    /// Interval between process listings.
    pub interval: Duration,
    /// Runs shorter than this are dropped when they end (no playtime, play count or last played).
    pub min_session: Duration,
    /// How often a running session's heartbeat is saved (for crash recovery).
    pub heartbeat: Duration,
    /// An interval longer than `interval + sleep_gap` is treated as system sleep.
    pub sleep_gap: Duration,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            min_session: Duration::from_secs(60),
            heartbeat: Duration::from_secs(30),
            sleep_gap: Duration::from_secs(30),
        }
    }
}

/// Where processes come from. Abstracted so the watcher is testable without real processes.
pub trait ProcessSource: Send {
    /// Replaces `out` with the ids of the running processes.
    fn list(&mut self, out: &mut Vec<u32>);
    /// Paths identifying one process (its executable first) and the reference that tells
    /// whether it is still the same, running process.
    fn open(&mut self, pid: u32) -> (Vec<String>, ProcessRef);
}

/// The operating system's processes.
pub struct SystemSource;

impl ProcessSource for SystemSource {
    fn list(&mut self, out: &mut Vec<u32>) {
        platform::list_processes(out);
    }

    fn open(&mut self, pid: u32) -> (Vec<String>, ProcessRef) {
        platform::open_process(pid)
    }
}

pub(super) enum Control {
    Enable(bool),
    Stop,
}

/// The running watcher thread.
pub(super) struct WatcherHandle {
    pub(super) control: mpsc::Sender<Control>,
    pub(super) thread: JoinHandle<()>,
}

/// State shared between the watcher thread and the launcher.
pub(super) struct Shared {
    pub(super) db: Arc<Database>,
    pub(super) events: EventBus,
    /// The launcher's trackers; their games are skipped.
    pub(super) trackers: Arc<Mutex<HashMap<Uuid, ActiveSession>>>,
    /// Games with a running outside session, reported as running by the launcher.
    pub(super) running: Arc<Mutex<HashSet<Uuid>>>,
    /// Set when the library changed, so install folders are read again.
    pub(super) library_changed: Arc<AtomicBool>,
}

/// Install folders mapped to the game rows they belong to.
#[derive(Debug, Default, PartialEq, Eq)]
struct DirIndex {
    dirs: HashMap<String, Uuid>,
    /// Programs that don't count as playing, per game row.
    ignored: HashMap<Uuid, IgnoredPrograms>,
}

impl DirIndex {
    /// `rows` in priority order (the first row wins a folder shared by copies). Folders that
    /// would match unrelated programs (drive roots, the home folder, Program Files) are left out.
    fn build(rows: Vec<DetectionDir>) -> Self {
        let blocked = blocked_dirs();
        let mut index = Self::default();
        for row in rows {
            let dir = normalize_dir(row.install_dir.trim());
            let depth = dir.split('/').filter(|s| !s.is_empty()).count();
            if depth < 2 || blocked.contains(&dir) {
                continue;
            }
            index.dirs.entry(dir).or_insert(row.id);
            index
                .ignored
                .insert(row.id, IgnoredPrograms::new(&row.ignored_processes));
        }
        index
    }

    /// The game whose install folder holds one of `paths` (the deepest folder wins, so a game
    /// installed inside another game's folder gets its own programs), and whether the process is
    /// a program that doesn't count as playing it.
    fn lookup(&self, paths: &[String]) -> Option<(Uuid, bool)> {
        let game = paths.iter().find_map(|path| {
            let normalized = normalize_dir(path);
            normalized
                .rmatch_indices('/')
                .find_map(|(i, _)| self.dirs.get(&normalized[..=i]))
        })?;
        let fallback;
        let ignored = match self.ignored.get(game) {
            Some(ignored) => ignored,
            None => {
                fallback = IgnoredPrograms::default();
                &fallback
            }
        };
        Some((*game, paths.iter().any(|path| ignored.matches(path))))
    }
}

fn blocked_dirs() -> HashSet<String> {
    let mut dirs: Vec<std::path::PathBuf> = dirs::home_dir().into_iter().collect();
    for var in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "SystemRoot",
    ] {
        if let Some(value) = std::env::var_os(var) {
            dirs.push(value.into());
        }
    }
    dirs.iter()
        .map(|d| normalize_dir(&d.display().to_string()))
        .collect()
}

/// A process seen before, with what it matched.
struct Known {
    /// Keeps the id from being reused while known (Windows) and tells when the process exited.
    process: ProcessRef,
    /// The listing it was last seen in.
    seen: u64,
    game: Option<Uuid>,
    helper: bool,
}

/// A game seen running outside Questkeep.
struct Outside {
    played: Duration,
    session: i64,
    last_heartbeat: Instant,
}

pub(super) struct Watcher {
    shared: Shared,
    config: WatcherConfig,
    source: Box<dyn ProcessSource>,
    control: mpsc::Receiver<Control>,
    enabled: bool,
    index: DirIndex,
    listing: Vec<u32>,
    known: HashMap<u32, Known>,
    generation: u64,
    live: HashSet<Uuid>,
    outside: HashMap<Uuid, Outside>,
    last_tick: Option<Instant>,
}

impl Watcher {
    pub(super) fn new(
        shared: Shared,
        config: WatcherConfig,
        source: Box<dyn ProcessSource>,
        control: mpsc::Receiver<Control>,
        enabled: bool,
    ) -> Self {
        Self {
            shared,
            config,
            source,
            control,
            enabled,
            index: DirIndex::default(),
            listing: Vec::new(),
            known: HashMap::new(),
            generation: 0,
            live: HashSet::new(),
            outside: HashMap::new(),
            last_tick: None,
        }
    }

    pub(super) fn run(mut self) {
        self.shared.library_changed.store(true, Ordering::SeqCst);
        let mut next = Instant::now();
        loop {
            let message = if self.enabled {
                self.control
                    .recv_timeout(next.saturating_duration_since(Instant::now()))
            } else {
                // Nothing to do until switched on: no wake-ups at all.
                self.control
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected)
            };
            match message {
                Err(RecvTimeoutError::Timeout) => {
                    self.tick();
                    next = Instant::now() + self.config.interval;
                }
                Ok(Control::Enable(enabled)) => {
                    if enabled == self.enabled {
                        continue;
                    }
                    self.enabled = enabled;
                    if enabled {
                        self.shared.library_changed.store(true, Ordering::SeqCst);
                        next = Instant::now();
                    } else {
                        self.finish_all();
                        self.known = HashMap::new();
                        self.index = DirIndex::default();
                        self.last_tick = None;
                    }
                }
                Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    self.finish_all();
                    return;
                }
            }
        }
    }

    fn tick(&mut self) {
        if self.shared.library_changed.swap(false, Ordering::SeqCst) {
            match self.shared.db.launch_detection_dirs() {
                Ok(rows) => {
                    let index = DirIndex::build(rows);
                    if index != self.index {
                        // Earlier matches may point at the wrong game now.
                        self.index = index;
                        self.known.clear();
                    }
                }
                Err(err) => tracing::warn!(%err, "could not read install folders"),
            }
        }
        let now = Instant::now();
        let interval = self.last_tick.map(|last| now.duration_since(last));
        self.last_tick = Some(now);
        if self.index.dirs.is_empty() && self.outside.is_empty() {
            return;
        }

        self.generation += 1;
        let generation = self.generation;
        self.source.list(&mut self.listing);
        for &pid in &self.listing {
            match self.known.get_mut(&pid) {
                Some(known) if !known.process.has_exited() => known.seen = generation,
                _ => {
                    let (paths, process) = self.source.open(pid);
                    let matched = self.index.lookup(&paths);
                    self.known.insert(
                        pid,
                        Known {
                            process,
                            seen: generation,
                            game: matched.map(|(game, _)| game),
                            helper: matched.is_some_and(|(_, helper)| helper),
                        },
                    );
                }
            }
        }
        self.known.retain(|_, known| known.seen == generation);

        self.live.clear();
        self.live.extend(
            self.known
                .values()
                .filter(|known| !known.helper)
                .filter_map(|known| known.game),
        );
        if self.live.is_empty() && self.outside.is_empty() {
            return;
        }
        let tracked: HashSet<Uuid> = self
            .shared
            .trackers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, session)| !session.handle.is_finished())
            .map(|(id, _)| *id)
            .collect();
        self.live.retain(|game| !tracked.contains(game));

        let counted = interval
            .filter(|i| *i <= self.config.interval + self.config.sleep_gap)
            .unwrap_or_default();
        let mut ended = Vec::new();
        for (game, outside) in &mut self.outside {
            if !self.live.contains(game) {
                ended.push(*game);
                continue;
            }
            outside.played += counted;
            if now.duration_since(outside.last_heartbeat) >= self.config.heartbeat {
                outside.last_heartbeat = now;
                if let Err(err) = self
                    .shared
                    .db
                    .heartbeat_session(outside.session, Utc::now())
                {
                    tracing::warn!(%err, "session heartbeat failed");
                }
            }
        }
        for game in ended {
            if let Some(outside) = self.outside.remove(&game) {
                self.finish(game, outside);
            }
        }
        let new: Vec<Uuid> = self
            .live
            .iter()
            .filter(|game| !self.outside.contains_key(game))
            .copied()
            .collect();
        for game in new {
            match self.shared.db.begin_session(game, Utc::now()) {
                Ok(session) => {
                    tracing::info!(%game, "game started outside Questkeep");
                    self.outside.insert(
                        game,
                        Outside {
                            played: Duration::ZERO,
                            session,
                            last_heartbeat: now,
                        },
                    );
                    self.shared
                        .running
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(game);
                    self.shared
                        .events
                        .publish(CoreEvent::GameStarted { id: game });
                }
                Err(err) => tracing::error!(%err, "could not start play session"),
            }
        }
    }

    fn finish(&self, game: Uuid, outside: Outside) {
        let (saved, seconds) = if outside.played < self.config.min_session {
            // A settings tool, a self-updating launcher, a crash on start: not a play session.
            tracing::info!(%game, played = ?outside.played, "short outside run, no session");
            (self.shared.db.discard_session(outside.session), 0)
        } else {
            let seconds = outside.played.as_secs();
            let saved = self
                .shared
                .db
                .finish_session(outside.session, Utc::now(), seconds)
                .map(|_| ());
            (saved, seconds)
        };
        // Not running only once the session is saved, so a reader never sees neither.
        self.shared
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&game);
        match saved {
            Ok(_) => {
                tracing::info!(%game, seconds, "outside play session finished");
                self.shared.events.publish(CoreEvent::GameStopped {
                    id: game,
                    session_seconds: seconds,
                });
            }
            Err(err) => {
                tracing::error!(%err, "could not save play session");
                self.shared.events.publish(CoreEvent::GameError {
                    id: game,
                    message: format!("Playtime could not be saved: {err}"),
                });
            }
        }
    }

    fn finish_all(&mut self) {
        for (game, outside) in std::mem::take(&mut self.outside) {
            self.finish(game, outside);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(dirs: Vec<(Uuid, String)>) -> Vec<DetectionDir> {
        dirs.into_iter()
            .map(|(id, install_dir)| DetectionDir {
                id,
                install_dir,
                ignored_processes: vec!["Launcher.exe".into()],
            })
            .collect()
    }

    #[test]
    fn index_picks_the_deepest_folder_and_flags_helpers() {
        let outer = Uuid::new_v4();
        let inner = Uuid::new_v4();
        let index = DirIndex::build(rows(vec![
            (outer, "/games/Collection".into()),
            (inner, "/games/Collection/Bonus Game/".into()),
        ]));
        let lookup = |p: &str| index.lookup(&[p.to_string()]);
        assert_eq!(lookup("/games/Collection/main.exe"), Some((outer, false)));
        assert_eq!(
            lookup("/games/Collection/Bonus Game/bin/bonus.exe"),
            Some((inner, false))
        );
        assert_eq!(
            lookup("/games/Collection/Bonus Game/Launcher.exe"),
            Some((inner, true))
        );
        assert_eq!(lookup("/games/Collection 2/main.exe"), None);
        assert_eq!(index.lookup(&[]), None);
    }

    #[test]
    fn index_skips_folders_that_hold_unrelated_programs() {
        let index = DirIndex::build(rows(vec![
            (Uuid::new_v4(), "/".into()),
            (Uuid::new_v4(), "C:\\".into()),
            (Uuid::new_v4(), "/games".into()),
            (Uuid::new_v4(), "  ".into()),
        ]));
        assert!(index.dirs.is_empty(), "{:?}", index.dirs);
    }

    #[test]
    fn first_row_wins_a_shared_folder() {
        let first = Uuid::new_v4();
        let index = DirIndex::build(rows(vec![
            (first, "/games/Shared".into()),
            (Uuid::new_v4(), "/games/shared/".into()),
        ]));
        assert_eq!(
            index.lookup(&["/games/Shared/x.exe".into()]),
            Some((first, false))
        );
    }
}
