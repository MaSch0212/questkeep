//! Tauri commands. Each one is a thin wrapper that runs a core call on the blocking pool and turns
//! errors into user-readable strings. All logic lives in `questkeep-core`.

use std::path::PathBuf;
use std::sync::Arc;

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
    blocking(state, move |s| s.library.add_manual_game(&input)).await
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

/// Re-downloads store metadata (and a missing background) for one game in the background.
#[tauri::command]
pub async fn refresh_metadata(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| {
        if let Some(job) = s.library.metadata_job(parse_id(&id)?)? {
            s.metadata.enqueue([job]);
        }
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
pub async fn save_settings(state: State<'_, Arc<AppState>>, settings: Settings) -> CmdResult<()> {
    blocking(state, move |s| s.library.save_settings(&settings)).await
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
