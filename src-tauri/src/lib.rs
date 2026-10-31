//! Thin Tauri shell: exposes core functionality as commands and forwards core events to the UI.

mod autostart;
mod commands;
mod signin;
mod tray;
#[cfg(windows)]
mod tray_popup;
mod tray_theme;
mod updates;
mod window;

use std::sync::Arc;

use questkeep_core::Library;
use questkeep_core::auth::KeyringTokenStore;
use questkeep_core::events::EventBus;
use std::time::Duration;

use questkeep_core::images::{CoverFetcher, MetadataFetcher};
use questkeep_core::launcher::Launcher;
use questkeep_core::paths::DataPaths;
use questkeep_core::providers::ProviderRegistry;
use questkeep_core::tray::{self as tray_logic, MINIMIZED_ARG};
use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

/// Name of the event every core event is forwarded as.
const CORE_EVENT: &str = "core-event";
/// Concurrent cover downloads.
const COVER_WORKERS: usize = 4;
/// Minimum time between two store metadata requests.
const METADATA_INTERVAL: Duration = Duration::from_millis(1500);

/// Shared state behind every command.
pub struct AppState {
    pub library: Library,
    pub launcher: Launcher,
    pub covers: CoverFetcher,
    pub metadata: MetadataFetcher,
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

    /// Queues metadata downloads for games never checked, if enabled in settings.
    pub fn queue_missing_metadata(&self) {
        if !self.library.settings().fetch_metadata {
            return;
        }
        match self.library.missing_metadata_jobs() {
            Ok(jobs) => {
                let queued = self.metadata.enqueue(jobs);
                if queued > 0 {
                    tracing::info!(queued, "queued metadata downloads");
                }
            }
            Err(err) => tracing::warn!(%err, "could not list games without metadata"),
        }
    }

    /// Queues icon jobs for games without an icon. Each game is tried once per run; the jobs
    /// share the metadata worker and run after the metadata queued before them.
    pub fn queue_missing_icons(&self) {
        match self.library.missing_icon_ids() {
            Ok(ids) => {
                let queued = self.metadata.enqueue_icons(ids);
                if queued > 0 {
                    tracing::info!(queued, "queued icon jobs");
                }
            }
            Err(err) => tracing::warn!(%err, "could not list games without icons"),
        }
    }

    /// Follow-up downloads after the library changed (a scan or startup).
    pub fn queue_downloads(&self) {
        self.queue_missing_covers();
        self.queue_missing_metadata();
        self.queue_missing_icons();
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
        // Only the main window listens (never the store login windows). With no window (the
        // app waits in the tray) this is a no-op.
        if let Err(err) = handle.emit_to(window::MAIN_WINDOW, CORE_EVENT, event) {
            tracing::debug!(%err, "could not forward event to the UI");
        }
    });

    let keyring: Arc<KeyringTokenStore> = Arc::new(KeyringTokenStore::new());
    let providers = ProviderRegistry::with_builtin(
        keyring.clone(),
        Some(&paths.cache_dir()),
        Some(Arc::new(signin::ShellWebSession::new(
            app.clone(),
            paths.webview_dir(),
        ))),
    );
    let library = Library::open(paths, providers.clone(), events.clone(), keyring)
        .map_err(|e| e.to_string())?;
    match library.database().recover_open_sessions() {
        Ok(ids) if !ids.is_empty() => {
            tracing::info!(count = ids.len(), "recovered unfinished play sessions");
        }
        Ok(_) => {}
        Err(err) => tracing::error!(%err, "could not recover play sessions"),
    }
    let launcher = Launcher::new(library.database().clone(), events.clone());
    launcher.start_watching(library.settings().detect_external_launches);
    let covers = CoverFetcher::new(
        library.database().clone(),
        library.images().clone(),
        providers.clone(),
        events.clone(),
        library.artwork().clone(),
        COVER_WORKERS,
    );
    let metadata = MetadataFetcher::new(
        library.database().clone(),
        library.images().clone(),
        providers,
        events,
        library.artwork().clone(),
        library.igdb().clone(),
        METADATA_INTERVAL,
    );
    Ok((
        AppState {
            library,
            launcher,
            covers,
            metadata,
        },
        guard,
    ))
}

/// Startup work that must not delay the window: provider scans and image/metadata downloads.
fn spawn_startup_tasks(state: Arc<AppState>) {
    tauri::async_runtime::spawn_blocking(move || {
        if state.library.settings().scan_on_startup {
            state.library.scan_all();
        }
        state.queue_downloads();
    });
}

/// Only the main window may call app commands. Store login windows load remote pages and must
/// never reach the core, whatever those pages do (they also have no capability, see
/// capabilities/default.json).
fn main_window_only(
    handler: impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    move |invoke| {
        if invoke.message.webview().label() != "main" {
            tracing::warn!(
                command = %invoke.message.command(),
                window = %invoke.message.webview().label(),
                "rejected a command from a non-main window"
            );
            invoke
                .resolver
                .reject("This window is not allowed to call that command.");
            return true;
        }
        handler(invoke)
    }
}

