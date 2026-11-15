//! Game scripts: programs or scripts run before a game started from Questkeep starts (pre), after
//! it closes (post), or on demand from its page (actions). Global scripts run for every game (or
//! the games matching their filter) in the user's order; a game's own scripts run at the place of
//! the "Game's own scripts" row. Inline scripts are files in `<data>/scripts`.

pub mod runtimes;
pub mod variables;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::db::Database;
use crate::error::{CoreError, Result};
use crate::events::{CoreEvent, EventBus, OutputStream};
use crate::images::ImageCache;
use crate::launcher::{LaunchOutcome, ProcInfo, ProcessProbe, SystemProbe};
use crate::model::{
    Game, GameScripts, GlobalScripts, PreTiming, RuntimeId, RuntimeInfo, Script, ScriptInput,
    ScriptKind, ScriptSlot, ScriptSource, ScriptVariable,
};
use crate::notifications::{
    NewNotification, NotificationAction, NotificationSource, Notifications,
};
use crate::paths::DataPaths;
use crate::platform;
use crate::providers::ProviderRegistry;

/// How many script output logs are kept.
const KEEP_LOGS: usize = 50;
const POLL: Duration = Duration::from_millis(100);
/// How long the end of a run from the script dialog waits for output still in the pipes (programs
/// the script started may keep them open).
const OUTPUT_GRACE: Duration = Duration::from_secs(1);

/// A run from the script dialog, which can be stopped.
struct DebugRun {
    child: Mutex<Child>,
    stopped: AtomicBool,
}

/// A script's command with its environment and folder, ready to start.
struct Prepared {
    command: Command,
    /// The game as JSON for the script (removed when the run ends).
    json_path: PathBuf,
}

struct Inner {
    db: Arc<Database>,
    paths: DataPaths,
    images: ImageCache,
    providers: ProviderRegistry,
    notifications: Notifications,
    events: EventBus,
    runtimes: Mutex<Option<Vec<RuntimeInfo>>>,
    debug_runs: Mutex<HashMap<Uuid, Arc<DebugRun>>>,
}

/// Saved scripts and running them. Cheap to clone.
#[derive(Clone)]
pub struct Scripts {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Scripts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scripts").finish_non_exhaustive()
    }
}

/// The scripts of one launch, in run order.
#[derive(Debug, Clone, Default)]
pub struct LaunchScripts {
    pub before_start: Vec<Script>,
    pub once_running: Vec<Script>,
    pub post: Vec<Script>,
}

impl Scripts {
    pub fn new(
        db: Arc<Database>,
        paths: DataPaths,
        images: ImageCache,
        providers: ProviderRegistry,
        notifications: Notifications,
        events: EventBus,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                paths,
                images,
                providers,
                notifications,
                events,
                runtimes: Mutex::new(None),
                debug_runs: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn db(&self) -> &Database {
        &self.inner.db
    }

    // ----- runtimes -----

    /// The runtimes on this PC (found once; `refresh` looks again).
    pub fn runtimes(&self, refresh: bool) -> Vec<RuntimeInfo> {
        let mut cached = self
            .inner
            .runtimes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if refresh || cached.is_none() {
            *cached = Some(runtimes::detect());
        }
        cached.clone().unwrap_or_default()
    }

    fn runtime(&self, id: RuntimeId) -> Option<RuntimeInfo> {
        self.runtimes(false).into_iter().find(|r| r.id == id)
    }

    // ----- files -----

    /// The file of an inline script.
    pub fn script_file(&self, script: &Script) -> Option<PathBuf> {
        let runtime = script.spec.runtime?;
        Some(
            self.inner
                .paths
                .scripts_dir()
                .join(format!("{}.{}", script.id, runtime.extension())),
        )
    }

    fn with_content(&self, mut script: Script) -> Script {
        if script.spec.source == ScriptSource::Inline {
            script.spec.content = self
                .script_file(&script)
                .and_then(|f| std::fs::read_to_string(f).ok());
        }
        script
    }

    /// Removes script files whose script is gone (a deleted game's scripts go with it).
    pub fn clean_up_files(&self) {
        let Ok(ids) = self.db().script_ids() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(self.inner.paths.scripts_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| Uuid::parse_str(s).ok());
            if id.is_some_and(|id| !ids.contains(&id))
                && let Err(err) = std::fs::remove_file(&path)
            {
                tracing::warn!(%err, path = %path.display(), "could not remove a script file");
            }
        }
    }

