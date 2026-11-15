//! Closing a store's launcher after playing ("Close launcher after playing" per store).
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: wait a while after the game
//! closes (cloud saves), skip short sessions and launchers another game of the store still uses,
//! and close each launcher the way its store says.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use questkeep_store_api::model::{ActionKind, LauncherClose, LauncherProcess};
use uuid::Uuid;

use crate::db::Database;
use crate::launcher::{LaunchOutcome, ProcInfo, ProcessProbe, RunningGames};
use crate::notifications::{
    NewNotification, NotificationAction, NotificationSource, Notifications,
};
use crate::platform;
use crate::settings::{LauncherClosePolicy, Settings};

/// How long a launcher asked to close may take before it is ended by force.
const CLOSE_GRACE: Duration = Duration::from_secs(120);
const CLOSE_POLL: Duration = Duration::from_secs(2);

/// What a launch needs to close its store's launcher afterwards.
#[derive(Debug, Clone)]
pub(super) struct LauncherPlan {
    pub store: String,
    pub store_name: String,
    pub process: LauncherProcess,
    pub policy: LauncherClosePolicy,
    /// The launcher ran when the game was started.
    pub was_running: bool,
    pub delay: Duration,
    pub min_session: Duration,
}

impl LauncherPlan {
    /// The plan for a game of `store`, or `None` when nothing will be closed.
    pub fn new(
        settings: &Settings,
        store: &str,
        store_name: &str,
        process: Option<LauncherProcess>,
        probe: &dyn ProcessProbe,
    ) -> Option<Self> {
        let policy = settings
            .launcher_close
            .get(store)
            .copied()
            .unwrap_or_default();
        if policy == LauncherClosePolicy::Never {
            return None;
        }
        let process = process?;
        let was_running = !pids_of(&probe.snapshot(), &process.process_names).is_empty();
        Some(Self {
            store: store.to_string(),
            store_name: store_name.to_string(),
            process,
            policy,
            was_running,
            delay: Duration::from_secs(u64::from(settings.launcher_close_delay_seconds)),
            min_session: Duration::from_secs(u64::from(settings.launcher_close_min_minutes) * 60),
        })
    }

    /// Whether the launcher is to be closed after this outcome (before the wait and the check
    /// for other games of the store).
    pub fn wants_close(&self, outcome: &LaunchOutcome) -> bool {
        outcome.ran
            && Duration::from_secs(outcome.seconds) >= self.min_session
            && match self.policy {
                LauncherClosePolicy::Never => false,
                LauncherClosePolicy::Always => true,
                LauncherClosePolicy::IfStartedByGame => !self.was_running,
            }
    }
}

/// What closing needs from the app.
#[derive(Clone)]
pub(super) struct CloseContext {
    pub db: Arc<Database>,
    pub probe: Arc<dyn ProcessProbe>,
    pub running: RunningGames,
    pub notifications: Notifications,
    /// The launched game, which no longer counts as using the launcher.
    pub game_id: Uuid,
}

