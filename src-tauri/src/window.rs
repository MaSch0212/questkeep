//! Main-window lifecycle: creating it, showing it from the tray, and "hiding" it to the tray by
//! destroying the webview so the WebView2 renderer (150-250 MB) is freed. The core keeps running
//! while no window exists. The decisions live in `questkeep_core::tray`; this is the glue.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use questkeep_core::tray::{self, CloseAction};
use questkeep_core::window_placement::{MonitorArea, PlacementStore, WindowPlacement};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};

use crate::AppState;

/// Label of the main window (the only one with IPC access).
pub const MAIN_WINDOW: &str = "main";

/// Flags shared between the tray, the window events and the exit handling.
#[derive(Default)]
pub struct Lifecycle {
    /// The tray icon exists, so hiding to the tray leaves a way back.
    tray_available: AtomicBool,
    /// The main window was destroyed (or never created) to wait in the tray.
    hidden_to_tray: AtomicBool,
    /// The user chose Exit in the tray menu.
    quit_requested: AtomicBool,
    /// Serializes window creation (a double click on the tray icon must not build two).
    window_lock: Mutex<()>,
    /// Last normal rectangle and maximized state, tracked while the window moves so a minimized
    /// window (whose position is off-screen) still saves where it was.
    placement: Mutex<Option<WindowPlacement>>,
    /// Messages for the UI that arrived while it may not exist (e.g. from a second start).
    notices: Mutex<Vec<String>>,
}

impl Lifecycle {
    pub fn set_tray_available(&self, available: bool) {
        self.tray_available.store(available, Ordering::SeqCst);
    }

    pub fn tray_available(&self) -> bool {
        self.tray_available.load(Ordering::SeqCst)
    }

    pub fn set_hidden_to_tray(&self, hidden: bool) {
        self.hidden_to_tray.store(hidden, Ordering::SeqCst);
    }

    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }

    /// Queues a message for the UI (shown as a toast once the window asks for it).
    pub fn push_notice(&self, notice: String) {
        self.notices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(notice);
    }

    pub fn take_notices(&self) -> Vec<String> {
        std::mem::take(&mut *self.notices.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

pub(crate) fn keep_hidden() -> bool {
    cfg!(debug_assertions) && std::env::var("QUESTKEEP_HIDDEN").is_ok_and(|v| v == "1")
}

/// Creates the main window from its config (`"create": false` in `tauri.conf.json`) and shows it
/// once setup succeeded, which also avoids a white flash.
///
/// Debug builds with `QUESTKEEP_HIDDEN=1` build it invisible, unfocused and off the taskbar, so the
/// app can be driven through the WebView2 DevTools port while someone else uses the machine (e.g. a
/// fullscreen game) without a window ever appearing or taking focus.
pub fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN_WINDOW)
    else {
        return Ok(());
    };
    let keep_hidden = keep_hidden();
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, config)?;
    if keep_hidden {
        tracing::info!("QUESTKEEP_HIDDEN=1: creating the window hidden");
        builder = builder.visible(false).focused(false).skip_taskbar(true);
    }
    let window = builder.build()?;
    if !keep_hidden {
        restore_placement(app, &window);
        window.show()?;
    }
    Ok(())
}

fn placement_store(app: &AppHandle) -> Option<PlacementStore> {
    let state = app.try_state::<Arc<AppState>>()?;
    Some(PlacementStore::new(state.library.paths().window_file()))
}

/// Moves the (still invisible) window to where it was last closed, if that spot is still on a
/// monitor; otherwise it keeps the default size, centered by the OS.
fn restore_placement(app: &AppHandle, window: &WebviewWindow) {
    let Some(saved) = placement_store(app).and_then(|store| store.load()) else {
        return;
    };
    let monitors: Vec<MonitorArea> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let area = m.work_area();
            MonitorArea {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
            }
        })
        .collect();
    let Some(placement) = saved.fit(&monitors) else {
        tracing::debug!("saved window position is off-screen, using the default");
        return;
    };
    let position = PhysicalPosition::new(placement.x, placement.y);
    // Position first so the size applies at the target monitor's scale; again afterwards in case
    // the move across monitors with different DPI resized the window.
    let result = window
        .set_position(position)
        .and_then(|()| window.set_size(PhysicalSize::new(placement.width, placement.height)))
        .and_then(|()| window.set_position(position))
        .and_then(|()| {
            if placement.maximized {
                window.maximize()
            } else {
                Ok(())
            }
        });
    if let Err(err) = result {
        tracing::warn!(%err, "could not restore the window position");
    }
    let lifecycle = app.state::<Lifecycle>();
    *lifecycle
        .placement
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(placement);
}