    /// Reads a script file to import it (its text goes into the editor).
    pub fn read_file(&self, path: &Path) -> Result<String> {
        let bytes = std::fs::read(path)?;
        if bytes.len() > 1024 * 1024 {
            return Err(CoreError::Invalid(
                "That file is too large for a script (over 1 MB).".into(),
            ));
        }
        String::from_utf8(bytes)
            .map_err(|_| CoreError::Invalid("That file isn't a text file.".into()))
    }

    // ----- saved scripts -----

    pub fn global(&self) -> Result<GlobalScripts> {
        Ok(GlobalScripts {
            scripts: self
                .db()
                .global_scripts()?
                .into_iter()
                .map(|s| self.with_content(s))
                .collect(),
            pre_order: self.db().global_order(ScriptKind::Pre)?,
            post_order: self.db().global_order(ScriptKind::Post)?,
            games_with_pre_scripts: self.db().games_with_scripts(ScriptKind::Pre)?,
            games_with_post_scripts: self.db().games_with_scripts(ScriptKind::Post)?,
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Script> {
        Ok(self.with_content(self.db().get_script(id)?))
    }

    /// The scripts of a game: its own, and all that apply in run order.
    pub fn for_game(&self, id: Uuid) -> Result<GameScripts> {
        let own = self
            .db()
            .own_scripts(id)?
            .into_iter()
            .map(|s| self.with_content(s))
            .collect();
        let mut applied = Vec::new();
        for kind in [ScriptKind::Pre, ScriptKind::Post, ScriptKind::Action] {
            applied.extend(self.resolve(id, kind)?);
        }
        Ok(GameScripts {
            own,
            run_global: self.db().runs_global_scripts(id)?,
            applied,
        })
    }

    /// Creates or replaces a script (and its file when inline).
    pub fn save(&self, input: &ScriptInput) -> Result<Script> {
        let before = input.id.and_then(|id| self.db().get_script(id).ok());
        if input.spec.source == ScriptSource::Inline && input.spec.runtime.is_none() {
            return Err(CoreError::Invalid(
                "Choose what runs the script (e.g. PowerShell).".into(),
            ));
        }
        let saved = self.db().save_script(input)?;
        // The file of the old runtime (another extension) or an inline script no longer inline.
        if let Some(old) = before.as_ref().and_then(|b| self.script_file(b))
            && Some(&old) != self.script_file(&saved).as_ref()
        {
            let _ = std::fs::remove_file(old);
        }
        if saved.spec.source == ScriptSource::Inline
            && let Some(file) = self.script_file(&saved)
        {
            std::fs::create_dir_all(self.inner.paths.scripts_dir())?;
            let content = input.spec.content.clone().unwrap_or_default();
            std::fs::write(file, content)?;
        }
        self.changed();
        self.get(saved.id)
    }

    pub fn delete(&self, id: Uuid) -> Result<()> {
        let script = self.db().get_script(id)?;
        self.db().delete_script(id)?;
        if let Some(file) = self.script_file(&script) {
            let _ = std::fs::remove_file(file);
        }
        self.changed();
        Ok(())
    }

    pub fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<()> {
        self.db().set_script_enabled(id, enabled)?;
        self.changed();
        Ok(())
    }

    pub fn set_global_order(&self, kind: ScriptKind, order: &[ScriptSlot]) -> Result<()> {
        self.db().set_global_order(kind, order)?;
        self.changed();
        Ok(())
    }

    pub fn set_own_order(&self, ids: &[Uuid]) -> Result<()> {
        self.db().set_own_order(ids)?;
        self.changed();
        Ok(())
    }

    pub fn set_run_global(&self, game: Uuid, run: bool) -> Result<()> {
        self.db().set_runs_global_scripts(game, run)?;
        self.changed();
        Ok(())
    }

    fn changed(&self) {
        self.inner.events.publish(CoreEvent::ScriptsChanged);
    }

    // ----- what runs -----

    /// The enabled scripts of `kind` that apply to the entry of `game`, in run order.
    pub fn resolve(&self, game: Uuid, kind: ScriptKind) -> Result<Vec<Script>> {
        let db = self.db();
        let own: Vec<Script> = db
            .own_scripts(game)?
            .into_iter()
            .filter(|s| s.kind == kind && s.enabled)
            .collect();
        let global: Vec<Script> = if db.runs_global_scripts(game)? {
            let mut list = Vec::new();
            for script in db.global_scripts()? {
                if script.kind != kind || !script.enabled {
                    continue;
                }
                let applies = match &script.filter {
                    Some(filter) => db.entry_matches(filter, game)?,
                    None => true,
                };
                if applies {
                    list.push(script);
                }
            }
            list
        } else {
            Vec::new()
        };
        if kind == ScriptKind::Action {
            return Ok(own.into_iter().chain(global).collect());
        }
        let slot = db.game_scripts_slot(kind)?;
        let (before, after): (Vec<Script>, Vec<Script>) =
            global.into_iter().partition(|s| s.position < slot);
        Ok(before.into_iter().chain(own).chain(after).collect())
    }

    /// The pre and post scripts of a launch of `game` (a copy, with its profile applied).
    pub fn for_launch(&self, game: &Game) -> Result<LaunchScripts> {
        let (once_running, before_start) = self
            .resolve(game.id, ScriptKind::Pre)?
            .into_iter()
            .partition(|s| s.spec.timing == PreTiming::OnceRunning);
        Ok(LaunchScripts {
            before_start,
            once_running,
            post: self.resolve(game.id, ScriptKind::Post)?,
        })
    }

    // ----- variables -----

    fn entry(&self, game: Uuid) -> Result<Game> {
        let mut entry = self.db().get_entry(game)?;
        let resolve = |p: Option<String>| {
            p.as_deref()
                .and_then(|p| self.inner.images.resolve(p))
                .map(|p| p.display().to_string())
        };
        entry.cover_path = resolve(entry.cover_path.take());
        entry.icon_path = resolve(entry.icon_path.take());
        entry.background_path = resolve(entry.background_path.take());
        Ok(entry)
    }

    fn category_names(&self, entry: &Game) -> Vec<String> {
        let all = self.db().list_categories().unwrap_or_default();
        entry
            .categories
            .iter()
            .filter_map(|id| all.iter().find(|c| &c.id == id).map(|c| c.name.clone()))
            .collect()
    }

    fn store_name(&self, source: &str) -> String {
        self.inner
            .providers
            .get(source)
            .map_or_else(|| source.to_string(), |p| p.name().to_string())
    }

    /// The variables a script of `kind` gets for `game` (values as they are now), for the
    /// script dialog's panel.
    pub fn variables(&self, game: Uuid, kind: ScriptKind) -> Result<Vec<ScriptVariable>> {
        let entry = self.entry(game)?;
        let launched = self.db().get_game(
            entry
                .copies
                .iter()
                .find(|c| c.installed)
                .or(entry.copies.first())
                .map_or(entry.id, |c| c.id),
        )?;
        let categories = self.category_names(&entry);
        let store_name = self.store_name(&entry.source);
        let data_dir = self.inner.paths.root().to_path_buf();
        Ok(variables::variables(&variables::Context {
            entry: &entry,
            launched: Some(&launched),
            phase: kind,
            script_name: "",
            script_file: None,
            data_dir: &data_dir,
            game_json: None,
            categories: &categories,
            store_name: &store_name,
            outcome: None,
        }))
    }

    // ----- running -----

    /// Runs an action script for a game now (also "Run now" in the script dialog, for any
    /// kind). Waits only when the script says so; failures are reported as notifications.
    pub fn run_action(&self, script: Uuid, game: Uuid) -> Result<()> {
        let script = self.get(script)?;
        let this = self.clone();
        std::thread::Builder::new()
            .name("script-action".into())
            .spawn(move || {
                let launched = this.db().get_game(game).ok();
                let _ = this.run(&script, game, launched.as_ref(), ScriptKind::Action, None);
            })
            .map_err(|e| CoreError::Invalid(format!("The script could not start: {e}")))?;
        Ok(())
    }

    /// Runs a script as edited in the script dialog, before it is saved ("Run now"), for `game`,
    /// to debug it: its output arrives as [`CoreEvent::ScriptOutput`] and its end as
    /// [`CoreEvent::ScriptRunEnded`], both tagged with `run` (chosen by the caller, so it can
    /// listen before the run starts). The run always waits for the script and has no timeout;
    /// [`Scripts::stop_run`] ends it. Nothing is posted to the notifications. An inline script's
    /// text is written to a file of its own (removed when the run ends).
    pub fn run_draft(&self, input: &ScriptInput, game: Uuid, run: Uuid) -> Result<()> {
        let script = Script {
            id: Uuid::new_v4(),
            name: input.name.trim().to_string(),
            kind: input.kind,
            enabled: true,
            game_id: None,
            filter: None,
            spec: input.spec.clone(),
            position: 0,
        };
        if script.spec.source == ScriptSource::Inline {
            let file = self.script_file(&script).ok_or_else(|| {
                CoreError::Invalid("Choose what runs the script (e.g. PowerShell).".into())
            })?;
            std::fs::create_dir_all(self.inner.paths.scripts_dir())?;
            std::fs::write(file, script.spec.content.clone().unwrap_or_default())?;
        }
        let this = self.clone();
        std::thread::Builder::new()
            .name("script-draft".into())
            .spawn(move || {
                let launched = this.db().get_game(game).ok();
                let (exit_code, error) = this.debug_run(&script, game, launched.as_ref(), run);
                if script.spec.source == ScriptSource::Inline
                    && let Some(file) = this.script_file(&script)
                {
                    let _ = std::fs::remove_file(file);
                }
                this.inner.events.publish(CoreEvent::ScriptRunEnded {
                    run,
                    exit_code,
                    error,
                });
            })
            .map_err(|e| CoreError::Invalid(format!("The script could not start: {e}")))?;
        Ok(())
    }

    /// Stops a run from the script dialog, with the programs it started. False when it already
    /// ended.
    pub fn stop_run(&self, run: Uuid) -> bool {
        let found = self
            .inner
            .debug_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&run)
            .cloned();
        let Some(found) = found else {
            return false;
        };
        found.stopped.store(true, Ordering::SeqCst);
        let mut child = found.child.lock().unwrap_or_else(PoisonError::into_inner);
        // Programs the script started (a shell running `ping`) would keep going, and keep the
        // output open, if only the script's own process ended.
        let tree = descendants(child.id(), &SystemProbe::new().snapshot());
        platform::terminate(&tree);
        let killed = child.kill();
        if let Err(err) = killed {
            tracing::warn!(%err, "could not stop a script");
        }
        true
    }

    /// Runs a script with its output forwarded as events until it ends. Returns its exit code,
    /// and what went wrong when it didn't start or end normally.
    fn debug_run(
        &self,
        script: &Script,
        game: Uuid,
        launched: Option<&Game>,
        run: Uuid,
    ) -> (Option<i32>, Option<String>) {
        let Prepared {
            mut command,
            json_path,
        } = match self.prepare(script, game, launched, script.kind, None) {
            Ok(prepared) => prepared,
            Err(message) => return (None, Some(message)),
        };
        // The output shows in the dialog, so no console window opens.
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        platform::hide_console(&mut command);
        tracing::info!(script = %script.name, "running a script from the script dialog");
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = std::fs::remove_file(&json_path);
                return (
                    None,
                    Some(format!("“{}” could not start: {err}", script.name)),
                );
            }
        };
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let mut readers = 0;
        if let Some(out) = child.stdout.take() {
            readers += self.forward(run, OutputStream::Stdout, out, done_tx.clone());
        }
        if let Some(err) = child.stderr.take() {
            readers += self.forward(run, OutputStream::Stderr, err, done_tx.clone());
        }
        drop(done_tx);

        let handle = Arc::new(DebugRun {
            child: Mutex::new(child),
            stopped: AtomicBool::new(false),
        });
        let runs = &self.inner.debug_runs;
        runs.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run, handle.clone());
        let status = loop {
            let polled = handle
                .child
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .try_wait();
            match polled {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => std::thread::sleep(POLL),
                Err(err) => break Err(err),
            }
        };
        runs.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&run);
        let deadline = Instant::now() + OUTPUT_GRACE;
        for _ in 0..readers {
            let left = deadline.saturating_duration_since(Instant::now());
            if done_rx.recv_timeout(left).is_err() {
                break;
            }
        }
        let _ = std::fs::remove_file(&json_path);
        match status {
            Ok(_) if handle.stopped.load(Ordering::SeqCst) => {
                (None, Some(format!("“{}” was stopped.", script.name)))
            }
            Ok(status) => (
                status.code(),
                status
                    .code()
                    .is_none()
                    .then(|| format!("“{}” was stopped.", script.name)),
            ),
            Err(err) => (None, Some(format!("Waiting for the script failed: {err}"))),
        }
    }

    /// Publishes what a program writes to one of its outputs, line by line, on a thread of its
    /// own; `done` hears when the output closed. Returns how many threads started (0 or 1).
    fn forward(
        &self,
        run: Uuid,
        stream: OutputStream,
        output: impl Read + Send + 'static,
        done: mpsc::Sender<()>,
    ) -> usize {
        let events = self.inner.events.clone();
        let spawned = std::thread::Builder::new()
            .name("script-output".into())
            .spawn(move || {
                let mut reader = BufReader::new(output);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => events.publish(CoreEvent::ScriptOutput {
                            run,
                            stream,
                            text: String::from_utf8_lossy(&line).into_owned(),
                        }),
                    }
                }
                let _ = done.send(());
            });
        match spawned {
            Ok(_) => 1,
            Err(err) => {
                tracing::warn!(%err, "could not read a script's output");
                0
            }
        }
    }

    /// Runs one script for `game` and, when it waits, until it ends or times out. `Err` carries
    /// the message of a failure (already posted as a notification).
    pub fn run(
        &self,
        script: &Script,
        game: Uuid,
        launched: Option<&Game>,
        phase: ScriptKind,
        outcome: Option<&LaunchOutcome>,
    ) -> std::result::Result<(), String> {
        let failed = |message: String, log: Option<&Path>| {
            let mut note = NewNotification::error(
                NotificationSource::Scripts,
                match phase {
                    ScriptKind::Pre => "Pre script failed",
                    ScriptKind::Post => "Post script failed",
                    ScriptKind::Action => "Script failed",
                },
                message.clone(),
            )
            .game(game);
            note = match log {
                Some(log) => note.action(NotificationAction::OpenFile {
                    path: log.display().to_string(),
                }),
                None => note.action(NotificationAction::EditScript { id: script.id }),
            };
            self.inner.notifications.post(note);
            message
        };
        let Prepared {
            mut command,
            json_path,
        } = self
            .prepare(script, game, launched, phase, outcome)
            .map_err(|m| failed(m, None))?;
        let log = (!script.spec.options.show_window)
            .then(|| self.log_file(&script.name))
            .flatten();
        match log
            .as_ref()
            .and_then(|(_, f)| f.try_clone().ok().zip(f.try_clone().ok()))
        {
            Some((out, err)) => {
                command.stdout(out).stderr(err);
            }
            None => {
                command.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        if !script.spec.options.show_window {
            platform::hide_console(&mut command);
        }
        let log_path = log.map(|(p, _)| p);
        tracing::info!(script = %script.name, phase = phase.as_str(), "running a game script");
        let child = command.spawn().map_err(|e| {
            let _ = std::fs::remove_file(&json_path);
            failed(format!("“{}” could not start: {e}", script.name), None)
        })?;

        let wait = script.spec.options.wait;
        let timeout = match script.spec.options.timeout_seconds {
            0 => None,
            s => Some(Duration::from_secs(u64::from(s))),
        };
        let name = script.name.clone();
        let finish =
            move |status: Option<std::process::ExitStatus>| -> std::result::Result<(), String> {
                let _ = std::fs::remove_file(&json_path);
                match status {
                    Some(status) if !status.success() => Err(match status.code() {
                        Some(code) => format!("“{name}” ended with exit code {code}."),
                        None => format!("“{name}” was stopped."),
                    }),
                    _ => Ok(()),
                }
            };
        if wait {
            match wait_for(child, timeout) {
                Waited::Ended(status) => {
                    finish(Some(status)).map_err(|m| failed(m, log_path.as_deref()))
                }
                Waited::TimedOut(child) => {
                    tracing::info!(script = %script.name, "stopped waiting for a script");
                    self.reap(child, finish, log_path, phase, script.id, game);
                    Ok(())
                }
                Waited::Failed(err) => Err(failed(err, log_path.as_deref())),
            }
        } else {
            self.reap(child, finish, log_path, phase, script.id, game);
            Ok(())
        }
    }

    /// The command for a script with its arguments, environment and working folder, and the game
    /// JSON file it gets.
    fn prepare(
        &self,
        script: &Script,
        game: Uuid,
        launched: Option<&Game>,
        phase: ScriptKind,
        outcome: Option<&LaunchOutcome>,
    ) -> std::result::Result<Prepared, String> {
        let entry = self.entry(game).map_err(|e| e.to_string())?;
        let launched = launched.unwrap_or(&entry);
        let categories = self.category_names(&entry);
        let store_name = self.store_name(&entry.source);
        let data_dir = self.inner.paths.root().to_path_buf();
        let script_file = match script.spec.source {
            ScriptSource::Inline => self.script_file(script),
            ScriptSource::Linked => Some(PathBuf::from(script.spec.program.trim())),
            ScriptSource::Program => None,
        };
        let json_path =
            std::env::temp_dir().join(format!("questkeep-game-{}.json", Uuid::new_v4()));
        let ctx = variables::Context {
            entry: &entry,
            launched: Some(launched),
            phase,
            script_name: &script.name,
            script_file: script_file.as_deref(),
            data_dir: &data_dir,
            game_json: Some(&json_path),
            categories: &categories,
            store_name: &store_name,
            outcome,
        };
        let vars = variables::variables(&ctx);
        if let Err(err) = serde_json::to_vec_pretty(&variables::game_json(&ctx, &vars))
            .map_err(std::io::Error::other)
            .and_then(|bytes| std::fs::write(&json_path, bytes))
        {
            tracing::warn!(%err, "could not write the game JSON for a script");
        }

        let mut command = self
            .command(script, script_file.as_deref(), &vars)
            .map_err(|m| {
                let _ = std::fs::remove_file(&json_path);
                format!("“{}”: {m}", script.name)
            })?;
        for var in &vars {
            command.env(&var.env, &var.value);
        }
        let working_dir = script
            .spec
            .working_dir
            .as_deref()
            .filter(|w| !w.trim().is_empty())
            .map(|w| variables::expand(w, &vars))
            .or_else(|| launched.install_dir.clone().or(entry.install_dir.clone()))
            .filter(|w| Path::new(w).is_dir());
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }
        command.stdin(Stdio::null());
        Ok(Prepared { command, json_path })
    }

    /// Waits for a script nobody waits for on a thread of its own, reporting a failure.
    fn reap(
        &self,
        mut child: Child,
        finish: impl FnOnce(Option<std::process::ExitStatus>) -> std::result::Result<(), String>
        + Send
        + 'static,
        log: Option<PathBuf>,
        phase: ScriptKind,
        script: Uuid,
        game: Uuid,
    ) {
        let notifications = self.inner.notifications.clone();
        let spawned = std::thread::Builder::new()
            .name("script-wait".into())
            .spawn(move || {
                let status = child.wait().ok();
                if let Err(message) = finish(status) {
                    let note = NewNotification::error(
                        NotificationSource::Scripts,
                        match phase {
                            ScriptKind::Pre => "Pre script failed",
                            ScriptKind::Post => "Post script failed",
                            ScriptKind::Action => "Script failed",
                        },
                        message,
                    )
                    .game(game)
                    .action(match log {
                        Some(log) => NotificationAction::OpenFile {
                            path: log.display().to_string(),
                        },
                        None => NotificationAction::EditScript { id: script },
                    });
                    notifications.post(note);
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not watch a script");
        }
    }

    /// The command for a script without environment, folder and output.
    fn command(
        &self,
        script: &Script,
        file: Option<&Path>,
        vars: &[ScriptVariable],
    ) -> std::result::Result<Command, String> {
        let args = variables::expand(&script.spec.args, vars);
        let mut command = match script.spec.source {
            ScriptSource::Program => {
                let program = variables::expand(script.spec.program.trim(), vars);
                if program.is_empty() {
                    return Err("no program is set.".into());
                }
                Command::new(program)
            }
            ScriptSource::Inline | ScriptSource::Linked => {
                let file = file.ok_or("no script file is set.")?;
                if !file.is_file() {
                    return Err(format!("the script file {} is missing.", file.display()));
                }
                let runtime = script
                    .spec
                    .runtime
                    .or_else(|| runtimes::for_extension(file));
                match runtime {
                    Some(id) => {
                        let info = self.runtime(id).ok_or_else(|| {
                            format!("{} isn't installed on this PC.", runtimes::display_name(id))
                        })?;
                        runtimes::command(&info, file)
                    }
                    None => Command::new(file),
                }
            }
        };
        platform::add_raw_args(&mut command, &args);
        Ok(command)
    }

    /// A new log file for a run (old ones beyond [`KEEP_LOGS`] are removed).
    fn log_file(&self, name: &str) -> Option<(PathBuf, File)> {
        let dir = self.inner.paths.script_logs_dir();
        std::fs::create_dir_all(&dir).ok()?;
        prune_logs(&dir);
        let safe: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .take(40)
            .collect();
        let path = dir.join(format!(
            "{}-{safe}.log",
            chrono::Local::now().format("%Y%m%d-%H%M%S%.3f")
        ));
        File::create(&path).ok().map(|f| (path, f))
    }
}

/// The processes started by `root`, directly or through others.
fn descendants(root: u32, processes: &[ProcInfo]) -> Vec<u32> {
    let mut found = Vec::new();
    let mut parents = vec![root];
    while let Some(parent) = parents.pop() {
        for p in processes {
            if p.parent == Some(parent) && p.pid != root && !found.contains(&p.pid) {
                found.push(p.pid);
                parents.push(p.pid);
            }
        }
    }
    found
}

fn prune_logs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .collect();
    if logs.len() < KEEP_LOGS {
        return;
    }
    logs.sort();
    for old in &logs[..=logs.len() - KEEP_LOGS] {
        let _ = std::fs::remove_file(old);
    }
}

enum Waited {
    Ended(std::process::ExitStatus),
    TimedOut(Child),
    Failed(String),
}

fn wait_for(mut child: Child, timeout: Option<Duration>) -> Waited {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Waited::Ended(status),
            Ok(None) => {}
            Err(err) => return Waited::Failed(format!("Waiting for the script failed: {err}")),
        }
        if timeout.is_some_and(|t| started.elapsed() >= t) {
            return Waited::TimedOut(child);
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests;