/// Name of the event that tells the UI to fetch queued notices (`take_notices`).
const NOTICE_EVENT: &str = "app-notice";

/// A second start of a *different* executable hands over to this one and exits; tell the user,
/// or a newer build seems to run old code.
fn note_second_start(app: &tauri::AppHandle, started: Option<&str>) {
    let canonical = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
    let Ok(running) = std::env::current_exe() else {
        return;
    };
    let started = started.map(|s| canonical(std::path::Path::new(s)));
    let Some(notice) = tray_logic::second_start_notice(&canonical(&running), started.as_deref())
    else {
        return;
    };
    tracing::info!(started = ?started, "a different executable handed over to this instance");
    if let Some(lifecycle) = app.try_state::<window::Lifecycle>() {
        lifecycle.push_notice(notice);
    }
    // An open window shows it now; a new window fetches it on start.
    let _ = app.emit_to(window::MAIN_WINDOW, NOTICE_EVENT, ());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    // Starting Questkeep again (e.g. from the Start menu while it waits in the tray) shows the
    // running instance instead of opening a second process. Debug builds skip it so a dev
    // instance can run next to an installed one.
    if !cfg!(debug_assertions) {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            note_second_start(app, args.first().map(String::as_str));
            // An autostart racing with a manual start must not pop the window up.
            if !tray_logic::parse_launch_args(&args).minimized {
                window::show_main_window_async(app);
            }
        }));
    }
    let app = builder
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let (state, guard) = build_state(app.handle())?;
            let state = Arc::new(state);
            app.manage(state.clone());
            if let Some(guard) = guard {
                // Keep the log writer alive for the whole process.
                app.manage(guard);
            }
            app.manage(window::Lifecycle::default());
            app.manage(updates::Updates::default());
            let lifecycle = app.state::<window::Lifecycle>();
            // Hidden test runs (QUESTKEEP_HIDDEN=1) must not show anything, a tray icon included.
            if window::keep_hidden() {
                tracing::info!("QUESTKEEP_HIDDEN=1: no tray icon");
            } else {
                match tray::create(app.handle(), &state) {
                    Ok(()) => lifecycle.set_tray_available(true),
                    Err(err) => tracing::error!(%err, "could not create the tray icon"),
                }
            }
            let settings = state.library.settings();
            let args = tray_logic::parse_launch_args(std::env::args());
            if tray_logic::start_in_tray(&settings, args, lifecycle.tray_available()) {
                tracing::info!("started minimized: staying in the tray");
                lifecycle.set_hidden_to_tray(true);
            } else {
                window::create_main_window(app.handle())?;
            }
            autostart::sync_on_startup(app.handle(), settings.start_with_system);
            spawn_startup_tasks(state);
            updates::check_on_startup(app.handle());
            Ok(())
        })
        .invoke_handler(main_window_only(tauri::generate_handler![
            commands::list_games,
            commands::get_game,
            commands::add_manual_game,
            commands::update_game,
            commands::set_cover,
            commands::remove_game,
            commands::list_providers,
            commands::scan_library,
            commands::scan_all_libraries,
            commands::launch_game,
            commands::install_game,
            commands::uninstall_game,
            commands::combine_games,
            commands::separate_copy,
            commands::prefer_copy,
            commands::find_duplicates,
            commands::dismiss_duplicates,
            commands::add_game_with_option,
            commands::refresh_metadata,
            commands::refetch_artwork,
            commands::running_games,
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::open_app_folder,
            commands::open_install_folder,
            commands::take_notices,
            commands::seed_fake_library,
            commands::check_update,
            commands::available_update,
            commands::take_update_request,
            commands::install_update,
            commands::account_status,
            commands::sign_in,
            commands::sign_in_with_code,
            commands::sign_out,
            commands::count_games,
            commands::list_views,
            commands::create_view,
            commands::update_view,
            commands::duplicate_view,
            commands::delete_view,
            commands::create_view_section,
            commands::rename_view_section,
            commands::delete_view_section,
            commands::arrange_views,
            commands::sync_times,
            commands::start_page,
            commands::last_page,
            commands::set_last_page,
            commands::list_categories,
            commands::create_category,
            commands::rename_category,
            commands::delete_category,
            commands::reorder_categories,
            commands::merge_categories,
            commands::separate_category_source,
            commands::set_game_categories,
            commands::list_launchers,
            commands::open_launcher,
            commands::exit_app,
        ]))
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!("failed to start Questkeep: {err}");
            std::process::exit(1);
        }
    };

    app.run(|handle, event| match event {
        RunEvent::WindowEvent { label, event, .. } if label == window::MAIN_WINDOW => {
            window::on_main_window_event(handle, &event);
        }
        // The last window went away (or the app asked to exit). While the app waits in the tray
        // only a real exit (tray "Exit") may end the process.
        RunEvent::ExitRequested { code, api, .. } => {
            if window::keep_running(handle, code) {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            if let Some(state) = handle.try_state::<Arc<AppState>>() {
                tracing::info!("shutting down");
                state.covers.close();
                state.metadata.close();
                // Stops trackers and saves the elapsed time of running games.
                state.launcher.shutdown();
            }
        }
        _ => {}
    });
}
