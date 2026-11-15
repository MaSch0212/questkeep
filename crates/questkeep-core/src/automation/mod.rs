//! What happens around launches from Questkeep: game scripts before and after the game, and
//! closing the store's launcher afterwards. [`Automation`] is the launcher's [`LaunchHooks`].

mod launcher_close;

use std::sync::Arc;

use crate::db::Database;
use crate::launcher::{
    LaunchHooks, LaunchOutcome, LaunchSession, ProcessProbe, RunningGames, SystemProbe,
};
use crate::model::{Game, ScriptKind};
use crate::notifications::Notifications;
use crate::providers::ProviderRegistry;
use crate::scripts::{LaunchScripts, Scripts};
use crate::settings::SettingsStore;

use launcher_close::{CloseContext, LauncherPlan};

/// Shared by every launch.
struct Inner {
    db: Arc<Database>,
    providers: ProviderRegistry,
    settings: SettingsStore,
    notifications: Notifications,
    scripts: Option<Scripts>,
    probe: Arc<dyn ProcessProbe>,
    running: RunningGames,
}

/// The launcher's hooks. Cheap to clone.
#[derive(Clone)]
pub struct Automation {
    inner: Arc<Inner>,
}

impl Automation {
    pub fn new(
        db: Arc<Database>,
        providers: ProviderRegistry,
        settings: SettingsStore,
        notifications: Notifications,
        scripts: Option<Scripts>,
        running: RunningGames,
    ) -> Self {
        Self::with_probe(
            db,
            providers,
            settings,
            notifications,
            scripts,
            running,
            Arc::new(SystemProbe::new()),
        )
    }

    pub fn with_probe(
        db: Arc<Database>,
        providers: ProviderRegistry,
        settings: SettingsStore,
        notifications: Notifications,
        scripts: Option<Scripts>,
        running: RunningGames,
        probe: Arc<dyn ProcessProbe>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                providers,
                settings,
                notifications,
                scripts,
                probe,
                running,
            }),
        }
    }
}

impl LaunchHooks for Automation {
    fn begin(&self, game: &Game) -> Box<dyn LaunchSession> {
        let inner = &self.inner;
        let settings = inner.settings.load();
        let provider = inner.providers.get(&game.source);
        let launcher = provider.as_ref().and_then(|p| {
            LauncherPlan::new(
                &settings,
                &game.source,
                p.name(),
                p.launcher_process(),
                inner.probe.as_ref(),
            )
        });
        let scripts = inner
            .scripts
            .as_ref()
            .map(|s| match s.for_launch(game) {
                Ok(plan) => plan,
                Err(err) => {
                    tracing::warn!(%err, "could not read the game's scripts");
                    LaunchScripts::default()
                }
            })
            .unwrap_or_default();
        Box::new(Session {
            inner: self.inner.clone(),
            game: game.clone(),
            launcher,
            scripts,
            pre_ran: false,
        })
    }
}

/// One launch.
struct Session {
    inner: Arc<Inner>,
    game: Game,
    launcher: Option<LauncherPlan>,
    scripts: LaunchScripts,
    /// A pre script ran, so the post scripts run whatever happens to the game.
    pre_ran: bool,
}

impl LaunchSession for Session {
    fn has_work_before_start(&self) -> bool {
        !self.scripts.before_start.is_empty()
    }

    fn before_start(&mut self) -> Result<(), String> {
        let Some(runner) = self.inner.scripts.clone() else {
            return Ok(());
        };
        for script in std::mem::take(&mut self.scripts.before_start) {
            self.pre_ran = true;
            let result = runner.run(
                &script,
                self.game.id,
                Some(&self.game),
                ScriptKind::Pre,
                None,
            );
            if let Err(message) = result
                && script.spec.options.stop_launch_on_failure
            {
                return Err(format!(
                    "{} was not started because a script failed: {message}",
                    self.game.name
                ));
            }
        }
        Ok(())
    }

