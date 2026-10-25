//! App updates: one check shortly after start (and whenever Settings checks), the result kept
//! here for the title bar and the tray, and installing it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::CmdResult;

/// Event with the newer version (`UpdateInfo`), sent when a check finds one.
pub const AVAILABLE_EVENT: &str = "update-available";
/// Event asking the window to show the update dialog (the tray's "Update" entry).
pub const SHOW_EVENT: &str = "show-update";

/// Wait after start before checking, so the check doesn't compete with the first scan.
const STARTUP_DELAY: Duration = Duration::from_secs(5);

/// A newer version than the running one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UpdateInfo {
    pub version: String,
    /// Release notes (markdown) from the update manifest, if any.
    pub notes: Option<String>,
    /// Publication date as RFC 3339, if the manifest has one.
    pub date: Option<String>,
}

/// The newest update found, and whether the tray asked the window to show it before the window
/// could listen (it was in the tray).
#[derive(Default)]
pub struct Updates {
    available: Mutex<Option<UpdateInfo>>,
    show_requested: AtomicBool,
}

impl Updates {
    pub fn available(&self) -> Option<UpdateInfo> {
        self.available
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether the tray asked to show the update dialog since the last call.
    pub fn take_show_request(&self) -> bool {
        self.show_requested.swap(false, Ordering::SeqCst)
    }
}

/// Checks for an update in the background a few seconds after start.
pub fn check_on_startup(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            std::thread::sleep(STARTUP_DELAY);
            // Failures are logged in `find`; there is nothing to show for a background check.
            let _ = tauri::async_runtime::block_on(check(&app));
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the update check");
    }
}

/// Asks the update endpoint for a newer release and remembers the answer.
pub async fn check(app: &AppHandle) -> CmdResult<Option<UpdateInfo>> {
    let info = if let Some(fake) = fake_update() {
        Some(fake)
    } else {
        find(app).await?.map(|update| UpdateInfo {
            version: update.version.clone(),
            notes: update.body.clone().filter(|b| !b.trim().is_empty()),
            date: update
                .date
                .and_then(|d| chrono::DateTime::from_timestamp(d.unix_timestamp(), 0))
                .map(|d| d.to_rfc3339()),
        })
    };
    remember(app, info.clone());
    Ok(info)
}

fn remember(app: &AppHandle, info: Option<UpdateInfo>) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let changed = {
        let mut available = updates
            .available
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let changed = *available != info;
        available.clone_from(&info);
        changed
    };
    if !changed {
        return;
    }
    crate::tray::set_update(info.as_ref().map(|i| i.version.clone()));
    if let Some(info) = info {
        tracing::info!(version = %info.version, "an update is available");
        if let Err(err) = app.emit(AVAILABLE_EVENT, info) {
            tracing::warn!(%err, "could not announce the update");
        }
    }
}

/// The tray's "Update" entry: opens the window and its update dialog.
pub fn show(app: &AppHandle) {
    if let Some(updates) = app.try_state::<Updates>() {
        updates.show_requested.store(true, Ordering::SeqCst);
    }
    crate::window::show_main_window_async(app);
    // A window that is already loaded hears the event; a new one asks `take_show_request`.
    let _ = app.emit(SHOW_EVENT, ());
}

/// Debug builds never see real updates; `QUESTKEEP_FAKE_UPDATE=<version>` pretends one exists, to
/// try the title bar and tray entries (installing it does nothing).
fn fake_update() -> Option<UpdateInfo> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let version = std::env::var("QUESTKEEP_FAKE_UPDATE").ok()?;
    Some(UpdateInfo {
        version,
        notes: Some("A pretend update for testing (QUESTKEEP_FAKE_UPDATE).".into()),
        date: None,
    })
}

/// Asks the update endpoint for a newer release. Always `None` in debug builds.
async fn find(app: &AppHandle) -> CmdResult<Option<tauri_plugin_updater::Update>> {
    use tauri_plugin_updater::UpdaterExt;
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let updater = app.updater().map_err(|e| {
        tracing::warn!(error = %e, "updater is not available");
        format!("Updates are not available: {e}")
    })?;
    updater.check().await.map_err(|e| {
        tracing::warn!(error = %e, "update check failed");
        format!("Could not check for updates: {e}")
    })
}

/// Downloads and installs the newest release, then restarts the app. Does nothing (and returns
/// normally) when no newer release exists.
pub async fn install(app: &AppHandle) -> CmdResult<()> {
    let Some(update) = find(app).await? else {
        return Ok(());
    };
    tracing::info!(version = %update.version, "installing update");
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "update installation failed");
            format!("Could not install the update: {e}")
        })?;
    app.restart()
}