/// Records the window's current rectangle unless it is minimized (then the position is
/// off-screen); while maximized only the flag changes, so un-maximizing later restores the size.
fn track_placement(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if window.is_minimized().unwrap_or(true) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let lifecycle = app.state::<Lifecycle>();
    let mut placement = lifecycle
        .placement
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if maximized {
        if let Some(placement) = placement.as_mut() {
            placement.maximized = true;
        }
        return;
    }
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) {
        *placement = Some(WindowPlacement {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
}

/// Saves where the main window is, before it is destroyed or the app exits.
pub fn save_placement(app: &AppHandle) {
    if keep_hidden() {
        return;
    }
    track_placement(app);
    let placement = *app
        .state::<Lifecycle>()
        .placement
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let (Some(placement), Some(store)) = (placement, placement_store(app)) else {
        return;
    };
    if let Err(err) = store.save(&placement) {
        tracing::warn!(%err, "could not save the window position");
    }
}

/// Brings the main window to the front, creating it when it was destroyed in the tray. Blocks
/// while the window is built, so call it from a worker thread, not from the event loop.
pub fn show_main_window(app: &AppHandle) {
    let lifecycle = app.state::<Lifecycle>();
    let _guard = lifecycle
        .window_lock
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        if !keep_hidden() {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        lifecycle.set_hidden_to_tray(false);
        return;
    }
    match create_main_window(app) {
        Ok(()) => lifecycle.set_hidden_to_tray(false),
        Err(err) => tracing::error!(%err, "could not open the main window"),
    }
}

/// [`show_main_window`] on a short-lived thread, for callers on the event loop.
pub fn show_main_window_async(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || show_main_window(&app));
}

/// Destroys the main window and keeps the app alive in the tray.
fn hide_to_tray(app: &AppHandle) {
    // Set first: destroying the last window triggers an exit request that must see the flag.
    app.state::<Lifecycle>().set_hidden_to_tray(true);
    save_placement(app);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        tracing::info!("hiding to the tray (destroying the main window)");
        if let Err(err) = window.destroy() {
            tracing::warn!(%err, "could not destroy the main window");
        }
    }
}

/// Reacts to events of the main window: close and minimize go to the tray when configured.
pub fn on_main_window_event(app: &AppHandle, event: &WindowEvent) {
    let (Some(state), Some(lifecycle)) = (
        app.try_state::<Arc<AppState>>(),
        app.try_state::<Lifecycle>(),
    ) else {
        return;
    };
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            let settings = state.library.settings();
            if tray::close_action(&settings, lifecycle.tray_available()) == CloseAction::HideToTray
            {
                api.prevent_close();
                hide_to_tray(app);
            } else {
                save_placement(app);
            }
        }
        WindowEvent::Moved(_) => track_placement(app),
        WindowEvent::Resized(_) => {
            // Resized fires continuously while dragging; the cheap check comes first.
            let minimized = app
                .get_webview_window(MAIN_WINDOW)
                .is_some_and(|w| w.is_minimized().unwrap_or(false));
            if minimized
                && tray::hide_on_minimize(&state.library.settings(), lifecycle.tray_available())
            {
                hide_to_tray(app);
            } else if !minimized {
                track_placement(app);
            }
        }
        _ => {}
    }
}

/// Whether an exit request must be ignored because the app waits in the tray.
pub fn keep_running(app: &AppHandle, code: Option<i32>) -> bool {
    app.try_state::<Lifecycle>().is_some_and(|l| {
        tray::keep_running_without_windows(
            code,
            l.hidden_to_tray.load(Ordering::SeqCst),
            l.quit_requested.load(Ordering::SeqCst),
        )
    })
}
