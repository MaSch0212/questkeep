//! Tauri commands. Each one is a thin wrapper that runs a core call on the blocking pool and turns
//! errors into user-readable strings. All logic lives in `questkeep-core`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use questkeep_core::auth::AccountStatus;
use questkeep_core::backup::BackupInfo;
use questkeep_core::library::{AccountWarning, MetadataSourceInfo};
use questkeep_core::model::{
    Achievements, BulkChange, Category, CombinePlan, CompletionSettings, Game, GameActivity,
    GameEditInfo, GameEdits, GameNews, GameQuery, GameUpdate, ImageOption, ImageSlot, ImageSource,
    LaunchProfiles, LibraryView, ListField, NewManualGame, NewView, ProfileInput, ScanSummary,
    SectionOrder, SetupState, StartPage, SyncTimes, ValueCount, ViewSection, ViewUiState,
    ViewUpdate,
};
use questkeep_core::providers::ProviderInfo;
use questkeep_core::settings::Settings;
use questkeep_core::{AppInfo, CoreError};
use tauri::State;
use uuid::Uuid;

use crate::updates::{UpdateInfo, Updates};
use crate::websearch::WebImageSize;

use crate::AppState;

pub(crate) type CmdResult<T> = Result<T, String>;

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

fn parse_ids(ids: &[String]) -> Result<Vec<Uuid>, CoreError> {
    ids.iter().map(|id| parse_id(id)).collect()
}

#[tauri::command]
pub async fn bulk_change(
    state: State<'_, Arc<AppState>>,
    ids: Vec<String>,
    change: BulkChange,
) -> CmdResult<()> {
    blocking(state, move |s| {
        s.library.bulk_change(&parse_ids(&ids)?, &change)
    })
    .await
}

#[tauri::command]
pub async fn combine_with_plan(
    state: State<'_, Arc<AppState>>,
    plan: CombinePlan,
) -> CmdResult<Game> {
    blocking(state, move |s| s.library.combine_with_plan(&plan)).await
}

#[tauri::command]
pub async fn remove_games(state: State<'_, Arc<AppState>>, ids: Vec<String>) -> CmdResult<()> {
    blocking(state, move |s| s.library.remove_games(&parse_ids(&ids)?)).await
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

/// Opens one of a provider's extra ways to add games (e.g. Prism Launcher's "Add Instance"
/// dialog). A worker then checks the provider every few seconds for up to ten minutes and
/// imports the first new game it sees, so the game shows up without a manual sync.
#[tauri::command]
pub async fn add_game_with_option(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    option_id: String,
) -> CmdResult<()> {
    let app_state = Arc::clone(&state);
    blocking(state, move |s| {
        let before = s.library.run_add_game_option(&provider_id, &option_id)?;
        std::thread::Builder::new()
            .name("add-game-watch".into())
            .spawn(move || {
                let added = app_state.library.wait_for_new_games(
                    &provider_id,
                    &before,
                    std::time::Duration::from_secs(5),
                    std::time::Duration::from_secs(600),
                );
                if added {
                    app_state.queue_downloads();
                }
            })
            .map_err(|e| CoreError::Invalid(format!("Could not watch for the new game: {e}")))?;
        Ok(())
    })
    .await
}

#[tauri::command]
/// Plays a game: a launch profile (`profile_id`), the store's copy (`copy_id`) of a game owned
/// in several, or else the default profile.
pub async fn launch_game(
    state: State<'_, Arc<AppState>>,
    id: String,
    copy_id: Option<String>,
    profile_id: Option<String>,
) -> CmdResult<()> {
    blocking(state, move |s| {
        let id = parse_id(&id)?;
        let game = match copy_id.as_deref().map(parse_id).transpose()? {
            Some(copy) => s.library.play_target(id, Some(copy))?,
            None => {
                let profile = profile_id.as_deref().map(parse_id).transpose()?;
                s.library.launch_target(id, profile)?
            }
        };
        s.launcher.launch(&game)
    })
    .await
}

/// Starts installing an owned game through its store client.
#[tauri::command]
pub async fn install_game(
    state: State<'_, Arc<AppState>>,
    id: String,
    copy_id: Option<String>,
) -> CmdResult<()> {
    blocking(state, move |s| {
        let copy = copy_id.as_deref().map(parse_id).transpose()?;
        s.library.install_game(parse_id(&id)?, copy)
    })
    .await
}

/// Hands the uninstall to the store client or the game's uninstaller, which asks the user.
#[tauri::command]
pub async fn uninstall_game(
    state: State<'_, Arc<AppState>>,
    id: String,
    copy_id: Option<String>,
) -> CmdResult<()> {
    blocking(state, move |s| {
        let copy = copy_id.as_deref().map(parse_id).transpose()?;
        s.library.uninstall_game(parse_id(&id)?, copy)
    })
    .await
}

/// Combines other games into `into` (one game owned in several stores); returns the entry.
#[tauri::command]
pub async fn combine_games(
    state: State<'_, Arc<AppState>>,
    into: String,
    others: Vec<String>,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        let others = others
            .iter()
            .map(|id| parse_id(id))
            .collect::<Result<Vec<_>, _>>()?;
        s.library.combine_games(parse_id(&into)?, &others)
    })
    .await
}

