//! System tray icon and its menu. The menu lists recently played and favorite games and opens
//! store clients; it is rebuilt (debounced, off the UI thread) when the library changes.
//!
//! The selection logic is in `questkeep_core::tray` and `questkeep_core::platform`; this module
//! only turns it into Tauri menu items, so it has no unit tests of its own (creating a tray icon
//! needs a real desktop session).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use questkeep_core::events::CoreEvent;
use questkeep_core::images::load_icon_rgba;
use questkeep_core::platform::{self, StoreClient};
use questkeep_core::tray::{TrayGame, TrayGames, menu_label};
use tauri::image::Image;
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, MenuBuilder, MenuItem, Submenu, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use uuid::Uuid;

use crate::AppState;
use crate::window::{self, Lifecycle};

const TRAY_ID: &str = "main";
/// Minimum time between two menu rebuilds.
const REBUILD_INTERVAL: Duration = Duration::from_secs(2);

const ID_OPEN: &str = "open";
const ID_EXIT: &str = "exit";
const GAME_PREFIX: &str = "game:";
const URI_PREFIX: &str = "uri:";

/// Everything the menu shows; two equal models need no rebuild.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MenuModel {
    games: TrayGames,
    clients: Vec<StoreClient>,
}

/// Why the menu is rebuilt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    /// Games changed.
    Games,
    /// A store scan finished: installed store clients may have changed as well.
    Scan,
}

/// Creates the tray icon and starts the background thread that keeps its menu current.
pub fn create(app: &AppHandle, state: &Arc<AppState>) -> tauri::Result<()> {
    let menu = build_menu(app, &MenuModel::default())?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Questkeep")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_main_window_async(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()));
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    let (tx, rx) = mpsc::channel();
    state.library.events().subscribe(move |event| {
        let trigger = match event {
            CoreEvent::GamesChanged { .. }
            | CoreEvent::GameRemoved { .. }
            | CoreEvent::GameStarted { .. }
            | CoreEvent::GameStopped { .. } => Trigger::Games,
            CoreEvent::LibraryScanned { .. } => Trigger::Scan,
            CoreEvent::GameError { .. } => return,
        };
        // The receiver only goes away at shutdown.
        let _ = tx.send(trigger);
    });
    let app = app.clone();
    let state = Arc::clone(state);
    std::thread::Builder::new()
        .name("tray-menu".into())
        .spawn(move || refresh_loop(&app, &state, &rx))
        .map_err(|e| tauri::Error::Anyhow(e.into()))?;
    Ok(())
}

/// Sleeps on the channel (no timers, no polling) until the library changed, waits for the burst to
/// settle, and rebuilds the menu once if its content differs.
fn refresh_loop(app: &AppHandle, state: &AppState, rx: &Receiver<Trigger>) {
    let mut clients: Option<Vec<StoreClient>> = None;
    let mut shown: Option<MenuModel> = None;
    let mut first = true;
    loop {
        let mut rescan = clients.is_none();
        if !first {
            match rx.recv() {
                Ok(trigger) => rescan |= trigger == Trigger::Scan,
                Err(_) => return,
            }
            std::thread::sleep(REBUILD_INTERVAL);
            while let Ok(trigger) = rx.try_recv() {
                rescan |= trigger == Trigger::Scan;
            }
        }
        first = false;

        let games = match state.library.tray_games() {
            Ok(games) => games,
            Err(err) => {
                tracing::warn!(%err, "could not read the games for the tray menu");
                continue;
            }
        };
        // Games without an icon get one made in the background (never here); the icon's arrival
        // is a `GamesChanged` event, which comes back as the next rebuild.
        let missing = games.missing_icons();
        if !missing.is_empty() {
            state.metadata.enqueue_icons(missing);
        }
        if rescan {
            clients = Some(platform::installed_store_clients());
        }
        let model = MenuModel {
            games,
            clients: clients.clone().unwrap_or_default(),
        };
        if shown.as_ref() == Some(&model) {
            continue;
        }
        let for_menu = model.clone();
        let handle = app.clone();
        // Menus are created and applied on the main thread.
        let queued = app.run_on_main_thread(move || {
            let result = build_menu(&handle, &for_menu).and_then(|menu| {
                handle
                    .tray_by_id(TRAY_ID)
                    .map_or(Ok(()), |tray| tray.set_menu(Some(menu)))
            });
            if let Err(err) = result {
                tracing::warn!(%err, "could not update the tray menu");
            }
        });
        match queued {
            Ok(()) => shown = Some(model),
            Err(err) => tracing::warn!(%err, "could not schedule the tray menu update"),
        }
    }
}

