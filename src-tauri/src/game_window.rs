//! What the shell does around games and notifications: minimizing or hiding the window while a
//! game started from Questkeep runs (and bringing it back), and system notifications for errors
//! and warnings while the window is in the background.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use questkeep_core::events::{CoreEvent, EventBus};
use questkeep_core::notifications::NotificationSource;
use questkeep_core::settings::WindowOnGameStart;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

use crate::AppState;
use crate::window::{self, Lifecycle, MAIN_WINDOW};

/// Games started from Questkeep that run, and whether the window was put away for them.
#[derive(Default)]
pub struct GameWindow {
    running: Mutex<HashSet<Uuid>>,
    /// The window was minimized or hidden because a game started.
    put_away: AtomicBool,
    /// A system notification was shown: the UI opens the bell when it shows next.
    open_bell: AtomicBool,
}

impl GameWindow {
    /// Whether the UI should open the notification bell (asked when it shows), once.
    pub fn take_open_bell(&self) -> bool {
        self.open_bell.swap(false, Ordering::SeqCst)
    }
}

/// Subscribes to the core events this module reacts to.
pub fn watch(app: &AppHandle, events: &EventBus) {
    let app = app.clone();
    events.subscribe(move |event| match event {
        CoreEvent::GameStarted { id } => game_started(&app, *id),
        CoreEvent::GameStopped { id, .. } => game_stopped(&app, *id),
        // Plugin updates wait in the bell: no system notification, which may pop up over a game.
        CoreEvent::Notified { notification }
            if notification.source != NotificationSource::Plugins =>
        {
            notify(&app, &notification.title, &notification.message);
        }
        _ => {}
    });
}

fn game_started(app: &AppHandle, id: Uuid) {
    let (Some(state), Some(games)) = (
        app.try_state::<Arc<AppState>>(),
        app.try_state::<GameWindow>(),
    ) else {
        return;
    };
    if !state.launcher.is_launched(id) {
        return; // noticed by the watcher, not started from Questkeep
    }
    games
        .running
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id);
    if window::keep_hidden() {
        return;
    }
    let action = state.library.settings().window_on_game_start;
    let Some(main) = app.get_webview_window(MAIN_WINDOW) else {
        return; // already in the tray
    };
    if !main.is_visible().unwrap_or(false) || main.is_minimized().unwrap_or(false) {
        return; // already out of the way; nothing to bring back later
    }
    match action {
        WindowOnGameStart::Nothing => {}
        WindowOnGameStart::Minimize => {
            tracing::info!("minimizing while a game runs");
            games.put_away.store(true, Ordering::SeqCst);
            let _ = main.minimize();
        }
        WindowOnGameStart::HideToTray => {
            games.put_away.store(true, Ordering::SeqCst);
            if app.state::<Lifecycle>().tray_available() {
                tracing::info!("hiding to the tray while a game runs");
                window::hide_to_tray(app);
            } else {
                let _ = main.minimize();
            }
        }
    }
}

fn game_stopped(app: &AppHandle, id: Uuid) {
    let (Some(state), Some(games)) = (
        app.try_state::<Arc<AppState>>(),
        app.try_state::<GameWindow>(),
    ) else {
        return;
    };
    let none_left = {
        let mut running = games.running.lock().unwrap_or_else(PoisonError::into_inner);
        if !running.remove(&id) {
            return;
        }
        running.is_empty()
    };
    if !none_left || !games.put_away.swap(false, Ordering::SeqCst) {
        return;
    }
    if state.library.settings().restore_window_after_game {
        tracing::info!("bringing the window back after the game");
        let app = app.clone();
        std::thread::spawn(move || window::restore_main_window(&app));
    }
}

/// Shows a system notification while the window is minimized, invisible or in the tray.
fn notify(app: &AppHandle, title: &str, message: &str) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    if !state.library.settings().system_notifications || window::keep_hidden() {
        return;
    }
    let background = app
        .get_webview_window(MAIN_WINDOW)
        .is_none_or(|w| !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false));
    if !background {
        return;
    }
    if let Some(games) = app.try_state::<GameWindow>() {
        games.open_bell.store(true, Ordering::SeqCst);
    }
    if let Err(err) = app
        .notification()
        .builder()
        .title(title)
        .body(message)
        .show()
    {
        tracing::warn!(%err, "could not show a system notification");
    }
}