/// Takes a store's copy out of its combined game; returns the remaining game.
#[tauri::command]
pub async fn separate_copy(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<Game> {
    blocking(state, move |s| s.library.separate_copy(parse_id(&id)?)).await
}

/// Makes a store's copy the one Play starts; returns its game.
#[tauri::command]
pub async fn prefer_copy(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<Game> {
    blocking(state, move |s| s.library.prefer_copy(parse_id(&id)?)).await
}

/// Groups of games that look like the same game in different stores.
#[tauri::command]
pub async fn find_duplicates(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<Vec<Game>>> {
    blocking(state, |s| s.library.find_duplicates()).await
}

/// Remembers that these games are different, so they aren't suggested together again.
#[tauri::command]
pub async fn dismiss_duplicates(
    state: State<'_, Arc<AppState>>,
    ids: Vec<String>,
) -> CmdResult<()> {
    blocking(state, move |s| {
        let ids = ids
            .iter()
            .map(|id| parse_id(id))
            .collect::<Result<Vec<_>, _>>()?;
        s.library.dismiss_duplicates(&ids)
    })
    .await
}

/// Re-downloads store metadata (and a missing background) for one game in the background.
#[tauri::command]
pub async fn refresh_metadata(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| {
        let jobs = s.library.metadata_jobs(parse_id(&id)?)?;
        s.metadata.enqueue(jobs);
        Ok(())
    })
    .await
}

/// Re-downloads metadata for every game in the background (Settings → Metadata). Returns how
/// many jobs were queued.
#[tauri::command]
pub async fn refresh_all_metadata(state: State<'_, Arc<AppState>>) -> CmdResult<usize> {
    blocking(state, move |s| {
        let jobs = s.library.all_metadata_jobs()?;
        Ok(s.metadata.enqueue(jobs))
    })
    .await
}

/// The values of a list field with game counts, for a filter's value list.
#[tauri::command]
pub async fn list_values(
    state: State<'_, Arc<AppState>>,
    field: ListField,
) -> CmdResult<Vec<ValueCount>> {
    blocking(state, move |s| s.library.list_values(field)).await
}

/// The metadata sources the user can order in settings.
#[tauri::command]
pub async fn metadata_sources(
    state: State<'_, Arc<AppState>>,
) -> CmdResult<Vec<MetadataSourceInfo>> {
    blocking(state, move |s| Ok(s.library.metadata_sources())).await
}

#[tauri::command]
pub async fn completion_settings(state: State<'_, Arc<AppState>>) -> CmdResult<CompletionSettings> {
    blocking(state, move |s| s.library.completion_settings()).await
}

/// Saves the completion status list; games of removed statuses get `replacements[id]`.
#[tauri::command]
pub async fn set_completion_settings(
    state: State<'_, Arc<AppState>>,
    settings: CompletionSettings,
    replacements: BTreeMap<String, Option<String>>,
) -> CmdResult<CompletionSettings> {
    blocking(state, move |s| {
        s.library.set_completion_settings(&settings, &replacements)
    })
    .await
}

/// Sets (or clears) the completion status of games.
#[tauri::command]
pub async fn set_completion_status(
    state: State<'_, Arc<AppState>>,
    ids: Vec<String>,
    status: Option<String>,
) -> CmdResult<()> {
    blocking(state, move |s| {
        let ids = ids
            .iter()
            .map(|id| parse_id(id))
            .collect::<Result<Vec<_>, _>>()?;
        s.library.set_completion_status(&ids, status.as_deref())
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
        if before.detect_external_launches != settings.detect_external_launches {
            s.launcher.set_watching(settings.detect_external_launches);
        }
        if before.language != settings.language {
            crate::tray::language_changed();
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

/// Opens a game's install folder in the file manager, from Rust for the same reason.
#[tauri::command]
pub async fn open_install_folder(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let folder = blocking(state, move |s| s.library.install_folder(parse_id(&id)?)).await?;
    app.opener()
        .open_path(folder.display().to_string(), None::<&str>)
        .map_err(|e| {
            tracing::warn!(error = %e, "could not open an install folder");
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
    let ((login_url, window), profile) = {
        let provider = provider.clone();
        blocking(state.clone(), move |s| {
            let profile = s.library.paths().webview_dir().join(&provider);
            Ok((s.library.login(&provider)?, profile))
        })
        .await?
    };
    match crate::signin::run(&app, &provider, &login_url, window, profile).await? {
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

/// Asks for a newer release now (Settings); the title bar and tray learn about it too.
#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> CmdResult<Option<UpdateInfo>> {
    crate::updates::check(&app).await
}

/// The update found by the last check, if any (the window asks when it loads).
#[tauri::command]
pub fn available_update(updates: State<'_, Updates>) -> Option<UpdateInfo> {
    updates.available()
}

/// Whether the tray asked to show the update dialog while the window wasn't listening.
#[tauri::command]
pub fn take_update_request(updates: State<'_, Updates>) -> bool {
    updates.take_show_request()
}

/// Downloads and installs the newest release, then restarts the app.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> CmdResult<()> {
    crate::updates::install(&app).await
}

#[tauri::command]
pub async fn count_games(
    state: State<'_, Arc<AppState>>,
    queries: Vec<GameQuery>,
) -> CmdResult<Vec<u64>> {
    blocking(state, move |s| s.library.count_games(&queries)).await
}

#[tauri::command]
pub async fn list_views(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<ViewSection>> {
    blocking(state, |s| s.library.list_views()).await
}

#[tauri::command]
pub async fn create_view(state: State<'_, Arc<AppState>>, view: NewView) -> CmdResult<LibraryView> {
    blocking(state, move |s| s.library.create_view(&view)).await
}

#[tauri::command]
pub async fn update_view(
    state: State<'_, Arc<AppState>>,
    id: String,
    update: ViewUpdate,
) -> CmdResult<LibraryView> {
    blocking(state, move |s| s.library.update_view(&id, &update)).await
}

#[tauri::command]
pub async fn duplicate_view(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<LibraryView> {
    blocking(state, move |s| s.library.duplicate_view(&id)).await
}

#[tauri::command]
pub async fn delete_view(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.delete_view(&id)).await
}

#[tauri::command]
pub async fn create_view_section(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> CmdResult<ViewSection> {
    blocking(state, move |s| s.library.create_view_section(&name)).await
}

#[tauri::command]
pub async fn rename_view_section(
    state: State<'_, Arc<AppState>>,
    id: String,
    name: String,
) -> CmdResult<()> {
    blocking(state, move |s| s.library.rename_view_section(&id, &name)).await
}

#[tauri::command]
pub async fn delete_view_section(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.delete_view_section(&id)).await
}

#[tauri::command]
pub async fn arrange_views(
    state: State<'_, Arc<AppState>>,
    order: Vec<SectionOrder>,
) -> CmdResult<()> {
    blocking(state, move |s| s.library.arrange_views(&order)).await
}

#[tauri::command]
pub async fn sync_times(state: State<'_, Arc<AppState>>) -> CmdResult<SyncTimes> {
    blocking(state, |s| s.library.sync_times()).await
}

#[tauri::command]
pub async fn start_page(state: State<'_, Arc<AppState>>) -> CmdResult<StartPage> {
    blocking(state, |s| s.library.start_page()).await
}

#[tauri::command]
pub async fn last_page(state: State<'_, Arc<AppState>>) -> CmdResult<Option<String>> {
    blocking(state, |s| s.library.last_page()).await
}

#[tauri::command]
pub async fn set_last_page(state: State<'_, Arc<AppState>>, page: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.set_last_page(&page)).await
}

#[tauri::command]
pub async fn view_ui_states(
    state: State<'_, Arc<AppState>>,
) -> CmdResult<BTreeMap<String, ViewUiState>> {
    blocking(state, |s| s.library.view_ui_states()).await
}

#[tauri::command]
pub async fn set_view_ui_state(
    state: State<'_, Arc<AppState>>,
    id: String,
    ui: ViewUiState,
) -> CmdResult<()> {
    blocking(state, move |s| s.library.set_view_ui_state(&id, &ui)).await
}

#[tauri::command]
pub async fn setup_state(state: State<'_, Arc<AppState>>) -> CmdResult<SetupState> {
    blocking(state, |s| s.library.setup_state()).await
}

#[tauri::command]
pub async fn finish_setup(state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    blocking(state, |s| s.library.finish_setup()).await
}

#[tauri::command]
pub async fn dismiss_igdb_hint(state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    blocking(state, |s| s.library.dismiss_igdb_hint()).await
}

#[tauri::command]
pub async fn test_igdb_credentials(
    state: State<'_, Arc<AppState>>,
    client_id: String,
    client_secret: String,
) -> CmdResult<()> {
    blocking(state, move |s| {
        s.library.test_igdb_credentials(&client_id, &client_secret)
    })
    .await
}

#[tauri::command]
pub async fn list_categories(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<Category>> {
    blocking(state, |s| s.library.list_categories()).await
}

#[tauri::command]
pub async fn create_category(state: State<'_, Arc<AppState>>, name: String) -> CmdResult<Category> {
    blocking(state, move |s| s.library.create_category(&name)).await
}

#[tauri::command]
pub async fn rename_category(
    state: State<'_, Arc<AppState>>,
    id: String,
    name: String,
) -> CmdResult<Category> {
    blocking(state, move |s| s.library.rename_category(&id, &name)).await
}

#[tauri::command]
pub async fn delete_category(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.delete_category(&id)).await
}

#[tauri::command]
pub async fn reorder_categories(
    state: State<'_, Arc<AppState>>,
    ids: Vec<String>,
) -> CmdResult<()> {
    blocking(state, move |s| s.library.reorder_categories(&ids)).await
}

#[tauri::command]
pub async fn merge_categories(
    state: State<'_, Arc<AppState>>,
    from: String,
    into: String,
) -> CmdResult<Category> {
    blocking(state, move |s| s.library.merge_categories(&from, &into)).await
}

#[tauri::command]
pub async fn separate_category_source(
    state: State<'_, Arc<AppState>>,
    id: String,
    source: String,
    name: String,
) -> CmdResult<Category> {
    blocking(state, move |s| {
        s.library.separate_category_source(&id, &source, &name)
    })
    .await
}

#[tauri::command]
pub async fn set_game_categories(
    state: State<'_, Arc<AppState>>,
    id: String,
    category_ids: Vec<String>,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        s.library.set_game_categories(parse_id(&id)?, &category_ids)
    })
    .await
}

#[tauri::command]
pub async fn list_launchers(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<String>> {
    blocking(state, |s| Ok(s.library.launchers())).await
}

#[tauri::command]
pub async fn open_launcher(state: State<'_, Arc<AppState>>, provider_id: String) -> CmdResult<()> {
    blocking(state, move |s| s.library.open_launcher(&provider_id)).await
}

/// The Questkeep menu's "Exit": quits even when the app would otherwise wait in the tray.
#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    crate::tray::exit(&app);
}

/// How many people play a game right now (game page; `null` when no store can tell).
#[tauri::command]
pub async fn game_player_count(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> CmdResult<Option<u32>> {
    blocking(state, move |s| s.library.player_count(parse_id(&id)?)).await
}

/// A game's latest news (game page).
#[tauri::command]
pub async fn game_news(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<Option<GameNews>> {
    blocking(state, move |s| s.library.game_news(parse_id(&id)?)).await
}

/// The user's achievements in a game (game page).
#[tauri::command]
pub async fn game_achievements(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> CmdResult<Option<Achievements>> {
    blocking(state, move |s| s.library.achievements(parse_id(&id)?)).await
}

/// A game's playtime per week for the last 12 weeks (game page).
#[tauri::command]
pub async fn game_activity(state: State<'_, Arc<AppState>>, id: String) -> CmdResult<GameActivity> {
    blocking(state, move |s| s.library.game_activity(parse_id(&id)?)).await
}

/// What the Edit dialog needs: the user's edits, the automatic values and all links.
#[tauri::command]
pub async fn game_edit_info(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> CmdResult<GameEditInfo> {
    blocking(state, move |s| s.library.game_edit_info(parse_id(&id)?)).await
}

#[tauri::command]
pub async fn save_game_edits(
    state: State<'_, Arc<AppState>>,
    id: String,
    edits: GameEdits,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        s.library.save_game_edits(parse_id(&id)?, &edits)
    })
    .await
}

/// Sets a cover, background or icon from a file path or a picked image URL.
#[tauri::command]
pub async fn set_user_image(
    state: State<'_, Arc<AppState>>,
    id: String,
    slot: ImageSlot,
    source: String,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        s.library.set_user_image(parse_id(&id)?, slot, &source)
    })
    .await
}

#[tauri::command]
pub async fn remove_user_image(
    state: State<'_, Arc<AppState>>,
    id: String,
    slot: ImageSlot,
) -> CmdResult<Game> {
    blocking(state, move |s| {
        s.library.remove_user_image(parse_id(&id)?, slot)
    })
    .await
}

#[tauri::command]
pub async fn image_sources(
    state: State<'_, Arc<AppState>>,
    id: String,
    slot: ImageSlot,
) -> CmdResult<Vec<ImageSource>> {
    blocking(state, move |s| {
        s.library.image_sources(parse_id(&id)?, slot)
    })
    .await
}

#[tauri::command]
pub async fn image_options(
    state: State<'_, Arc<AppState>>,
    id: String,
    slot: ImageSlot,
    source: String,
) -> CmdResult<Vec<ImageOption>> {
    blocking(state, move |s| {
        s.library.image_options(parse_id(&id)?, slot, &source)
    })
    .await
}

/// Searches the web (DuckDuckGo) for images in a hidden window.
#[tauri::command]
pub async fn search_web_images(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    query: String,
    large: bool,
    transparent: bool,
) -> CmdResult<Vec<ImageOption>> {
    let profile = state.library.paths().webview_dir().join("websearch");
    let size = if large {
        WebImageSize::Large
    } else {
        WebImageSize::Any
    };
    tauri::async_runtime::spawn_blocking(move || {
        crate::websearch::search_images(&app, &profile, &query, size, transparent, 60)
    })
    .await
    .map_err(|e| format!("Internal error: {e}"))?
    .map(|images| {
        images
            .into_iter()
            .map(|image| ImageOption {
                url: image.url,
                thumbnail: Some(image.thumbnail),
                width: (image.width > 0).then_some(image.width),
                height: (image.height > 0).then_some(image.height),
                label: image.site,
            })
            .collect()
    })
}

#[tauri::command]
pub async fn launch_profiles(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> CmdResult<LaunchProfiles> {
    blocking(state, move |s| s.library.launch_profiles(parse_id(&id)?)).await
}

/// Adds (`profile_id` none) or changes a launch profile.
#[tauri::command]
pub async fn save_launch_profile(
    state: State<'_, Arc<AppState>>,
    id: String,
    profile_id: Option<String>,
    input: ProfileInput,
) -> CmdResult<LaunchProfiles> {
    blocking(state, move |s| {
        let profile = profile_id.as_deref().map(parse_id).transpose()?;
        s.library
            .save_launch_profile(parse_id(&id)?, profile, &input)
    })
    .await
}

#[tauri::command]
pub async fn delete_launch_profile(
    state: State<'_, Arc<AppState>>,
    id: String,
    profile_id: String,
) -> CmdResult<LaunchProfiles> {
    blocking(state, move |s| {
        s.library
            .delete_launch_profile(parse_id(&id)?, parse_id(&profile_id)?)
    })
    .await
}

#[tauri::command]
pub async fn order_launch_profiles(
    state: State<'_, Arc<AppState>>,
    id: String,
    order: Vec<String>,
) -> CmdResult<LaunchProfiles> {
    blocking(state, move |s| {
        let order = order
            .iter()
            .map(|p| parse_id(p))
            .collect::<Result<Vec<_>, _>>()?;
        s.library.order_launch_profiles(parse_id(&id)?, &order)
    })
    .await
}

/// Makes a profile the default; none goes back to automatic.
#[tauri::command]
pub async fn set_default_launch_profile(
    state: State<'_, Arc<AppState>>,
    id: String,
    profile_id: Option<String>,
) -> CmdResult<LaunchProfiles> {
    blocking(state, move |s| {
        let profile = profile_id.as_deref().map(parse_id).transpose()?;
        s.library
            .set_default_launch_profile(parse_id(&id)?, profile)
    })
    .await
}

/// The backups in the backup folder, newest first.
#[tauri::command]
pub async fn list_backups(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<BackupInfo>> {
    blocking(state, |s| Ok(s.library.backups())).await
}

/// The folder backups go to (the configured one or the default).
#[tauri::command]
pub fn backup_folder(state: State<'_, Arc<AppState>>) -> String {
    state.library.backup_folder().to_string_lossy().into_owned()
}

/// Writes a backup now; progress and the result also arrive as core events.
#[tauri::command]
pub async fn backup_now(state: State<'_, Arc<AppState>>) -> CmdResult<BackupInfo> {
    blocking(state, |s| s.library.run_backup()).await
}

/// Stops the running backup; its partial archive is deleted.
#[tauri::command]
pub fn cancel_backup(state: State<'_, Arc<AppState>>) {
    state.library.cancel_backup();
}

/// Backs up the current library, stages `path` and restarts the app, which then swaps it in.
#[tauri::command]
pub async fn restore_backup(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
) -> CmdResult<()> {
    blocking(state, move |s| {
        s.library.restore_backup(&PathBuf::from(path))
    })
    .await?;
    tracing::info!("restarting to restore a backup");
    app.restart()
}

/// Whether a quit waits for the user because a backup runs (the UI asks after it loads).
#[tauri::command]
pub fn quit_pending(guard: State<'_, crate::backups::QuitGuard>) -> bool {
    guard.is_pending()
}

/// Quits after the running backup: `wait` lets it finish, otherwise it is cancelled.
#[tauri::command]
pub async fn finish_quit(app: tauri::AppHandle, wait: bool) -> CmdResult<()> {
    crate::backups::finish_quit(app, wait).await;
    Ok(())
}

/// The user keeps the app open after all.
#[tauri::command]
pub fn cancel_quit(app: tauri::AppHandle) {
    crate::backups::cancel_quit(&app);
}

/// Store accounts that were signed in and no longer are (not dismissed).
#[tauri::command]
pub async fn account_warnings(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<AccountWarning>> {
    blocking(state, |s| s.library.account_warnings()).await
}

/// Hides a signed-out account's warning until it is signed in again.
#[tauri::command]
pub async fn dismiss_account_warning(
    state: State<'_, Arc<AppState>>,
    provider: String,
) -> CmdResult<()> {
    blocking(state, move |s| s.library.dismiss_account_warning(&provider)).await
}