    fn started(&mut self) {
        let scripts = std::mem::take(&mut self.scripts.once_running);
        let Some(runner) = self.inner.scripts.clone() else {
            return;
        };
        if scripts.is_empty() {
            return;
        }
        self.pre_ran = true;
        // On a thread of its own: the tracker must keep its pace while they run.
        let game = self.game.clone();
        let spawned = std::thread::Builder::new()
            .name("scripts-running".into())
            .spawn(move || {
                for script in scripts {
                    let _ = runner.run(&script, game.id, Some(&game), ScriptKind::Pre, None);
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not run the scripts for a running game");
        }
    }

    fn ended(&mut self, outcome: &LaunchOutcome) {
        let post = std::mem::take(&mut self.scripts.post);
        if (self.pre_ran || outcome.ran || !outcome.tracked)
            && let Some(runner) = self.inner.scripts.clone()
        {
            for script in post {
                let _ = runner.run(
                    &script,
                    self.game.id,
                    Some(&self.game),
                    ScriptKind::Post,
                    Some(outcome),
                );
            }
        }
        if let Some(plan) = self.launcher.take()
            && plan.wants_close(outcome)
        {
            launcher_close::schedule(
                plan,
                CloseContext {
                    db: self.inner.db.clone(),
                    probe: self.inner.probe.clone(),
                    running: self.inner.running.clone(),
                    notifications: self.inner.notifications.clone(),
                    game_id: self.game.id,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::auth::MemoryTokenStore;
    use crate::db::NewGameRecord;
    use crate::events::EventBus;
    use crate::images::ImageCache;
    use crate::launcher::ProcInfo;
    use crate::model::{ScriptInput, ScriptOptions, ScriptSpec};
    use crate::paths::DataPaths;

    struct NoProcesses;
    impl ProcessProbe for NoProcesses {
        fn snapshot(&self) -> Vec<ProcInfo> {
            Vec::new()
        }
    }

    fn writes(file: &std::path::Path, text: &str, stop: bool) -> ScriptSpec {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe",
                format!("/d /c \"echo {text}>> {}\"", file.display()),
            )
        } else {
            ("sh", format!("-c 'echo {text} >> {}'", file.display()))
        };
        ScriptSpec {
            program: program.into(),
            args,
            options: ScriptOptions {
                stop_launch_on_failure: stop,
                ..ScriptOptions::default()
            },
            ..ScriptSpec::default()
        }
    }

    #[test]
    fn runs_post_scripts_after_pre_scripts_even_when_the_game_never_ran() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path());
        paths.ensure_dirs().unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let events = EventBus::new();
        let notifications = Notifications::in_memory(&events);
        let scripts = Scripts::new(
            db.clone(),
            paths.clone(),
            ImageCache::new(paths.clone()),
            ProviderRegistry::new(),
            notifications.clone(),
            events,
        );
        let game = db
            .insert_game(
                &NewGameRecord {
                    name: "Game".into(),
                    source: "steam".into(),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap();
        let log = tmp.path().join("log.txt");
        for (name, kind, text) in [
            ("EQ on", ScriptKind::Pre, "pre"),
            ("EQ off", ScriptKind::Post, "post"),
        ] {
            scripts
                .save(&ScriptInput {
                    id: None,
                    name: name.into(),
                    kind,
                    enabled: true,
                    game_id: None,
                    filter: None,
                    spec: writes(&log, text, false),
                })
                .unwrap();
        }
        let automation = Automation::with_probe(
            db.clone(),
            ProviderRegistry::new(),
            SettingsStore::new(
                paths.settings_file(),
                Arc::new(MemoryTokenStore::default()),
                Vec::new(),
            ),
            notifications,
            Some(scripts.clone()),
            Arc::new(Vec::new),
            Arc::new(NoProcesses),
        );

        let mut session = automation.begin(&game);
        assert!(session.has_work_before_start());
        session.before_start().unwrap();
        session.ended(&LaunchOutcome::not_run());
        let text = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        assert_eq!(lines, ["pre", "post"]);

        // A failing pre script that should stop the launch does.
        scripts
            .save(&ScriptInput {
                id: None,
                name: "Broken".into(),
                kind: ScriptKind::Pre,
                enabled: true,
                game_id: Some(game.id),
                filter: None,
                spec: ScriptSpec {
                    program: "Z:/not/here.exe".into(),
                    options: ScriptOptions {
                        stop_launch_on_failure: true,
                        ..ScriptOptions::default()
                    },
                    ..ScriptSpec::default()
                },
            })
            .unwrap();
        let mut session = automation.begin(&game);
        let err = session.before_start().unwrap_err();
        assert!(err.contains("was not started"), "{err}");
    }
}
