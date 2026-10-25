//! Tauri commands. Each one is a thin wrapper that runs a core call on the blocking pool and turns
//! errors into user-readable strings. All logic lives in `questkeep-core`.

use std::path::PathBuf;
use std::sync::Arc;

use questkeep_core::auth::AccountStatus;
use questkeep_core::model::{Game, GameQuery, GameUpdate, NewManualGame, ScanSummary};
use questkeep_core::providers::ProviderInfo;
use questkeep_core::settings::Settings;
use questkeep_core::{AppInfo, CoreError};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

type CmdResult<T> = Result<T, String>;

/// Runs `f` with the app state on Tauri's blocking thread pool.
async fn blocking<T, F>(state: State<'_, Arc<AppState>>, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> Result<T, CoreError> + Send + 'static,
{
    let state = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || f(&state))
        .await
        .map_err(|e| format!("Internal error: {e}"))?
        .map_err(|e| {
            tracing::warn!(error = %e, "command failed");
            e.to_string()
        })
}

fn parse_id(id: &str) -> Result<Uuid, CoreError> {
    Uuid::parse_str(id).map_err(|_| CoreError::Invalid(format!("Invalid game id '{id}'")))
}

#[tauri::command]
pub async fn list_games(state: State<'_, Arc<AppState>>, query: GameQuery) -> CmdResult<Vec<Game>> {
    blocking(state, move |s| s.library.list_games(&query)).await
}

#[tauri::command]
pub async fn get_game(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<Game> {
    blocking(state, move |s| s.library.get_game(parse_id(&id)?)).await
}

#[tauri::command]
pub async fn add_manual_game(
    state: State<'_, Arc<AppState>>,
    input: NewManualGame,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        let game = s.library.add_manual_game(&input)?;
        // Cover (SteamGridDB) and metadata (IGDB) right away, not only on the next start.
        s.queue_downloads();
        Ok(game)
    })
    .await
}

#[tauri::command]
pub async fn update_game(
    state: State<'_, Arc<AppState>>,
    id: String,
    update: GameUpdate,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        s.library.update_game(parse_id(&id)?, &update)
    })
    .await
}