/// Waits `plan.delay` on a thread of its own, then closes the launcher unless another game of
/// the store runs.
pub(super) fn schedule(plan: LauncherPlan, context: CloseContext) {
    let spawned = std::thread::Builder::new()
        .name(format!("close-{}", plan.store))
        .spawn(move || {
            std::thread::sleep(plan.delay);
            close_now(&plan, &context);
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not schedule closing the launcher");
    }
}

fn close_now(plan: &LauncherPlan, context: &CloseContext) {
    let busy = (context.running)().into_iter().any(|id| {
        id != context.game_id
            && context
                .db
                .get_game(id)
                .is_ok_and(|game| game.source == plan.store)
    });
    if busy {
        tracing::info!(store = %plan.store, "not closing the launcher: another of its games runs");
        return;
    }
    let pids = pids_of(&context.probe.snapshot(), &plan.process.process_names);
    if pids.is_empty() {
        tracing::debug!(store = %plan.store, "launcher already closed");
        return;
    }
    tracing::info!(store = %plan.store, ?pids, "closing the launcher after playing");
    let warn = |message: String| {
        context.notifications.post(
            NewNotification::warning(
                NotificationSource::Launchers,
                "Launcher not closed",
                message,
            )
            .action(NotificationAction::StoreSettings {
                store: plan.store.clone(),
            }),
        );
    };
    match &plan.process.close {
        LauncherClose::Command(action) => {
            if let Err(err) = run_action(action) {
                warn(format!(
                    "{} could not be asked to close: {err}",
                    plan.store_name
                ));
            }
        }
        LauncherClose::ForceQuit => platform::terminate(&pids),
        LauncherClose::CloseWindows { process_names } => {
            let names = if process_names.is_empty() {
                &plan.process.process_names
            } else {
                process_names
            };
            platform::close_windows(&pids_of(&context.probe.snapshot(), names));
            let asked = Instant::now();
            loop {
                std::thread::sleep(CLOSE_POLL);
                let left = pids_of(&context.probe.snapshot(), &plan.process.process_names);
                if left.is_empty() {
                    return;
                }
                if asked.elapsed() >= CLOSE_GRACE {
                    platform::terminate(&left);
                    warn(format!(
                        "{} did not close when asked, so it was ended by force.",
                        plan.store_name
                    ));
                    return;
                }
            }
        }
    }
}

/// Runs a store's close command (an executable with arguments, or a URI).
fn run_action(action: &questkeep_store_api::model::PlayAction) -> std::io::Result<()> {
    match action.kind {
        ActionKind::Uri => platform::open_uri(&action.target),
        ActionKind::File => {
            let mut command = Command::new(&action.target);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Some(args) = action.args.as_deref() {
                platform::add_raw_args(&mut command, args);
            }
            platform::hide_console(&mut command);
            command.spawn().map(drop)
        }
    }
}

/// Ids of the processes whose executable file name is one of `names` (case-insensitive).
pub(super) fn pids_of(snapshot: &[ProcInfo], names: &[String]) -> Vec<u32> {
    snapshot
        .iter()
        .filter(|p| names.iter().any(|n| n.eq_ignore_ascii_case(&p.name)))
        .map(|p| p.pid)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use chrono::Utc;

    use super::*;

    struct Snapshot(Mutex<Vec<ProcInfo>>);
    impl ProcessProbe for Snapshot {
        fn snapshot(&self) -> Vec<ProcInfo> {
            self.0.lock().unwrap().clone()
        }
    }

    fn proc(pid: u32, name: &str) -> ProcInfo {
        ProcInfo {
            pid,
            parent: None,
            exe: None,
            name: name.into(),
            hint_paths: Vec::new(),
        }
    }

    fn process() -> LauncherProcess {
        LauncherProcess {
            process_names: vec!["Steam.exe".into()],
            close: LauncherClose::ForceQuit,
        }
    }

    fn settings(policy: LauncherClosePolicy) -> Settings {
        Settings {
            launcher_close: BTreeMap::from([("steam".into(), policy)]),
            ..Settings::default()
        }
    }

    fn outcome(seconds: u64) -> LaunchOutcome {
        LaunchOutcome {
            ran: true,
            tracked: true,
            seconds,
            started_at: Some(Utc::now()),
            ended_at: Utc::now(),
        }
    }

    #[test]
    fn follows_the_policy_and_the_minimum_session() {
        let running = Snapshot(Mutex::new(vec![proc(7, "steam.exe")]));
        let idle = Snapshot(Mutex::new(Vec::new()));
        let plan = |policy, probe: &Snapshot| {
            LauncherPlan::new(&settings(policy), "steam", "Steam", Some(process()), probe)
        };

        assert!(plan(LauncherClosePolicy::Never, &running).is_none());
        assert!(
            LauncherPlan::new(
                &settings(LauncherClosePolicy::Always),
                "steam",
                "Steam",
                None,
                &running
            )
            .is_none()
        );

        let always = plan(LauncherClosePolicy::Always, &running).unwrap();
        assert!(always.was_running);
        assert!(always.wants_close(&outcome(600)));
        assert!(!always.wants_close(&outcome(30)), "shorter than 2 minutes");
        assert!(!always.wants_close(&LaunchOutcome::not_run()));

        let if_started = plan(LauncherClosePolicy::IfStartedByGame, &running).unwrap();
        assert!(!if_started.wants_close(&outcome(600)));
        let if_started = plan(LauncherClosePolicy::IfStartedByGame, &idle).unwrap();
        assert!(if_started.wants_close(&outcome(600)));
    }

    #[test]
    fn matches_process_names_without_case() {
        let snapshot = [proc(1, "STEAM.EXE"), proc(2, "other.exe")];
        assert_eq!(pids_of(&snapshot, &["steam.exe".into()]), vec![1]);
    }
}
