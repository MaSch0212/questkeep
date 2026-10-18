//! Launching games and tracking play sessions.
//!
//! Each launched game gets one tracker thread that polls process snapshots, heartbeats its
//! session row and, when the game stops, credits the measured time. `Launcher::shutdown` stops
//! and joins every tracker and saves elapsed time, so no tracker outlives the app.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: polling every 2 s, skipping
//! intervals that are much longer than expected (system sleep), directory tracking for URI
//! launches. Unlike Playnite we wait for URI-launched games with a timeout and persist sessions.

mod process;

use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::Utc;
use uuid::Uuid;

pub use process::{Matcher, ProcInfo, ProcessProbe, SystemProbe, normalize_dir};

use crate::db::Database;
use crate::error::{CoreError, Result};
use crate::events::{CoreEvent, EventBus};
use crate::model::{ActionKind, Game, TrackingMode};
use crate::platform;

/// Timing knobs; tests shorten them.
#[derive(Debug, Clone, Copy)]
pub struct LauncherConfig {
    /// Interval between running-state checks.
    pub poll_interval: Duration,
    /// How long to wait for a game's process to appear (URI launches, directory tracking).
    pub startup_timeout: Duration,
    /// An interval longer than `poll_interval + sleep_gap` is treated as system sleep and not
    /// counted as playtime.
    pub sleep_gap: Duration,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            startup_timeout: Duration::from_secs(90),
            sleep_gap: Duration::from_secs(30),
        }
    }
}

struct ActiveSession {
    stop: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

/// Starts games and owns their tracker threads.
pub struct Launcher {
    db: Arc<Database>,
    events: EventBus,
    probe: Arc<dyn ProcessProbe>,
    config: LauncherConfig,
    active: Arc<Mutex<HashMap<Uuid, ActiveSession>>>,
}

impl Launcher {
    pub fn new(db: Arc<Database>, events: EventBus) -> Self {
        Self::with_probe(
            db,
            events,
            Arc::new(SystemProbe::new()),
            LauncherConfig::default(),
        )
    }