#[tauri::command]
pub async fn set_cover(
    state: State<'_, Arc<AppState>>,
    id: String,
    image_path: Option<String>,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        let path = image_path.map(PathBuf::from);
        s.library.set_cover(parse_id(&id)?, path.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn remove_game(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.remove_game(parse_id(&id)?)).await
}

#[tauri::command]
pub async fn list_providers(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<ProviderInfo>> {
    blocking(state, |s| Ok(s.library.provider_infos())).await
}

/// Scans every store, one after another. Progress and results arrive as core events
/// (`scansQueued`, `scanStarted`, `libraryScanned`, `scanFailed`); failures are not an error here.
#[tauri::command]
pub async fn scan_all_libraries(state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    blocking(state, |s| {
        s.library.scan_all();
        s.queue_downloads();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn scan_library(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> CmdResult<ScanSummary> {
    blocking(state, move |s| {
        let summary = s.library.scan(&provider_id)?;
        s.queue_downloads();
        Ok(summary)
    })
    .await
}

#[tauri::command]
pub async fn launch_game(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| {
        let game = s.library.get_game(parse_id(&id)?)?;
        s.launcher.launch(&game)
    })
    .await
}

/// Starts installing an owned game through its store client.
#[tauri::command]
pub async fn install_game(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.install_game(parse_id(&id)?)).await
}

/// Hands the uninstall to the store client or the game's uninstaller, which asks the user.
#[tauri::command]
pub async fn uninstall_game(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.uninstall_game(parse_id(&id)?)).await
}

/// Re-downloads store metadata (and a missing background) for one game in the background.
#[tauri::command]
pub async fn refresh_metadata(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| {
        let job = s.library.metadata_job(parse_id(&id)?)?;
        s.metadata.enqueue([job]);
        Ok(())
    })
    .await
}

/// Fetches the game's cover and background from SteamGridDB again, replacing the current ones.
/// Fails with a readable message when no API key is configured. Results arrive as
/// `gamesChanged` events.
#[tauri::command]
pub async fn refetch_artwork(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| {
        let job = s.library.refetch_artwork(parse_id(&id)?)?;
        s.covers.enqueue([job]);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn running_games(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<String>> {
    blocking(state, |s| {
        Ok(s.launcher
            .running()
            .into_iter()
            .map(|id| id.to_string())
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn app_info(state: State<'_, Arc<AppState>>) -> CmdResult<AppInfo> {
    blocking(state, |s| Ok(s.library.app_info())).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> CmdResult<Settings> {
    blocking(state, |s| Ok(s.library.settings())).await
}

#[tauri::command]
pub async fn save_settings(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> CmdResult<()> {
    blocking(state, move |s| {
        let had_key = s.library.artwork().is_configured();
        let had_igdb = s.library.igdb().is_configured();
        let before = s.library.settings();
        // Autostart first: when the system refuses, nothing is saved and the UI shows why.
        if before.start_with_system != settings.start_with_system {
            crate::autostart::set_enabled(&app, settings.start_with_system)
                .map_err(CoreError::Invalid)?;
        }
        if let Err(err) = s.library.save_settings(&settings) {
            if before.start_with_system != settings.start_with_system {
                // Keep the startup entry and the saved setting in line.
                let _ = crate::autostart::set_enabled(&app, before.start_with_system);
            }
            return Err(err);
        }
        // A newly entered SteamGridDB key can fill in covers for games that had no source.
        if !had_key && s.library.artwork().is_configured() {
            s.queue_missing_covers();
        }
        // New IGDB credentials can fill in metadata for games whose store has none.
        if !had_igdb && s.library.igdb().is_configured() {
            s.queue_missing_metadata();
        }
        Ok(())
    })
    .await
}

/// Messages queued for the UI while it may not have existed (e.g. a second start of another
/// executable); each is returned once.
#[tauri::command]
pub fn take_notices(lifecycle: State<'_, crate::window::Lifecycle>) -> Vec<String> {
    lifecycle.take_notices()
}

/// Opens the data or logs folder (`kind` is `"data"` or `"logs"`) in the file manager. Done from
/// Rust so the webview needs no permission to open arbitrary paths.
#[tauri::command]
pub async fn open_app_folder(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    kind: String,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let folder = blocking(state, move |s| s.library.app_folder(&kind)).await?;
    app.opener()
        .open_path(folder.display().to_string(), None::<&str>)
        .map_err(|e| {
            tracing::warn!(error = %e, "could not open a folder");
            format!("Could not open the folder: {e}")
        })
}

/// Sign-in state of every store with an account (GOG, Epic).
#[tauri::command]
pub async fn account_status(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<AccountStatus>> {
    blocking(state, |s| Ok(s.library.account_statuses())).await
}

/// Opens the store's login window and waits for the result: true when the account is now
/// signed in (owned games are scanned and covers/metadata queued), false when the window was
/// closed without signing in.
#[tauri::command]
pub async fn sign_in(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    provider: String,
) -> CmdResult<bool> {
    let (login_url, profile) = {
        let provider = provider.clone();
        blocking(state.clone(), move |s| {
            let profile = s.library.paths().webview_dir().join(&provider);
            Ok((s.library.login_url(&provider)?, profile))
        })
        .await?
    };
    match crate::signin::run(&app, &provider, &login_url, profile).await? {
        crate::signin::Outcome::Closed => Ok(false),
        crate::signin::Outcome::Code(code) => {
            blocking(state, move |s| {
                s.library.sign_in_with_code(&provider, &code)?;
                s.queue_downloads();
                Ok(true)
            })
            .await
        }
    }
}

/// Signs in with a code (or, for GOG, the redirect address) the user pasted, for when the login
/// window cannot be used. Scans the store afterwards.
#[tauri::command]
pub async fn sign_in_with_code(
    state: State<'_, Arc<AppState>>,
    provider: String,
    code: String,
) -> CmdResult<AccountStatus> {
    blocking(state, move |s| {
        let status = s.library.sign_in_with_code(&provider, &code)?;
        s.queue_downloads();
        Ok(status)
    })
    .await
}

/// Forgets the saved sign-in of a store. Games stay in the library.
#[tauri::command]
pub async fn sign_out(
    state: State<'_, Arc<AppState>>,
    provider: String,
) -> CmdResult<AccountStatus> {
    blocking(state, move |s| s.library.sign_out(&provider)).await
}

#[tauri::command]
pub async fn seed_fake_library(state: State<'_, Arc<AppState>>, count: usize) -> CmdResult<usize> {
    if !cfg!(debug_assertions) {
        return Err("The fake-library seed is only available in development builds.".into());
    }
    blocking(state, move |s| {
        s.library.seed_fake_library(count.min(50_000))
    })
    .await
}

/// A newer version found by [`check_update`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateInfo {
    pub version: String,
    /// Release notes (markdown) from the update manifest, if any.
    pub notes: Option<String>,
    /// Publication date as RFC 3339, if the manifest has one.
    pub date: Option<String>,
}

/// Asks the update endpoint for a newer release. Always `None` in debug builds.
async fn find_update(app: &tauri::AppHandle) -> CmdResult<Option<tauri_plugin_updater::Update>> {
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

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> CmdResult<Option<UpdateInfo>> {
    Ok(find_update(&app).await?.map(|update| UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone().filter(|b| !b.trim().is_empty()),
        date: update
            .date
            .and_then(|d| chrono::DateTime::from_timestamp(d.unix_timestamp(), 0))
            .map(|d| d.to_rfc3339()),
    }))
}

/// Downloads and installs the newest release, then restarts the app. Does nothing (and returns
/// normally) when no newer release exists.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> CmdResult<()> {
    let Some(update) = find_update(&app).await? else {
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
