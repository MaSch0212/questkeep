//! Backups in the shell: the automatic backup after start and the guard that keeps a quit (or an
//! update install) from cutting a running backup short.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use questkeep_core::events::CoreEvent;
use questkeep_core::platform::BackgroundPriority;
use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::window;

/// How long after start the automatic backup waits, so it does not compete with the first scan.
const STARTUP_DELAY: Duration = Duration::from_secs(60);

/// What the app was about to do when a backup was still running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingQuit {
    Exit,
    Update,
}

/// The quit that waits for the user's answer ("Wait and quit" or "Quit now").
#[derive(Default)]
pub struct QuitGuard {
    pending: Mutex<Option<PendingQuit>>,
}

impl QuitGuard {
    fn set(&self, quit: Option<PendingQuit>) {
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = quit;
    }

    fn take(&self) -> Option<PendingQuit> {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    pub fn is_pending(&self) -> bool {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }
}

/// Runs the automatic backup once it is due, a while after start.
pub fn backup_on_startup(state: Arc<AppState>) {
    let spawned = std::thread::Builder::new()
        .name("startup-backup".into())
        .spawn(move || {
            std::thread::sleep(STARTUP_DELAY);
            let _priority = BackgroundPriority::begin();
            if let Some(Err(err)) = state.library.backup_if_due() {
                tracing::warn!(%err, "automatic backup failed");
            }
        });
    if let Err(err) = spawned {
        tracing::error!(%err, "could not start the backup thread");
    }
}

/// Holds `quit` back while a backup runs: the main window shows and asks the user. Returns
/// whether the quit was held back.
pub fn hold_quit(app: &AppHandle, quit: PendingQuit) -> bool {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return false;
    };
    if !state.library.backup_running() {
        return false;
    }
    if let Some(guard) = app.try_state::<QuitGuard>() {
        guard.set(Some(quit));
    }
    tracing::info!(?quit, "quit held back: a backup is running");
    window::show_main_window_async(app);
    state.library.events().publish(CoreEvent::QuitDuringBackup);
    true
}

/// Carries out the held-back quit, after the backup finished (`wait`) or after cancelling it.
pub async fn finish_quit(app: AppHandle, wait: bool) {
    let Some(quit) = app.try_state::<QuitGuard>().and_then(|g| g.take()) else {
        return;
    };
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        let state = Arc::clone(&state);
        let waited = tauri::async_runtime::spawn_blocking(move || {
            if !wait {
                state.library.cancel_backup();
            }
            state.library.wait_for_backup();
        })
        .await;
        if let Err(err) = waited {
            tracing::error!(%err, "waiting for the backup failed");
        }
    }
    match quit {
        PendingQuit::Exit => crate::tray::exit(&app),
        PendingQuit::Update => {
            if let Err(err) = crate::updates::install(&app).await {
                tracing::error!(%err, "update after the backup failed");
            }
        }
    }
}

/// The user stays: the held-back quit is dropped.
pub fn cancel_quit(app: &AppHandle) {
    if let Some(guard) = app.try_state::<QuitGuard>() {
        guard.set(None);
    }
}