    pub fn with_probe(
        db: Arc<Database>,
        events: EventBus,
        probe: Arc<dyn ProcessProbe>,
        config: LauncherConfig,
    ) -> Self {
        Self {
            db,
            events,
            probe,
            config,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn active(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, ActiveSession>> {
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ids of games that currently have a tracker (starting or running).
    pub fn running(&self) -> Vec<Uuid> {
        let mut active = self.active();
        active.retain(|_, s| !s.handle.is_finished());
        active.keys().copied().collect()
    }

    /// Starts `game` and begins tracking it.
    pub fn launch(&self, game: &Game) -> Result<()> {
        if self.running().contains(&game.id) {
            return Err(CoreError::Launch(format!(
                "{} is already running.",
                game.name
            )));
        }
        let action = game.play_action.as_ref().ok_or_else(|| {
            CoreError::Launch(format!("{} has no launch action configured.", game.name))
        })?;
        let target = action.target.trim();
        if target.is_empty() {
            return Err(CoreError::Launch("The launch target is empty.".into()));
        }

        let tracking_dir = action
            .tracking_path
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .or(game.install_dir.as_deref())
            .map(str::to_string);

        let (matcher, child) = match action.kind {
            ActionKind::File => {
                let exe = Path::new(target);
                let exe_dir = exe.parent().map(|p| p.display().to_string());
                let mut command = Command::new(exe);
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let working_dir = action
                    .working_dir
                    .as_deref()
                    .filter(|w| !w.trim().is_empty())
                    .map(str::to_string)
                    .or_else(|| exe_dir.clone());
                if let Some(dir) = &working_dir {
                    command.current_dir(dir);
                }
                if let Some(args) = action.args.as_deref() {
                    platform::add_raw_args(&mut command, args);
                }
                let child = command
                    .spawn()
                    .map_err(|e| CoreError::Launch(format!("Could not start '{target}': {e}")))?;
                let pid = child.id();
                let dir = tracking_dir.clone().or(exe_dir);
                let matcher = match action.tracking {
                    // Playnite's default too. Directory matching is opt-in: executables in shared
                    // folders would otherwise match unrelated processes.
                    TrackingMode::Default | TrackingMode::Process => Some(Matcher::tree(pid)),
                    TrackingMode::OriginalProcess => Some(Matcher::Original { pid }),
                    TrackingMode::Directory => dir.as_deref().map(Matcher::directory),
                    TrackingMode::Name => Some(Matcher::name(
                        action.tracking_path.as_deref().unwrap_or(target),
                    )),
                };
                (matcher, Some(child))
            }
            ActionKind::Uri => {
                platform::open_uri(target)
                    .map_err(|e| CoreError::Launch(format!("Could not open '{target}': {e}")))?;
                let matcher = match action.tracking {
                    TrackingMode::Name => action.tracking_path.as_deref().map(Matcher::name),
                    // No process of our own to follow; the install dir is the best signal.
                    _ => tracking_dir.as_deref().map(Matcher::directory),
                };
                (matcher, None)
            }
        };

        let Some(matcher) = matcher else {
            tracing::warn!(game = %game.id, "launched without a way to track playtime");
            self.events.publish(CoreEvent::GameError {
                id: game.id,
                message: format!(
                    "{} was started, but playtime can't be tracked: no install folder is known.",
                    game.name
                ),
            });
            return Ok(());
        };
        tracing::info!(game = %game.id, name = %game.name, ?matcher, "game launched");
        self.start_tracking(game.id, game.name.clone(), matcher, child);
        Ok(())
    }

    /// Spawns the tracker thread. Public within the crate for tests with fake probes.
    pub(crate) fn start_tracking(
        &self,
        game_id: Uuid,
        name: String,
        matcher: Matcher,
        child: Option<Child>,
    ) {
        let (stop_tx, stop_rx) = mpsc::channel();
        let tracker = Tracker {
            game_id,
            name,
            matcher,
            child,
            db: self.db.clone(),
            events: self.events.clone(),
            probe: self.probe.clone(),
            config: self.config,
            stop: stop_rx,
        };
        let spawn = std::thread::Builder::new()
            .name(format!("tracker-{game_id}"))
            .spawn(move || tracker.run());
        match spawn {
            Ok(handle) => {
                self.active().insert(
                    game_id,
                    ActiveSession {
                        stop: stop_tx,
                        handle,
                    },
                );
            }
            Err(err) => {
                tracing::error!(%err, "could not spawn tracker thread");
                self.events.publish(CoreEvent::GameError {
                    id: game_id,
                    message: format!("Playtime tracking could not start: {err}"),
                });
            }
        }
    }

    /// Stops all trackers, saving the elapsed time of running sessions, and waits for them.
    pub fn shutdown(&self) {
        let sessions: Vec<(Uuid, ActiveSession)> = self.active().drain().collect();
        for (_, session) in &sessions {
            let _ = session.stop.send(());
        }
        for (id, session) in sessions {
            if session.handle.join().is_err() {
                tracing::error!(game = %id, "tracker thread panicked");
            }
        }
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Tracker {
    game_id: Uuid,
    name: String,
    matcher: Matcher,
    child: Option<Child>,
    db: Arc<Database>,
    events: EventBus,
    probe: Arc<dyn ProcessProbe>,
    config: LauncherConfig,
    stop: mpsc::Receiver<()>,
}

enum Wait {
    Elapsed,
    Stop,
}

impl Tracker {
    /// Sleeps for one interval or until shutdown is requested.
    fn wait(&self, interval: Duration) -> Wait {
        match self.stop.recv_timeout(interval) {
            Err(RecvTimeoutError::Timeout) => Wait::Elapsed,
            Ok(()) | Err(RecvTimeoutError::Disconnected) => Wait::Stop,
        }
    }

    fn check(&mut self) -> bool {
        // Reap our own child so it doesn't linger as a zombie on Unix.
        if let Some(child) = self.child.as_mut()
            && matches!(child.try_wait(), Ok(Some(_)) | Err(_))
        {
            self.child = None;
        }
        let snapshot = self.probe.snapshot();
        self.matcher.is_running(&snapshot)
    }

    fn run(mut self) {
        // Phase 1: wait for the game to show up. File launches usually match immediately.
        let startup_started = Instant::now();
        let startup_poll = self.config.poll_interval.min(Duration::from_secs(1));
        while !self.check() {
            if startup_started.elapsed() >= self.config.startup_timeout {
                tracing::warn!(game = %self.game_id, "game process never appeared");
                self.events.publish(CoreEvent::GameError {
                    id: self.game_id,
                    message: format!(
                        "{} didn't start or its process couldn't be found, so no playtime was recorded.",
                        self.name
                    ),
                });
                return;
            }
            if let Wait::Stop = self.wait(startup_poll) {
                return;
            }
        }

        let session_id = match self.db.begin_session(self.game_id, Utc::now()) {
            Ok(id) => id,
            Err(err) => {
                tracing::error!(%err, "could not start play session");
                self.events.publish(CoreEvent::GameError {
                    id: self.game_id,
                    message: format!("Playtime can't be recorded: {err}"),
                });
                return;
            }
        };
        self.events
            .publish(CoreEvent::GameStarted { id: self.game_id });

        // Phase 2: poll until the game stops or the app shuts down.
        let max_interval = self.config.poll_interval + self.config.sleep_gap;
        let mut played = Duration::ZERO;
        let mut last_tick = Instant::now();
        loop {
            let wait = self.wait(self.config.poll_interval);
            let now = Instant::now();
            let interval = now.duration_since(last_tick);
            last_tick = now;
            if interval <= max_interval {
                played += interval;
            } else {
                tracing::info!(game = %self.game_id, ?interval, "skipping long interval (system sleep?)");
            }
            if let Wait::Stop = wait {
                tracing::info!(game = %self.game_id, "app shutting down, saving running session");
                break;
            }
            if !self.check() {
                break;
            }
            if let Err(err) = self.db.heartbeat_session(session_id, Utc::now()) {
                tracing::warn!(%err, "session heartbeat failed");
            }
        }

        let seconds = played.as_secs();
        match self.db.finish_session(session_id, Utc::now(), seconds) {
            Ok(_) => {
                tracing::info!(game = %self.game_id, seconds, "play session finished");
                self.events.publish(CoreEvent::GameStopped {
                    id: self.game_id,
                    session_seconds: seconds,
                });
            }
            Err(err) => {
                tracing::error!(%err, "could not save play session");
                self.events.publish(CoreEvent::GameError {
                    id: self.game_id,
                    message: format!("Playtime could not be saved: {err}"),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::db::NewGameRecord;
    use crate::model::PlayAction;

    /// Probe whose snapshot is controlled by the test.
    #[derive(Default)]
    struct FakeProbe {
        running: AtomicBool,
    }
    impl ProcessProbe for FakeProbe {
        fn snapshot(&self) -> Vec<ProcInfo> {
            if self.running.load(Ordering::SeqCst) {
                vec![ProcInfo {
                    pid: 4242,
                    parent: None,
                    exe: Some("/games/fake/fake.exe".into()),
                    name: "fake.exe".into(),
                    hint_paths: Vec::new(),
                }]
            } else {
                Vec::new()
            }
        }
    }

    fn fast() -> LauncherConfig {
        LauncherConfig {
            poll_interval: Duration::from_millis(20),
            startup_timeout: Duration::from_millis(300),
            sleep_gap: Duration::from_secs(5),
        }
    }

    struct Setup {
        db: Arc<Database>,
        probe: Arc<FakeProbe>,
        launcher: Launcher,
        events: Arc<Mutex<Vec<CoreEvent>>>,
        game: Game,
    }

    fn setup() -> Setup {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let game = db
            .insert_game(
                &NewGameRecord {
                    name: "Fake".into(),
                    source: "test".into(),
                    install_dir: Some("/games/fake".into()),
                    play_action: Some(PlayAction::uri("fake://1")),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap();
        let bus = EventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let e = events.clone();
        bus.subscribe(move |ev| e.lock().unwrap().push(ev.clone()));
        let probe = Arc::new(FakeProbe::default());
        let launcher = Launcher::with_probe(db.clone(), bus, probe.clone(), fast());
        Setup {
            db,
            probe,
            launcher,
            events,
            game,
        }
    }

    fn wait_until(mut cond: impl FnMut() -> bool) {
        let start = Instant::now();
        while !cond() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn records_session_when_game_stops() {
        let s = setup();
        s.launcher.start_tracking(
            s.game.id,
            s.game.name.clone(),
            Matcher::directory("/games/fake"),
            None,
        );
        // Not running yet: waits for startup.
        std::thread::sleep(Duration::from_millis(50));
        assert!(s.db.open_sessions().unwrap().is_empty());
        assert_eq!(s.launcher.running(), vec![s.game.id]);

        s.probe.running.store(true, Ordering::SeqCst);
        wait_until(|| s.db.open_sessions().unwrap().len() == 1);
        std::thread::sleep(Duration::from_millis(1100));
        s.probe.running.store(false, Ordering::SeqCst);
        wait_until(|| s.launcher.running().is_empty());

        let game = s.db.get_game(s.game.id).unwrap();
        assert_eq!(game.play_count, 1);
        assert!(
            (1..=2).contains(&game.playtime_seconds),
            "{}",
            game.playtime_seconds
        );
        assert!(game.last_played.is_some());
        let events = s.events.lock().unwrap();
        assert!(matches!(events[0], CoreEvent::GameStarted { .. }));
        assert!(matches!(events[1], CoreEvent::GameStopped { .. }));
    }

    #[test]
    fn startup_timeout_reports_error_without_session() {
        let s = setup();
        s.launcher.start_tracking(
            s.game.id,
            s.game.name.clone(),
            Matcher::directory("/games/fake"),
            None,
        );
        wait_until(|| s.launcher.running().is_empty());
        assert!(s.db.open_sessions().unwrap().is_empty());
        assert_eq!(s.db.get_game(s.game.id).unwrap().play_count, 0);
        assert!(matches!(
            s.events.lock().unwrap()[0],
            CoreEvent::GameError { .. }
        ));
    }

    #[test]
    fn shutdown_saves_running_session_and_joins_threads() {
        let s = setup();
        s.probe.running.store(true, Ordering::SeqCst);
        s.launcher.start_tracking(
            s.game.id,
            s.game.name.clone(),
            Matcher::directory("/games/fake"),
            None,
        );
        wait_until(|| s.db.open_sessions().unwrap().len() == 1);
        std::thread::sleep(Duration::from_millis(100));
        s.launcher.shutdown();
        assert!(s.launcher.running().is_empty());
        assert!(s.db.open_sessions().unwrap().is_empty());
        assert_eq!(s.db.get_game(s.game.id).unwrap().play_count, 1);
    }

    #[test]
    fn launch_rejects_missing_action_and_duplicates() {
        let s = setup();
        let mut no_action = s.game.clone();
        no_action.play_action = None;
        assert!(matches!(
            s.launcher.launch(&no_action),
            Err(CoreError::Launch(_))
        ));

        s.probe.running.store(true, Ordering::SeqCst);
        s.launcher.start_tracking(
            s.game.id,
            s.game.name.clone(),
            Matcher::directory("/games/fake"),
            None,
        );
        assert!(matches!(
            s.launcher.launch(&s.game),
            Err(CoreError::Launch(_))
        ));
        s.launcher.shutdown();
    }

    #[test]
    fn launch_reports_missing_executable() {
        let s = setup();
        let mut game = s.game.clone();
        game.play_action = Some(PlayAction::file("/definitely/not/here/game.exe"));
        let err = s.launcher.launch(&game).unwrap_err().to_string();
        assert!(err.contains("Could not start"), "{err}");
    }

    /// End-to-end with a real short-lived process and the real process probe.
    #[test]
    fn tracks_real_process_tree() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        #[cfg(windows)]
        let (exe, args) = ("ping.exe", "-n 3 127.0.0.1");
        #[cfg(not(windows))]
        let (exe, args) = ("sleep", "2");
        let exe = which(exe);
        let game = db
            .insert_game(
                &NewGameRecord {
                    name: "Sleeper".into(),
                    source: "test".into(),
                    play_action: Some(PlayAction {
                        args: Some(args.into()),
                        // The exe lives in a shared system folder: unrelated processes there
                        // must not keep the session alive.
                        tracking: TrackingMode::Default,
                        ..PlayAction::file(exe)
                    }),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap();
        let launcher = Launcher::with_probe(
            db.clone(),
            EventBus::new(),
            Arc::new(SystemProbe::new()),
            LauncherConfig {
                poll_interval: Duration::from_millis(100),
                ..fast()
            },
        );
        launcher.launch(&game).unwrap();
        wait_until(|| launcher.running().is_empty());
        let game = db.get_game(game.id).unwrap();
        assert_eq!(game.play_count, 1);
        assert!(game.playtime_seconds >= 1, "{}", game.playtime_seconds);
    }

    fn which(exe: &str) -> String {
        let path = std::env::var_os("PATH").unwrap();
        std::env::split_paths(&path)
            .map(|dir| dir.join(exe))
            .find(|candidate| candidate.is_file())
            .unwrap()
            .display()
            .to_string()
    }
}