fn disabled_item(app: &AppHandle, text: &str) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, format!("disabled:{text}"), text, false, None::<&str>)
}

/// One entry per game: with its cached icon when there is one, else a plain text item (no
/// placeholder box). Only the listed games (at most 40) are decoded, from small cached PNGs.
fn game_items(app: &AppHandle, games: &[TrayGame]) -> tauri::Result<Vec<Box<dyn IsMenuItem<Wry>>>> {
    games
        .iter()
        .map(|g| {
            let id = format!("{GAME_PREFIX}{}", g.id);
            let label = menu_label(&g.name);
            let icon = g
                .icon
                .as_deref()
                .and_then(load_icon_rgba)
                .map(|i| Image::new_owned(i.rgba, i.width, i.height));
            let item: Box<dyn IsMenuItem<Wry>> = match icon {
                Some(icon) => Box::new(IconMenuItem::with_id(
                    app,
                    id,
                    label,
                    true,
                    Some(icon),
                    None::<&str>,
                )?),
                None => Box::new(MenuItem::with_id(app, id, label, true, None::<&str>)?),
            };
            Ok(item)
        })
        .collect()
}

fn favorites_submenu(app: &AppHandle, favorites: &[TrayGame]) -> tauri::Result<Submenu<Wry>> {
    let mut builder = SubmenuBuilder::new(app, "Favorites");
    if favorites.is_empty() {
        builder = builder.item(&disabled_item(app, "No favorites")?);
    }
    for item in game_items(app, favorites)? {
        builder = builder.item(item.as_ref());
    }
    builder.build()
}

fn clients_submenu(app: &AppHandle, clients: &[StoreClient]) -> tauri::Result<Submenu<Wry>> {
    let mut builder = SubmenuBuilder::new(app, "Store clients");
    for client in clients {
        builder = builder.text(format!("{URI_PREFIX}{}", client.uri), client.name);
    }
    builder.build()
}

fn build_menu(app: &AppHandle, model: &MenuModel) -> tauri::Result<Menu<Wry>> {
    let mut builder = MenuBuilder::new(app);
    let recent = game_items(app, &model.games.recent)?;
    if recent.is_empty() {
        builder = builder.item(&disabled_item(app, "No recently played games")?);
    }
    for item in &recent {
        builder = builder.item(item.as_ref());
    }
    builder = builder
        .separator()
        .item(&favorites_submenu(app, &model.games.favorites)?)
        .separator()
        .text(ID_OPEN, "Open Questkeep");
    if !model.clients.is_empty() {
        builder = builder.item(&clients_submenu(app, &model.clients)?);
    }
    builder.separator().text(ID_EXIT, "Exit").build()
}

fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        ID_OPEN => window::show_main_window_async(app),
        ID_EXIT => {
            if let Some(lifecycle) = app.try_state::<Lifecycle>() {
                lifecycle.request_quit();
            }
            // Runs the exit handler: fetchers close and running sessions are saved.
            app.exit(0);
        }
        other => {
            if let Some(game) = other
                .strip_prefix(GAME_PREFIX)
                .and_then(|id| Uuid::parse_str(id).ok())
            {
                launch(app, game);
            } else if let Some(uri) = other.strip_prefix(URI_PREFIX) {
                open_client(uri.to_string());
            }
        }
    }
}

/// Launches a game like the `launch_game` command; a failure is logged and, if the window is
/// open, shown there as an error toast.
fn launch(app: &AppHandle, id: Uuid) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let state = Arc::clone(&state);
    std::thread::spawn(move || {
        let result = state
            .library
            .get_game(id)
            .and_then(|game| state.launcher.launch(&game));
        if let Err(err) = result {
            tracing::warn!(%err, %id, "launching from the tray failed");
            state.library.events().publish(CoreEvent::GameError {
                id,
                message: err.to_string(),
            });
        }
    });
}

fn open_client(uri: String) {
    std::thread::spawn(move || {
        if let Err(err) = platform::open_uri(&uri) {
            tracing::warn!(%err, %uri, "could not open the store client");
        }
    });
}
