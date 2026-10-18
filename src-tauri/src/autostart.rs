//! Start-with-system: registers or removes the OS autostart entry (the Run key on Windows). The
//! entry starts the app with `--minimized`, see `questkeep_core::tray::MINIMIZED_ARG`.
//!
//! Thin glue over `tauri-plugin-autostart`; not unit-tested because it writes to the user's real
//! startup configuration.

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// Turns the autostart entry on or off. Does nothing when it already matches.
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let launch = app.autolaunch();
    let current = launch.is_enabled().map_err(|e| e.to_string())?;
    if current == enabled {
        return Ok(());
    }
    let result = if enabled {
        launch.enable()
    } else {
        launch.disable()
    };
    result.map_err(|e| format!("Could not change the start-with-system setting: {e}"))
}

/// At startup: keeps the entry in line with the setting (refreshing the executable path after an
/// update moved it, and dropping a stale entry). Skipped in debug builds so a development binary
/// never ends up in the user's startup list.
pub fn sync_on_startup(app: &AppHandle, start_with_system: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let launch = app.autolaunch();
    let result = if start_with_system {
        launch.enable()
    } else if launch.is_enabled().unwrap_or(false) {
        launch.disable()
    } else {
        Ok(())
    };
    if let Err(err) = result {
        tracing::warn!(%err, "could not sync the autostart entry");
    }
}
