//! Thin Tauri shell: exposes core functionality as commands and forwards core events to the UI.

mod commands;

use std::sync::Arc;

use questkeep_core::Library;
use questkeep_core::events::EventBus;
use questkeep_core::images::CoverFetcher;
use questkeep_core::launcher::Launcher;
use questkeep_core::paths::DataPaths;
use questkeep_core::providers::ProviderRegistry;
use tauri::{Emitter, Manager, RunEvent};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

/// Name of the event every core event is forwarded as.
const CORE_EVENT: &str = "core-event";
/// Concurrent cover downloads.
const COVER_WORKERS: usize = 4;

/// Shared state behind every command.
pub struct AppState {
    pub library: Library,
    pub launcher: Launcher,
    pub covers: CoverFetcher,
}

impl AppState {
    /// Queues cover downloads for games without one, if enabled in settings.
    pub fn queue_missing_covers(&self) {
        if !self.library.settings().download_covers {
            return;
        }
        match self.library.missing_cover_jobs() {
            Ok(jobs) => {
                let queued = self.covers.enqueue(jobs);
                if queued > 0 {
                    tracing::info!(queued, "queued cover downloads");
                }
            }
            Err(err) => tracing::warn!(%err, "could not list games without covers"),
        }
    }
}

fn init_logging(paths: &DataPaths) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_env("QUESTKEEP_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,questkeep=debug,questkeep_core=debug"));
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("questkeep")
        .filename_suffix("log")
        .max_log_files(7)
        .build(paths.logs_dir());
    let (file_layer, guard) = match file_appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer);
            (Some(layer), Some(guard))
        }
        Err(_) => (None, None),
    };
    let stderr_layer = cfg!(debug_assertions).then(tracing_subscriber::fmt::layer);
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init();
    guard
}

fn build_state(app: &tauri::AppHandle) -> Result<(AppState, Option<WorkerGuard>), String> {
    let paths = DataPaths::resolve().map_err(|e| e.to_string())?;
    paths.ensure_dirs().map_err(|e| e.to_string())?;
    let guard = init_logging(&paths);
    tracing::info!(
        version = questkeep_core::VERSION,
        data_dir = %paths.root().display(),
        portable = paths.is_portable(),
        "starting Questkeep"
    );

    // Only the images folder is reachable through the asset protocol.
    if let Err(err) = app
        .asset_protocol_scope()
        .allow_directory(paths.images_dir(), true)
    {
        tracing::error!(%err, "could not allow the images directory in the asset scope");
    }

    let events = EventBus::new();
    let handle = app.clone();
    events.subscribe(move |event| {
        if let Err(err) = handle.emit(CORE_EVENT, event) {
            tracing::warn!(%err, "could not forward event to the UI");
        }
    });

    let providers = ProviderRegistry::with_builtin();
    let library =
        Library::open(paths, providers.clone(), events.clone()).map_err(|e| e.to_string())?;
    match library.database().recover_open_sessions() {
        Ok(ids) if !ids.is_empty() => {
            tracing::info!(count = ids.len(), "recovered unfinished play sessions");
        }
        Ok(_) => {}
        Err(err) => tracing::error!(%err, "could not recover play sessions"),
    }
    let launcher = Launcher::new(library.database().clone(), events.clone());
    let covers = CoverFetcher::new(
        library.database().clone(),
        library.images().clone(),
        providers,
        events,
        COVER_WORKERS,
    );
    Ok((
        AppState {
            library,
            launcher,
            covers,
        },
        guard,
    ))
}

/// Creates the main window from its config (`"create": false` in `tauri.conf.json`) and shows it
/// once setup succeeded, which also avoids a white flash.
///
/// Debug builds with `QUESTKEEP_HIDDEN=1` build it invisible, unfocused and off the taskbar, so the
/// app can be driven through the WebView2 DevTools port while someone else uses the machine (e.g. a
/// fullscreen game) without a window ever appearing or taking focus.
fn create_main_window(app: &tauri::App) -> tauri::Result<()> {
    let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") else {
        return Ok(());
    };
    let keep_hidden =
        cfg!(debug_assertions) && std::env::var("QUESTKEEP_HIDDEN").is_ok_and(|v| v == "1");
    let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), config)?;
    if keep_hidden {
        tracing::info!("QUESTKEEP_HIDDEN=1: creating the window hidden");
        builder = builder.visible(false).focused(false).skip_taskbar(true);
    }
    let window = builder.build()?;
    if !keep_hidden {
        window.show()?;
    }
    Ok(())
}

/// Startup work that must not delay the window: provider scans and cover downloads.
fn spawn_startup_tasks(state: Arc<AppState>) {
    tauri::async_runtime::spawn_blocking(move || {
        if state.library.settings().scan_on_startup {
            state.library.scan_all();
        }
        state.queue_missing_covers();
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let (state, guard) = build_state(app.handle())?;
            let state = Arc::new(state);
            app.manage(state.clone());
            if let Some(guard) = guard {
                // Keep the log writer alive for the whole process.
                app.manage(guard);
            }
            create_main_window(app)?;
            spawn_startup_tasks(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_games,
            commands::get_game,
            commands::add_manual_game,
            commands::update_game,
            commands::set_cover,
            commands::remove_game,
            commands::list_providers,
            commands::scan_library,
            commands::launch_game,
            commands::running_games,
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::seed_fake_library,
        ])
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!("failed to start Questkeep: {err}");
            std::process::exit(1);
        }
    };

    app.run(|handle, event| {
        if let RunEvent::Exit = event
            && let Some(state) = handle.try_state::<Arc<AppState>>()
        {
            tracing::info!("shutting down");
            state.covers.close();
            // Stops trackers and saves the elapsed time of running games.
            state.launcher.shutdown();
        }
    });
}
