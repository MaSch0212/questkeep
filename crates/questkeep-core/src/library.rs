//! `Library`: the facade the Tauri shell talks to. Wires database, providers, images and events.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::auth::{AccountStatus, TokenStore};
use crate::db::{Database, NewGameRecord};
use crate::error::{CoreError, Result};
use crate::events::{CoreEvent, EventBus};
use crate::images::{
    CoverJob, IGDB_CHECKED_KEY, Igdb, ImageCache, ImageKind, MetadataJob, SteamGridDb,
};
use crate::model::{
    ActionKind, Category, Game, GameQuery, GameUpdate, ImportedGame, LibraryView, MANUAL_SOURCE,
    NewManualGame, NewView, PlayAction, ScanSummary, SectionOrder, SyncTimes, TriState,
    ViewSection, ViewUpdate, display_name_from_path,
};
use crate::paths::DataPaths;
use crate::providers::{LibraryProvider, ProviderError, ProviderInfo, ProviderRegistry};
use crate::seed::is_fake_source;
use crate::settings::{Settings, SettingsStore};
use crate::tray::{self, TrayGames};

/// Static information about the running app, shown in settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub logs_dir: String,
    pub portable: bool,
    /// True in debug builds; enables developer tools such as the fake-library seed.
    pub dev_tools: bool,
}

pub struct Library {
    paths: DataPaths,
    db: Arc<Database>,
    providers: ProviderRegistry,
    events: EventBus,
    images: ImageCache,
    settings: SettingsStore,
    /// Shared with the fetchers; `save_settings` keeps its API key current.
    artwork: SteamGridDb,
    /// Shared with the metadata fetcher; `save_settings` keeps its credentials current.
    igdb: Igdb,
}

impl Library {
    /// Opens the library in `paths` (creating directories and the database as needed).
    /// `secrets` holds API keys from the settings (the system keyring in the app).
    pub fn open(
        paths: DataPaths,
        providers: ProviderRegistry,
        events: EventBus,
        secrets: Arc<dyn TokenStore>,
    ) -> Result<Self> {
        paths.ensure_dirs()?;
        let db = Arc::new(Database::open(&paths.database_file())?);
        Ok(Self::with_database(paths, db, providers, events, secrets))
    }

    /// Builds a library around an existing database (tests use an in-memory one).
    pub fn with_database(
        paths: DataPaths,
        db: Arc<Database>,
        providers: ProviderRegistry,
        events: EventBus,
        secrets: Arc<dyn TokenStore>,
    ) -> Self {
        let settings = SettingsStore::new(paths.settings_file(), secrets);
        let loaded = settings.load();
        Self {
            images: ImageCache::new(paths.clone()),
            artwork: SteamGridDb::new(loaded.steamgriddb_api_key),
            igdb: Igdb::new(loaded.igdb_client_id, loaded.igdb_client_secret),
            settings,
            paths,
            db,
            providers,
            events,
        }
    }

    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn paths(&self) -> &DataPaths {
        &self.paths
    }

    pub fn images(&self) -> &ImageCache {
        &self.images
    }

    pub fn providers(&self) -> &ProviderRegistry {
        &self.providers
    }

    pub fn app_info(&self) -> AppInfo {
        AppInfo {
            version: crate::VERSION.to_string(),
            data_dir: self.paths.root().display().to_string(),
            logs_dir: self.paths.logs_dir().display().to_string(),
            portable: self.paths.is_portable(),
            dev_tools: cfg!(debug_assertions),
        }
    }

    /// The data or logs folder (`kind` is `"data"` or `"logs"`), for "open folder" buttons.
    /// Anything else is rejected, so the UI cannot ask to open arbitrary paths.
    pub fn app_folder(&self, kind: &str) -> Result<std::path::PathBuf> {
        match kind {
            "data" => Ok(self.paths.root().to_path_buf()),
            "logs" => Ok(self.paths.logs_dir()),
            other => Err(CoreError::Invalid(format!("Unknown folder '{other}'"))),
        }
    }

    /// A game's install directory, for opening it in the file manager.
    pub fn install_folder(&self, id: Uuid) -> Result<std::path::PathBuf> {
        self.db
            .get_entry(id)?
            .install_dir
            .map(std::path::PathBuf::from)
            .ok_or_else(|| CoreError::Invalid("The game's install folder is unknown".into()))
    }

    /// The game lists of the tray menu (recently played, favorites). One query over installed,
    /// non-hidden games; the shell calls this on a background thread when the library changed.
    pub fn tray_games(&self) -> Result<TrayGames> {
        let games = self.db.list_games(&GameQuery {
            installed: TriState::Yes,
            ..GameQuery::default()
        })?;
        // Icons are shown from their absolute path; files that are gone count as missing, which
        // makes the shell queue a new icon job.
        Ok(tray::tray_games(&games).map_images(|relative| {
            self.images
                .resolve(&relative.to_string_lossy().replace('\\', "/"))
        }))
    }

    pub fn settings(&self) -> Settings {
        self.settings.load()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.settings.save(settings)?;
        self.artwork
            .set_api_key(settings.steamgriddb_api_key.clone());
        self.igdb.set_credentials(
            settings.igdb_client_id.clone(),
            settings.igdb_client_secret.clone(),
        );
        Ok(())
    }

    /// The IGDB client, to hand to the metadata fetcher.
    pub fn igdb(&self) -> &Igdb {
        &self.igdb
    }

    /// The SteamGridDB client, to hand to the image fetchers.
    pub fn artwork(&self) -> &SteamGridDb {
        &self.artwork
    }

    /// Converts stored relative image paths into absolute ones for the UI; drops missing files.
    fn resolve_images(&self, mut game: Game) -> Game {
        let resolve = |p: Option<String>| {
            p.as_deref()
                .and_then(|p| self.images.resolve(p))
                .map(|p| p.display().to_string())
        };
        game.cover_path = resolve(game.cover_path.take());
        game.icon_path = resolve(game.icon_path.take());
        game.background_path = resolve(game.background_path.take());
        game
    }

    pub fn list_games(&self, query: &GameQuery) -> Result<Vec<Game>> {
        Ok(self
            .db
            .list_games(query)?
            .into_iter()
            .map(|g| self.resolve_images(g))
            .collect())
    }

    /// How many games each query matches, in order (the sidebar's view counts).
    pub fn count_games(&self, queries: &[GameQuery]) -> Result<Vec<u64>> {
        queries.iter().map(|q| self.db.count_matching(q)).collect()
    }

    pub fn list_views(&self) -> Result<Vec<ViewSection>> {
        self.db.list_views()
    }

    pub fn create_view(&self, view: &NewView) -> Result<LibraryView> {
        self.db.create_view(view)
    }

    pub fn update_view(&self, id: &str, update: &ViewUpdate) -> Result<LibraryView> {
        self.db.update_view(id, update)
    }

    pub fn duplicate_view(&self, id: &str) -> Result<LibraryView> {
        self.db.duplicate_view(id)
    }

    pub fn delete_view(&self, id: &str) -> Result<()> {
        self.db.delete_view(id)
    }

    pub fn create_view_section(&self, name: &str) -> Result<ViewSection> {
        self.db.create_section(name)
    }

    pub fn rename_view_section(&self, id: &str, name: &str) -> Result<()> {
        self.db.rename_section(id, name)
    }

    pub fn delete_view_section(&self, id: &str) -> Result<()> {
        self.db.delete_section(id)
    }

    pub fn arrange_views(&self, order: &[SectionOrder]) -> Result<()> {
        self.db.arrange_views(order)
    }

    /// When each store, and the last "sync all", finished syncing.
    pub fn sync_times(&self) -> Result<SyncTimes> {
        self.db.sync_times()
    }

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        self.db.list_categories()
    }

    /// Creates a category (or returns the one with that name already).
    pub fn create_category(&self, name: &str) -> Result<Category> {
        let category = self.db.create_category(name)?;
        self.events.publish(CoreEvent::CategoriesChanged);
        Ok(category)
    }

    pub fn rename_category(&self, id: &str, name: &str) -> Result<Category> {
        let category = self.db.rename_category(id, name)?;
        self.events.publish(CoreEvent::CategoriesChanged);
        Ok(category)
    }

    pub fn delete_category(&self, id: &str) -> Result<()> {
        self.db.delete_category(id)?;
        self.events.publish(CoreEvent::CategoriesChanged);
        Ok(())
    }

    /// Puts the categories in this order (the rest follow in their old order).
    pub fn reorder_categories(&self, ids: &[String]) -> Result<()> {
        self.db.reorder_categories(ids)?;
        self.events.publish(CoreEvent::CategoriesChanged);
        Ok(())
    }

    /// Merges `from` into `into` (games and store categories) and returns `into`.
    pub fn merge_categories(&self, from: &str, into: &str) -> Result<Category> {
        let changed = self.db.merge_categories(from, into)?;
        self.categories_changed(changed);
        self.db.category(into)
    }

    /// Gives a store category merged into `id` its own category again and returns that one.
    pub fn separate_category_source(&self, id: &str, source: &str, name: &str) -> Result<Category> {
        let (target, changed) = self.db.separate_category_source(id, source, name)?;
        self.categories_changed(changed);
        self.db.category(&target)
    }

    fn categories_changed(&self, games: Vec<Uuid>) {
        self.events.publish(CoreEvent::CategoriesChanged);
        if !games.is_empty() {
            self.events.publish(CoreEvent::GamesChanged { ids: games });
        }
    }

    /// Gives a game exactly these categories.
    pub fn set_game_categories(&self, id: Uuid, category_ids: &[String]) -> Result<Game> {
        self.db.set_game_categories(id, category_ids)?;
        self.events.publish(CoreEvent::CategoriesChanged);
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![id] });
        self.get_game(id)
    }

    /// Ids of the providers whose store launcher is installed on this PC, in registry order.
    pub fn launchers(&self) -> Vec<String> {
        self.launcher_infos()
            .into_iter()
            .map(|info| info.id)
            .collect()
    }

    /// The stores whose own launcher is installed, with their names and logos (the tray lists
    /// them). Cheap file and registry checks per store.
    pub fn launcher_infos(&self) -> Vec<ProviderInfo> {
        self.provider_infos()
            .into_iter()
            .filter(|info| {
                self.providers
                    .get(&info.id)
                    .is_some_and(|p| p.launcher().is_some())
            })
            .collect()
    }

    /// Starts a store's own launcher.
    pub fn open_launcher(&self, provider_id: &str) -> Result<()> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or_else(|| CoreError::UnknownProvider(provider_id.to_string()))?;
        let action = provider.launcher().ok_or_else(|| {
            CoreError::Invalid(format!("{} isn't installed on this PC.", provider.name()))
        })?;
        run_store_action(&action)
            .map_err(|err| CoreError::Launch(format!("Could not open {}: {err}", provider.name())))
    }

    /// The library entry of a game (the entry a combined copy belongs to), with all its copies.
    pub fn get_game(&self, id: Uuid) -> Result<Game> {
        Ok(self.resolve_images(self.db.get_entry(id)?))
    }

    /// The copy of an entry to play: `copy` when given (it must belong to the entry), else the
    /// first installed copy in the entry's play order, else its first copy.
    pub fn play_target(&self, id: Uuid, copy: Option<Uuid>) -> Result<Game> {
        let entry = self.db.get_entry(id)?;
        let chosen = match copy {
            Some(copy) => {
                entry
                    .copies
                    .iter()
                    .find(|c| c.id == copy)
                    .ok_or_else(|| {
                        CoreError::Invalid(format!(
                            "That store's copy isn't part of '{}'.",
                            entry.name
                        ))
                    })?
                    .id
            }
            None => entry
                .copies
                .iter()
                .find(|c| c.installed)
                .or(entry.copies.first())
                .map_or(entry.id, |c| c.id),
        };
        self.db.get_game(chosen)
    }

    /// Checks that `copy` is one of the copies of `id`'s entry.
    fn copy_of(&self, id: Uuid, copy: Uuid) -> Result<Uuid> {
        Ok(self.play_target(id, Some(copy))?.id)
    }

    /// Combines other entries into `into`'s entry (one game owned in several stores) and
    /// returns the combined entry.
    pub fn combine_games(&self, into: Uuid, others: &[Uuid]) -> Result<Game> {
        let combined = self.db.combine_games(into, others)?;
        for id in &combined.absorbed {
            self.events.publish(CoreEvent::GameRemoved { id: *id });
        }
        self.events.publish(CoreEvent::GamesChanged {
            ids: vec![combined.entry],
        });
        self.events.publish(CoreEvent::CategoriesChanged);
        self.get_game(combined.entry)
    }

    /// Takes a store's copy out of its entry again and returns the remaining entry.
    pub fn separate_copy(&self, copy: Uuid) -> Result<Game> {
        let entry = self.db.separate_copy(copy)?;
        self.events.publish(CoreEvent::GamesChanged {
            ids: vec![entry, copy],
        });
        self.events.publish(CoreEvent::CategoriesChanged);
        self.get_game(entry)
    }

    /// Makes a copy the one Play starts (when installed) and returns its entry.
    pub fn prefer_copy(&self, copy: Uuid) -> Result<Game> {
        let entry = self.db.prefer_copy(copy)?;
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![entry] });
        self.get_game(entry)
    }

    /// Groups of entries that look like the same game in different stores.
    pub fn find_duplicates(&self) -> Result<Vec<Vec<Game>>> {
        Ok(self
            .db
            .find_duplicates()?
            .into_iter()
            .map(|group| group.into_iter().map(|g| self.resolve_images(g)).collect())
            .collect())
    }

    /// Remembers that these entries are different games.
    pub fn dismiss_duplicates(&self, ids: &[Uuid]) -> Result<()> {
        self.db.dismiss_duplicates(ids)
    }

    pub fn add_manual_game(&self, input: &NewManualGame) -> Result<Game> {
        let executable = input.executable.trim();
        if executable.is_empty() {
            return Err(CoreError::Invalid("Choose an executable to add.".into()));
        }
        let exe_path = Path::new(executable);
        if !exe_path.is_file() {
            return Err(CoreError::Invalid(format!(
                "The file '{executable}' does not exist."
            )));
        }
        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| display_name_from_path(executable));
        let non_empty = |s: &Option<String>| {
            s.as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let action = PlayAction {
            args: non_empty(&input.args),
            working_dir: non_empty(&input.working_dir),
            ..PlayAction::file(executable)
        };
        let record = NewGameRecord {
            name,
            source: MANUAL_SOURCE.into(),
            source_game_id: None,
            install_dir: exe_path.parent().map(|p| p.display().to_string()),
            play_action: Some(action),
            cover_path: None,
        };
        let game = self.db.insert_game(&record, Utc::now())?;
        if let Some(cover) = non_empty(&input.cover_image) {
            // A bad cover must not lose the game the user just added; report and continue.
            match self
                .images
                .import_image(ImageKind::Cover, game.id, Path::new(&cover))
            {
                Ok(rel) => self.db.set_cover_path(game.id, Some(&rel))?,
                Err(err) => tracing::warn!(%err, "could not import cover image"),
            }
        }
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![game.id] });
        self.get_game(game.id)
    }

    pub fn update_game(&self, id: Uuid, update: &GameUpdate) -> Result<Game> {
        self.db.update_game(id, update)?;
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![id] });
        self.get_game(id)
    }

    /// Replaces the cover with a user-picked image, or clears it with `None`.
    pub fn set_cover(&self, id: Uuid, image: Option<&Path>) -> Result<Game> {
        self.db.get_game(id)?;
        let relative = match image {
            Some(path) => Some(self.images.import_image(ImageKind::Cover, id, path)?),
            None => {
                self.images.remove_image(ImageKind::Cover, id)?;
                None
            }
        };
        self.db.set_cover_path(id, relative.as_deref())?;
        // An icon cropped from the old cover is made again from the new one.
        let game = self.db.get_game(id)?;
        if let Err(err) = crate::images::drop_cover_icon(&self.db, &self.images, &game) {
            tracing::warn!(%err, %id, "could not drop the icon made from the old cover");
        }
        self.events
            .publish(CoreEvent::GamesChanged { ids: vec![id] });
        self.get_game(id)
    }

    pub fn remove_game(&self, id: Uuid) -> Result<()> {
        self.db.delete_game(id)?;
        if let Err(err) = self.images.remove_all(id) {
            tracing::warn!(%err, %id, "could not delete cached images");
        }
        self.events.publish(CoreEvent::GameRemoved { id });
        Ok(())
    }

    pub fn provider_infos(&self) -> Vec<ProviderInfo> {
        let settings = self.settings();
        self.providers
            .infos(|id| settings.provider_config(id).clone())
    }

    fn account_provider(
        &self,
        provider_id: &str,
    ) -> Result<Arc<dyn crate::providers::LibraryProvider>> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or_else(|| CoreError::UnknownProvider(provider_id.to_string()))?;
        if provider.account().is_none() {
            return Err(CoreError::Invalid(format!(
                "{} has no account sign-in.",
                provider.name()
            )));
        }
        Ok(provider)
    }

    /// Sign-in state of every store that supports an account.
    pub fn account_statuses(&self) -> Vec<AccountStatus> {
        self.providers
            .iter()
            .filter_map(|p| p.account().map(|a| a.status()))
            .collect()
    }

    /// The page the in-app login window opens for a provider.
    pub fn login_url(&self, provider_id: &str) -> Result<String> {
        let provider = self.account_provider(provider_id)?;
        Ok(provider
            .account()
            .map(|a| a.login_url())
            .unwrap_or_default())
    }

    /// Completes a sign-in with a code (or, for GOG, the redirect URL) captured by the login
    /// window or pasted by the user, then scans the provider so owned games show up. A failing
    /// scan is logged, not an error: the account is connected either way.
    pub fn sign_in_with_code(&self, provider_id: &str, input: &str) -> Result<AccountStatus> {
        let provider = self.account_provider(provider_id)?;
        let Some(account) = provider.account() else {
            return Err(CoreError::UnknownProvider(provider_id.to_string()));
        };
        account.sign_in_with_code(input)?;
        // Metadata that needs the account was skipped or marked "none" while signed out.
        if let Err(err) = self.db.reset_metadata_checked(provider.id()) {
            tracing::warn!(%err, "could not reset metadata markers");
        }
        if let Err(err) = self.scan(provider_id) {
            tracing::warn!(provider = provider_id, %err, "scan after sign-in failed");
        }
        Ok(account.status())
    }

    /// Forgets a provider's tokens. Games stay in the library; owned-only games are simply no
    /// longer refreshed by scans.
    pub fn sign_out(&self, provider_id: &str) -> Result<AccountStatus> {
        let provider = self.account_provider(provider_id)?;
        let Some(account) = provider.account() else {
            return Err(CoreError::UnknownProvider(provider_id.to_string()));
        };
        account.sign_out()?;
        Ok(account.status())
    }

    /// Runs a provider's installed-games scan, plus its owned-games scan when supported and
    /// configured, and merges the result into the library. Fails only when nothing could be
    /// scanned. Announced as `ScanStarted`, then `LibraryScanned` or `ScanFailed`, so the UI can
    /// show one sync status for all scans.
    pub fn scan(&self, provider_id: &str) -> Result<ScanSummary> {
        self.events.publish(CoreEvent::ScanStarted {
            provider: provider_id.to_string(),
        });
        let result = self.scan_provider(provider_id);
        if let Err(err) = &result {
            self.events.publish(CoreEvent::ScanFailed {
                provider: provider_id.to_string(),
                message: err.to_string(),
            });
        }
        result
    }

    fn scan_provider(&self, provider_id: &str) -> Result<ScanSummary> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or_else(|| CoreError::UnknownProvider(provider_id.to_string()))?;
        let settings = self.settings();
        let config = settings.provider_config(provider_id);
        if provider.can_scan() && !provider.is_installed() && !provider.has_owned_source(config) {
            return Err(CoreError::Invalid(format!(
                "{} isn't installed on this PC. Sign in under Settings → Stores and accounts to sync the games you own.",
                provider.name()
            )));
        }
        provider.configure(config);
        let owned = if provider.can_scan_owned() {
            match provider.scan_owned(config) {
                Ok(games) => Some(games),
                Err(ProviderError::NotConfigured(reason)) => {
                    tracing::debug!(provider = provider_id, %reason, "owned scan skipped");
                    None
                }
                Err(err) => {
                    tracing::warn!(provider = provider_id, %err, "owned scan failed");
                    None
                }
            }
        } else {
            None
        };
        let installed = match provider.scan_installed() {
            Ok(games) => games,
            // The store client may be missing while the account is still readable online.
            Err(err) if owned.is_some() => {
                tracing::warn!(provider = provider_id, %err, "installed scan failed");
                Vec::new()
            }
            Err(err) => {
                return Err(CoreError::Provider {
                    provider: provider.name().to_string(),
                    message: err.to_string(),
                });
            }
        };
        let games = merge_owned(installed, owned.unwrap_or_default());
        let now = Utc::now();
        let mut summary = self.db.upsert_imported(provider.id(), &games, now)?;
        self.import_categories(provider.as_ref(), config, &mut summary);
        if let Err(err) = self.db.record_sync(Some(provider_id), now) {
            tracing::warn!(provider = provider_id, %err, "could not record the sync time");
        }
        tracing::info!(
            provider = provider_id,
            found = games.len(),
            added = summary.added,
            updated = summary.updated,
            uninstalled = summary.uninstalled,
            "library scan finished"
        );
        if !summary.changed_ids.is_empty() {
            self.events.publish(CoreEvent::GamesChanged {
                ids: summary.changed_ids.clone(),
            });
        }
        self.events.publish(CoreEvent::LibraryScanned {
            provider: provider_id.to_string(),
            summary: summary.clone(),
        });
        Ok(summary)
    }

    /// Applies the store's own categories after a scan. Failures only cost the categories.
    fn import_categories(
        &self,
        provider: &dyn LibraryProvider,
        config: &serde_json::Value,
        summary: &mut ScanSummary,
    ) {
        let by_game = match provider.scan_categories(config) {
            Ok(Some(by_game)) => by_game,
            Ok(None) => return,
            Err(err) => {
                tracing::warn!(provider = provider.id(), %err, "reading store categories failed");
                return;
            }
        };
        match self.db.sync_imported_categories(provider.id(), &by_game) {
            Ok(changed) => {
                if !changed.is_empty() {
                    self.events.publish(CoreEvent::CategoriesChanged);
                }
                for id in changed {
                    if !summary.changed_ids.contains(&id) {
                        summary.changed_ids.push(id);
                    }
                }
            }
            Err(err) => {
                tracing::warn!(provider = provider.id(), %err, "importing categories failed")
            }
        }
    }

    /// Scans every provider that supports scanning; failures are logged, not fatal.
    pub fn scan_all(&self) -> Vec<(String, Result<ScanSummary>)> {
        // Stores whose client is missing are skipped unless an account reads their owned games.
        let scannable: Vec<_> = self
            .provider_infos()
            .into_iter()
            .filter(|p| p.syncable)
            .collect();
        // Lets the UI list the stores still waiting, so the sync isn't "done" between two scans.
        self.events.publish(CoreEvent::ScansQueued {
            providers: scannable.iter().map(|p| p.id.clone()).collect(),
        });
        let results = scannable
            .into_iter()
            .map(|p| {
                let result = self.scan(&p.id);
                if let Err(err) = &result {
                    tracing::warn!(provider = %p.id, %err, "scan failed");
                }
                (p.id, result)
            })
            .collect();
        if let Err(err) = self.db.record_sync(None, Utc::now()) {
            tracing::warn!(%err, "could not record the sync time");
        }
        results
    }

    /// Cover-fetch jobs for games that don't have a cover yet. Store games always get a job (their
    /// provider is asked first). Manual games and games of unregistered providers can only be
    /// served by SteamGridDB, so they get a job only when an API key is configured. Fake games
    /// never get one.
    pub fn missing_cover_jobs(&self) -> Result<Vec<CoverJob>> {
        let has_key = self
            .settings()
            .steamgriddb_api_key
            .is_some_and(|k| !k.trim().is_empty());
        Ok(self
            .db
            .games_without_cover()?
            .into_iter()
            .filter(|(_, source, source_game_id)| {
                !is_fake_source(source)
                    && (has_key
                        || (source_game_id.is_some() && self.providers.get(source).is_some()))
            })
            .map(|(game_id, source, source_game_id)| CoverJob {
                game_id,
                source,
                source_game_id,
                force: false,
            })
            .collect())
    }

    /// Games that have no icon yet, for the icon fetcher. Icons are otherwise only made after a
    /// game's first metadata download or when the tray lists it, so games fetched before a new
    /// icon source existed would never get one.
    pub fn missing_icon_ids(&self) -> Result<Vec<Uuid>> {
        Ok(self
            .db
            .games_without_icon()?
            .into_iter()
            .filter(|(_, source)| !is_fake_source(source))
            .map(|(id, _)| id)
            .collect())
    }

    /// Prepares fetching a game's cover and background from SteamGridDB again, replacing the
    /// current ones: clears the "nothing found" markers and returns the job for the cover
    /// fetcher (which also handles the background). Fails when no API key is configured.
    pub fn refetch_artwork(&self, id: Uuid) -> Result<CoverJob> {
        let game = self.db.get_game(id)?;
        if is_fake_source(&game.source) {
            return Err(CoreError::Invalid("Fake games don't get artwork.".into()));
        }
        if !self
            .settings()
            .steamgriddb_api_key
            .is_some_and(|k| !k.trim().is_empty())
        {
            return Err(CoreError::Invalid(
                "Add a SteamGridDB API key in the settings first.".into(),
            ));
        }
        for kind in [ImageKind::Cover, ImageKind::Background] {
            self.db
                .set_extra_value(id, crate::images::checked_key(kind), None)?;
        }
        Ok(CoverJob {
            game_id: id,
            source: game.source,
            source_game_id: game.source_game_id,
            force: true,
        })
    }

    /// Metadata jobs for store games whose metadata was never fetched and, when IGDB is
    /// configured, for every game without a description or genres that IGDB was not asked about
    /// yet (manual games, stores without metadata or without genres, such as Epic). Those jobs
    /// carry no store id: the store was asked already (or there is none), so only IGDB is asked.
    pub fn missing_metadata_jobs(&self) -> Result<Vec<MetadataJob>> {
        let mut jobs: Vec<MetadataJob> = self
            .db
            .games_needing_metadata()?
            .into_iter()
            .filter(|(_, source, _)| self.providers.get(source).is_some())
            .map(|(game_id, source, source_game_id)| MetadataJob {
                game_id,
                source,
                source_game_id: Some(source_game_id),
            })
            .collect();
        if self.igdb.is_configured() {
            let known: std::collections::HashSet<Uuid> = jobs.iter().map(|j| j.game_id).collect();
            jobs.extend(
                self.db
                    .games_missing_metadata()?
                    .into_iter()
                    .filter(|g| {
                        !known.contains(&g.id)
                            && !is_fake_source(&g.source)
                            && g.extra.get(IGDB_CHECKED_KEY).is_none()
                    })
                    .map(|g| MetadataJob {
                        game_id: g.id,
                        source: g.source,
                        source_game_id: None,
                    }),
            );
        }
        Ok(jobs)
    }

    /// Metadata job for one game (to refresh it on demand). A refresh also lets IGDB look at the
    /// game again, so its "nothing found" marker is cleared. An error for games that neither a
    /// store nor IGDB can serve (readable, so the UI can say why nothing happens).
    pub fn metadata_job(&self, id: Uuid) -> Result<MetadataJob> {
        let game = self.db.get_game(id)?;
        if is_fake_source(&game.source) {
            return Err(CoreError::Invalid("Fake games don't get metadata.".into()));
        }
        let from_store =
            game.source_game_id.is_some() && self.providers.get(&game.source).is_some();
        if !from_store && !self.igdb.is_configured() {
            return Err(CoreError::Invalid(
                "Add IGDB credentials in Settings → Metadata to get metadata for this game.".into(),
            ));
        }
        self.db.set_extra_value(id, IGDB_CHECKED_KEY, None)?;
        Ok(MetadataJob {
            game_id: id,
            source: game.source,
            source_game_id: game.source_game_id,
        })
    }

    /// The store action that installs a game that is owned but not installed. The caller opens
    /// it; installation itself happens in the store client.
    pub fn install_action(&self, id: Uuid) -> Result<PlayAction> {
        let game = self.db.get_game(id)?;
        let not_installable = || {
            CoreError::Invalid(format!(
                "'{}' can't be installed from Questkeep.",
                game.name
            ))
        };
        let provider = self
            .providers
            .get(&game.source)
            .ok_or_else(not_installable)?;
        let source_game_id = game.source_game_id.as_deref().ok_or_else(not_installable)?;
        provider
            .install_action(source_game_id)
            .ok_or_else(not_installable)
    }

    /// Hands the install over to the store client (e.g. opens `steam://install/440`). The next
    /// scan picks the game up as installed.
    /// `copy` picks the store for a game owned in several; by default the first copy (in play
    /// order) that isn't installed and that its store can install.
    pub fn install_game(&self, id: Uuid, copy: Option<Uuid>) -> Result<()> {
        let action = match copy {
            Some(copy) => self.install_action(self.copy_of(id, copy)?)?,
            None => {
                let entry = self.db.get_entry(id)?;
                entry
                    .copies
                    .iter()
                    .filter(|c| !c.installed)
                    .find_map(|c| self.install_action(c.id).ok())
                    .map_or_else(|| self.install_action(entry.id), Ok)?
            }
        };
        run_store_action(&action)
            .map_err(|err| CoreError::Launch(format!("Could not start the install: {err}")))
    }

    /// The store action that uninstalls an installed game (a store URI or the game's own
    /// uninstaller). The caller runs it; the store asks the user to confirm.
    pub fn uninstall_action(&self, id: Uuid) -> Result<PlayAction> {
        let game = self.db.get_game(id)?;
        let not_uninstallable = || {
            CoreError::Invalid(format!(
                "'{}' can't be uninstalled from Questkeep.",
                game.name
            ))
        };
        if !game.installed {
            return Err(CoreError::Invalid(format!(
                "'{}' is not installed.",
                game.name
            )));
        }
        let provider = self
            .providers
            .get(&game.source)
            .ok_or_else(not_uninstallable)?;
        let source_game_id = game
            .source_game_id
            .as_deref()
            .ok_or_else(not_uninstallable)?;
        provider
            .uninstall_action(source_game_id)
            .ok_or_else(not_uninstallable)
    }

    /// Hands the uninstall over to the store client or the game's uninstaller. Questkeep never
    /// deletes files itself and writes nothing to the database: once the game is gone, the next
    /// scan marks it as not installed.
    /// `copy` picks the store for a game installed in several; by default the copy Play starts.
    pub fn uninstall_game(&self, id: Uuid, copy: Option<Uuid>) -> Result<()> {
        let target = match copy {
            Some(copy) => self.copy_of(id, copy)?,
            None => self.play_target(id, None)?.id,
        };
        let action = self.uninstall_action(target)?;
        run_store_action(&action)
            .map_err(|err| CoreError::Launch(format!("Could not start the uninstall: {err}")))
    }

    /// Starts one of a provider's extra ways to add games (e.g. Prism Launcher's "new instance"
    /// dialog) and returns the ids its installed scan sees now, for [`Self::wait_for_new_games`].
    pub fn run_add_game_option(
        &self,
        provider_id: &str,
        option_id: &str,
    ) -> Result<std::collections::HashSet<String>> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or_else(|| CoreError::UnknownProvider(provider_id.to_string()))?;
        if !provider.is_installed() {
            return Err(CoreError::Invalid(format!(
                "{} isn't installed on this PC.",
                provider.name()
            )));
        }
        let before = self.installed_ids(provider_id);
        provider
            .run_add_game_option(option_id)
            .map_err(|err| match err {
                ProviderError::Unsupported => {
                    CoreError::Invalid(format!("{} cannot add games that way.", provider.name()))
                }
                other => CoreError::Launch(format!("Could not open {}: {other}", provider.name())),
            })?;
        Ok(before)
    }

    /// Source ids the provider's installed scan reports right now (empty when it fails).
    fn installed_ids(&self, provider_id: &str) -> std::collections::HashSet<String> {
        self.providers
            .get(provider_id)
            .and_then(|p| p.scan_installed().ok())
            .map(|games| games.into_iter().map(|g| g.source_game_id).collect())
            .unwrap_or_default()
    }

    /// Checks the provider every `interval` until a game that is not in `before` shows up (then
    /// scans it into the library and returns true) or `timeout` passes. Blocks; run it on a
    /// worker thread after [`Self::run_add_game_option`].
    pub fn wait_for_new_games(
        &self,
        provider_id: &str,
        before: &std::collections::HashSet<String>,
        interval: std::time::Duration,
        timeout: std::time::Duration,
    ) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            std::thread::sleep(interval);
            if self
                .installed_ids(provider_id)
                .iter()
                .any(|id| !before.contains(id))
            {
                if let Err(err) = self.scan(provider_id) {
                    tracing::warn!(provider = provider_id, %err, "scan after adding a game failed");
                }
                return true;
            }
        }
        false
    }

    /// Dev tool: inserts `count` fake games to test performance.
    pub fn seed_fake_library(&self, count: usize) -> Result<usize> {
        let records = crate::seed::fake_games(count);
        let inserted = self.db.insert_many(&records, Utc::now())?;
        self.events
            .publish(CoreEvent::GamesChanged { ids: Vec::new() });
        Ok(inserted)
    }
}

/// Opens a store URI or starts a program (installer or uninstaller) without tracking it.
fn run_store_action(action: &PlayAction) -> std::io::Result<()> {
    match action.kind {
        ActionKind::Uri => crate::platform::open_uri(&action.target),
        ActionKind::File => {
            let mut command = std::process::Command::new(&action.target);
            if let Some(args) = action.args.as_deref().filter(|a| !a.trim().is_empty()) {
                crate::platform::add_raw_args(&mut command, args);
            }
            if let Some(dir) = &action.working_dir {
                command.current_dir(dir);
            }
            command.spawn().map(|_| ())
        }
    }
}

/// Combines an installed scan with an owned scan: installed entries win, owned-only games are
/// added as not installed, and store playtime from the owned scan fills gaps.
fn merge_owned(mut installed: Vec<ImportedGame>, owned: Vec<ImportedGame>) -> Vec<ImportedGame> {
    let index: std::collections::HashMap<String, usize> = installed
        .iter()
        .enumerate()
        .map(|(i, g)| (g.source_game_id.clone(), i))
        .collect();
    for game in owned {
        match index.get(&game.source_game_id) {
            Some(&i) => {
                let target = &mut installed[i];
                target.playtime_seconds = target.playtime_seconds.max(game.playtime_seconds);
                target.last_played = target.last_played.max(game.last_played);
            }
            None => installed.push(ImportedGame {
                installed: false,
                ..game
            }),
        }
    }
    installed
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    struct FakeStore(Vec<ImportedGame>);
    impl LibraryProvider for FakeStore {
        fn is_installed(&self) -> bool {
            true
        }
        fn id(&self) -> &str {
            "fake"
        }
        fn name(&self) -> &str {
            "Fake"
        }
        fn icon_svg(&self) -> &'static str {
            "<svg/>"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            if self.0.is_empty() {
                Err(ProviderError::NotFound("Fake store not installed".into()))
            } else {
                Ok(self.0.clone())
            }
        }
    }

    fn library(tmp: &Path, providers: ProviderRegistry) -> (Library, Arc<Mutex<Vec<CoreEvent>>>) {
        let paths = DataPaths::new(tmp.join("data"));
        paths.ensure_dirs().unwrap();
        let events = EventBus::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        events.subscribe(move |e| s.lock().unwrap().push(e.clone()));
        let db = Arc::new(Database::open_in_memory().unwrap());
        let secrets = Arc::new(crate::auth::MemoryTokenStore::default());
        (
            Library::with_database(paths, db, providers, events, secrets),
            seen,
        )
    }

    #[test]
    fn app_folder_resolves_known_kinds_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        assert_eq!(lib.app_folder("data").unwrap(), lib.paths().root());
        assert_eq!(lib.app_folder("logs").unwrap(), lib.paths().logs_dir());
        assert!(matches!(
            lib.app_folder("../etc"),
            Err(CoreError::Invalid(_))
        ));
        assert!(lib.app_folder("").is_err());
    }

    #[test]
    fn adds_manual_game_with_derived_name_and_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, seen) = library(tmp.path(), ProviderRegistry::new());
        let game_dir = tmp.path().join("Outer Wilds");
        std::fs::create_dir_all(&game_dir).unwrap();
        let exe = game_dir.join("game.exe");
        std::fs::write(&exe, b"").unwrap();
        let cover = tmp.path().join("c.jpg");
        std::fs::write(&cover, b"jpg").unwrap();

        let game = lib
            .add_manual_game(&NewManualGame {
                name: None,
                executable: exe.display().to_string(),
                args: Some("  -windowed ".into()),
                working_dir: Some(" ".into()),
                cover_image: Some(cover.display().to_string()),
            })
            .unwrap();
        assert_eq!(game.name, "Outer Wilds");
        assert_eq!(game.source, MANUAL_SOURCE);
        let action = game.play_action.unwrap();
        assert_eq!(action.args.as_deref(), Some("-windowed"));
        assert_eq!(action.working_dir, None);
        let cover_path = game.cover_path.unwrap();
        assert!(Path::new(&cover_path).is_absolute() && Path::new(&cover_path).is_file());
        assert!(matches!(
            seen.lock().unwrap()[0],
            CoreEvent::GamesChanged { .. }
        ));
    }

    #[test]
    fn manual_add_validates_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        let missing = lib.add_manual_game(&NewManualGame {
            name: None,
            executable: tmp.path().join("nope.exe").display().to_string(),
            args: None,
            working_dir: None,
            cover_image: None,
        });
        assert!(matches!(missing, Err(CoreError::Invalid(_))));
    }

    #[test]
    fn scan_merges_and_emits_events() {
        let tmp = tempfile::tempdir().unwrap();
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(FakeStore(vec![ImportedGame {
            play_action: Some(PlayAction::uri("fake://1")),
            ..ImportedGame::installed("1", "Fake Game")
        }])));
        let (lib, seen) = library(tmp.path(), providers);
        let summary = lib.scan("fake").unwrap();
        assert_eq!(summary.added, 1);
        assert_eq!(lib.scan("fake").unwrap().added, 0);
        assert_eq!(lib.list_games(&GameQuery::default()).unwrap().len(), 1);
        let events = seen.lock().unwrap();
        assert!(events.iter().any(
            |e| matches!(e, CoreEvent::LibraryScanned { provider, .. } if provider == "fake")
        ));
        // The next scan publishes events, and the recorder needs this lock.
        drop(events);
        assert!(matches!(
            lib.scan("nope"),
            Err(CoreError::UnknownProvider(_))
        ));
    }

    struct TaggingStore(std::sync::Mutex<Option<HashMap<String, Vec<String>>>>);
    impl LibraryProvider for TaggingStore {
        fn is_installed(&self) -> bool {
            true
        }
        fn id(&self) -> &str {
            "tags"
        }
        fn name(&self) -> &str {
            "Tags"
        }
        fn icon_svg(&self) -> &'static str {
            "<svg/>"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(vec![
                ImportedGame::installed("1", "One"),
                ImportedGame::installed("2", "Two"),
            ])
        }
        fn scan_categories(
            &self,
            _: &serde_json::Value,
        ) -> Result<Option<HashMap<String, Vec<String>>>, ProviderError> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    #[test]
    fn scans_import_store_categories_and_keep_them_when_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(TaggingStore(std::sync::Mutex::new(Some(HashMap::from([
            (
                "1".to_string(),
                vec!["RPG".to_string(), "Co-op".to_string()],
            ),
            ("2".to_string(), vec!["RPG".to_string()]),
        ])))));
        let mut providers = ProviderRegistry::new();
        providers.register(store.clone());
        let (lib, seen) = library(tmp.path(), providers);
        lib.scan("tags").unwrap();
        let counts = |lib: &Library| {
            lib.list_categories()
                .unwrap()
                .into_iter()
                .map(|c| (c.name, c.game_count))
                .collect::<Vec<_>>()
        };
        assert_eq!(counts(&lib), vec![("Co-op".into(), 1), ("RPG".into(), 2)]);
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, CoreEvent::CategoriesChanged))
        );

        // Unreadable this time: the imported categories stay.
        *store.0.lock().unwrap() = None;
        lib.scan("tags").unwrap();
        assert_eq!(counts(&lib), vec![("Co-op".into(), 1), ("RPG".into(), 2)]);

        // A user category survives the store dropping everything.
        let one = lib
            .list_games(&GameQuery::default())
            .unwrap()
            .into_iter()
            .find(|g| g.name == "One")
            .unwrap();
        let mine = lib.create_category("Backlog").unwrap();
        let mut ids = one.categories.clone();
        ids.push(mine.id.clone());
        assert_eq!(
            lib.set_game_categories(one.id, &ids)
                .unwrap()
                .categories
                .len(),
            3
        );
        *store.0.lock().unwrap() = Some(HashMap::new());
        lib.scan("tags").unwrap();
        assert_eq!(counts(&lib), vec![("Backlog".into(), 1)]);
    }

    #[test]
    fn combined_games_play_their_preferred_installed_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, seen) = library(tmp.path(), ProviderRegistry::new());
        let db = lib.database();
        let mut owned = ImportedGame::installed("1", "Rocket League");
        owned.installed = false;
        db.upsert_imported("steam", &[owned], Utc::now()).unwrap();
        db.upsert_imported(
            "epic",
            &[ImportedGame::installed("2", "Rocket League")],
            Utc::now(),
        )
        .unwrap();
        db.upsert_imported(
            "gog",
            &[ImportedGame::installed("3", "Rocket League")],
            Utc::now(),
        )
        .unwrap();
        let ids: Vec<Uuid> = lib
            .list_games(&GameQuery::default())
            .unwrap()
            .iter()
            .map(|g| g.id)
            .collect();
        let copy = |source: &str| {
            lib.list_games(&GameQuery::default())
                .unwrap()
                .iter()
                .flat_map(|g| g.copies.clone())
                .find(|c| c.source == source)
                .unwrap()
                .id
        };
        let (steam, epic, gog) = (copy("steam"), copy("epic"), copy("gog"));
        assert_eq!(ids.len(), 3);
        // Copies follow in the order given.
        let entry = lib.combine_games(steam, &[epic, gog]).unwrap();
        assert_eq!(entry.id, steam);
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, CoreEvent::GameRemoved { id } if *id == epic))
        );
        // Steam's copy comes first but isn't installed: Epic's plays.
        assert_eq!(lib.play_target(steam, None).unwrap().id, epic);
        assert_eq!(lib.play_target(epic, Some(gog)).unwrap().source, "gog");
        lib.prefer_copy(gog).unwrap();
        assert_eq!(lib.play_target(steam, None).unwrap().id, gog);
        db.upsert_imported("xbox", &[ImportedGame::installed("4", "Other")], Utc::now())
            .unwrap();
        assert!(lib.play_target(steam, Some(copy("xbox"))).is_err());
        assert_eq!(lib.get_game(gog).unwrap().copies.len(), 3);
    }

    #[test]
    fn scans_announce_start_queue_and_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let mut providers = ProviderRegistry::new();
        // An empty FakeStore reports "store not installed".
        providers.register(Arc::new(FakeStore(Vec::new())));
        let (lib, seen) = library(tmp.path(), providers);
        let results = lib.scan_all();
        assert!(results[0].1.is_err());
        let events = seen.lock().unwrap();
        let kinds: Vec<_> = events
            .iter()
            .map(|e| match e {
                CoreEvent::ScansQueued { providers } => format!("queued {providers:?}"),
                CoreEvent::ScanStarted { provider } => format!("started {provider}"),
                CoreEvent::ScanFailed { provider, message } => {
                    assert!(message.contains("not installed"));
                    format!("failed {provider}")
                }
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(kinds, [r#"queued ["fake"]"#, "started fake", "failed fake"]);
    }

    /// Launcher whose "new instance" dialog adds a game once it was opened.
    struct DialogStore(Mutex<Vec<ImportedGame>>);
    impl LibraryProvider for DialogStore {
        fn is_installed(&self) -> bool {
            true
        }
        fn id(&self) -> &str {
            "dialog"
        }
        fn name(&self) -> &str {
            "Dialog"
        }
        fn icon_svg(&self) -> &'static str {
            "<svg/>"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn run_add_game_option(&self, option_id: &str) -> Result<(), ProviderError> {
            if option_id != "new" {
                return Err(ProviderError::Unsupported);
            }
            self.0
                .lock()
                .unwrap()
                .push(ImportedGame::installed("2", "New Instance"));
            Ok(())
        }
    }

    #[test]
    fn add_game_option_imports_the_game_added_in_the_launcher() {
        let tmp = tempfile::tempdir().unwrap();
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(DialogStore(Mutex::new(vec![
            ImportedGame::installed("1", "Old Instance"),
        ]))));
        let (lib, _) = library(tmp.path(), providers);
        assert!(matches!(
            lib.run_add_game_option("dialog", "other"),
            Err(CoreError::Invalid(_))
        ));
        assert!(matches!(
            lib.run_add_game_option("nope", "new"),
            Err(CoreError::UnknownProvider(_))
        ));
        let short = std::time::Duration::from_millis(1);
        // Nothing new yet: gives up after the timeout.
        let before = lib.installed_ids("dialog");
        assert!(!lib.wait_for_new_games("dialog", &before, short, short * 5));

        let before = lib.run_add_game_option("dialog", "new").unwrap();
        assert_eq!(before.len(), 1);
        assert!(lib.wait_for_new_games(
            "dialog",
            &before,
            short,
            std::time::Duration::from_secs(5)
        ));
        let names: Vec<_> = lib
            .list_games(&GameQuery::default())
            .unwrap()
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"New Instance".to_string()));
    }

    /// Store whose client is missing but whose account lists owned games when configured.
    struct AccountStore;
    impl LibraryProvider for AccountStore {
        fn is_installed(&self) -> bool {
            true
        }
        fn id(&self) -> &str {
            "acct"
        }
        fn name(&self) -> &str {
            "Account"
        }
        fn icon_svg(&self) -> &'static str {
            "<svg/>"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(vec![ImportedGame {
                playtime_seconds: Some(10),
                ..ImportedGame::installed("1", "Installed")
            }])
        }
        fn can_scan_owned(&self) -> bool {
            true
        }
        fn scan_owned(
            &self,
            config: &serde_json::Value,
        ) -> Result<Vec<ImportedGame>, ProviderError> {
            if config["key"].as_str().is_none() {
                return Err(ProviderError::NotConfigured("no key".into()));
            }
            Ok(vec![
                ImportedGame {
                    playtime_seconds: Some(600),
                    ..ImportedGame::installed("1", "Installed")
                },
                ImportedGame::installed("2", "Owned"),
            ])
        }
        fn install_action(&self, id: &str) -> Option<PlayAction> {
            Some(PlayAction::uri(format!("acct://install/{id}")))
        }
        fn uninstall_action(&self, id: &str) -> Option<PlayAction> {
            (id == "1").then(|| PlayAction::uri(format!("acct://uninstall/{id}")))
        }
    }

    #[test]
    fn owned_scan_adds_uninstalled_games_when_configured() {
        let tmp = tempfile::tempdir().unwrap();
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(AccountStore));
        let (lib, _) = library(tmp.path(), providers);

        assert_eq!(
            lib.scan("acct").unwrap().added,
            1,
            "unconfigured: installed only"
        );

        let mut settings = lib.settings();
        settings
            .providers
            .insert("acct".into(), serde_json::json!({ "key": "k" }));
        lib.save_settings(&settings).unwrap();
        assert_eq!(lib.scan("acct").unwrap().added, 1);

        let games = lib.list_games(&GameQuery::default()).unwrap();
        let installed = games.iter().find(|g| g.name == "Installed").unwrap();
        let owned = games.iter().find(|g| g.name == "Owned").unwrap();
        assert!(installed.installed && !owned.installed);
        assert_eq!(installed.playtime_seconds, 600, "store playtime merged");
        assert_eq!(
            lib.install_action(owned.id).unwrap().target,
            "acct://install/2"
        );
        assert_eq!(
            lib.uninstall_action(installed.id).unwrap().target,
            "acct://uninstall/1"
        );
        let not_installed = lib.uninstall_action(owned.id).unwrap_err().to_string();
        assert!(not_installed.contains("not installed"), "{not_installed}");
        let jobs = lib.missing_metadata_jobs().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].game_id, installed.id, "installed games first");
        assert!(lib.metadata_job(owned.id).is_ok());
    }

    /// A store with an account: owned games appear after signing in with the right code.
    struct SignInStore {
        signed_in: std::sync::atomic::AtomicBool,
    }
    impl crate::auth::AccountAuth for SignInStore {
        fn status(&self) -> AccountStatus {
            AccountStatus {
                provider: "acct".into(),
                name: "Account".into(),
                signed_in: self.signed_in.load(std::sync::atomic::Ordering::SeqCst),
                display_name: None,
                supports_code_sign_in: true,
                code_url: None,
            }
        }
        fn login_url(&self) -> String {
            "https://login.test/".into()
        }
        fn sign_in_with_code(&self, input: &str) -> Result<(), crate::auth::AuthError> {
            if input == "good-code-1234" {
                self.signed_in
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            } else {
                Err(crate::auth::AuthError::Rejected("bad code".into()))
            }
        }
        fn sign_out(&self) -> Result<(), crate::auth::AuthError> {
            self.signed_in
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }
    struct SignInProvider(Arc<SignInStore>);
    impl LibraryProvider for SignInProvider {
        fn is_installed(&self) -> bool {
            true
        }
        fn id(&self) -> &str {
            "acct"
        }
        fn name(&self) -> &str {
            "Account"
        }
        fn icon_svg(&self) -> &'static str {
            "<svg/>"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(vec![ImportedGame::installed("1", "Installed")])
        }
        fn can_scan_owned(&self) -> bool {
            true
        }
        fn scan_owned(&self, _: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
            if crate::auth::AccountAuth::status(self.0.as_ref()).signed_in {
                Ok(vec![ImportedGame::installed("2", "Owned")])
            } else {
                Err(ProviderError::NotConfigured(
                    "Sign in to Account in Settings.".into(),
                ))
            }
        }
        fn account(&self) -> Option<&dyn crate::auth::AccountAuth> {
            Some(self.0.as_ref())
        }
    }

    /// A store whose client isn't installed; its account still reads owned games.
    struct ClientMissing(Arc<SignInStore>);
    impl LibraryProvider for ClientMissing {
        fn id(&self) -> &str {
            "acct"
        }
        fn name(&self) -> &str {
            "Account"
        }
        fn icon_svg(&self) -> &'static str {
            "<svg/>"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Err(ProviderError::NotFound("client not installed".into()))
        }
        fn can_scan_owned(&self) -> bool {
            true
        }
        fn scan_owned(&self, _: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
            let mut owned = ImportedGame::installed("2", "Owned");
            owned.installed = false;
            Ok(vec![owned])
        }
        fn account(&self) -> Option<&dyn crate::auth::AccountAuth> {
            Some(self.0.as_ref())
        }
        fn add_game_options(&self) -> Vec<crate::providers::AddGameOption> {
            vec![crate::providers::AddGameOption {
                id: "new".into(),
                label: "New thing".into(),
            }]
        }
    }

    #[test]
    fn stores_without_their_client_only_sync_when_signed_in() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(SignInStore {
            signed_in: false.into(),
        });
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(ClientMissing(store)));
        let (lib, _) = library(tmp.path(), providers);

        let info = |lib: &Library| lib.provider_infos().remove(0);
        assert!(!info(&lib).installed);
        assert!(!info(&lib).syncable);
        assert!(info(&lib).add_game_options.is_empty());
        assert!(lib.scan_all().is_empty(), "skipped by Sync all");
        let err = lib.scan("acct").unwrap_err().to_string();
        assert!(err.contains("isn't installed"), "{err}");
        assert!(lib.run_add_game_option("acct", "new").is_err());
        assert!(lib.launchers().is_empty());

        lib.sign_in_with_code("acct", "good-code-1234").unwrap();
        assert!(info(&lib).syncable);
        assert!(!info(&lib).installed);
        assert_eq!(lib.scan_all().len(), 1);
        assert_eq!(lib.database().count_games().unwrap(), 1, "the owned game");
    }

    #[test]
    fn sign_in_scans_owned_games_and_sign_out_keeps_them() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(SignInStore {
            signed_in: false.into(),
        });
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(SignInProvider(store)));
        providers.register(Arc::new(FakeStore(vec![ImportedGame::installed("9", "X")])));
        let (lib, _) = library(tmp.path(), providers);

        let statuses = lib.account_statuses();
        assert_eq!(statuses.len(), 1, "only providers with an account");
        assert!(!statuses[0].signed_in);
        assert_eq!(lib.login_url("acct").unwrap(), "https://login.test/");
        assert!(lib.login_url("fake").is_err());
        assert!(matches!(
            lib.login_url("nope"),
            Err(CoreError::UnknownProvider(_))
        ));

        assert_eq!(
            lib.scan("acct").unwrap().added,
            1,
            "signed out: installed only"
        );
        let err = lib.sign_in_with_code("acct", "wrong").unwrap_err();
        assert_eq!(err.to_string(), "bad code");

        let status = lib.sign_in_with_code("acct", "good-code-1234").unwrap();
        assert!(status.signed_in);
        assert_eq!(
            lib.database().count_games().unwrap(),
            2,
            "scan after sign-in"
        );

        assert!(!lib.sign_out("acct").unwrap().signed_in);
        assert_eq!(
            lib.database().count_games().unwrap(),
            2,
            "games are not deleted"
        );
    }

    #[test]
    fn sign_in_makes_games_without_description_fetch_metadata_again() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(SignInStore {
            signed_in: false.into(),
        });
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(SignInProvider(store)));
        let (lib, _) = library(tmp.path(), providers);
        lib.scan("acct").unwrap();
        let id = lib.missing_metadata_jobs().unwrap()[0].game_id;
        lib.database()
            .mark_metadata_checked(id, Utc::now())
            .unwrap();
        assert!(lib.missing_metadata_jobs().unwrap().is_empty());
        lib.sign_in_with_code("acct", "good-code-1234").unwrap();
        assert_eq!(lib.missing_metadata_jobs().unwrap().len(), 2);
    }

    #[test]
    fn manual_games_get_cover_jobs_only_with_a_steamgriddb_key() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        std::fs::write(tmp.path().join("hades.exe"), b"x").unwrap();
        let game = lib
            .add_manual_game(&NewManualGame {
                name: Some("Hades".into()),
                executable: tmp.path().join("hades.exe").display().to_string(),
                args: None,
                working_dir: None,
                cover_image: None,
            })
            .unwrap();
        assert!(lib.missing_cover_jobs().unwrap().is_empty());
        assert!(
            lib.refetch_artwork(game.id).is_err(),
            "no key: readable error"
        );

        let mut settings = lib.settings();
        settings.steamgriddb_api_key = Some("test-key".into());
        lib.save_settings(&settings).unwrap();
        assert!(lib.artwork().is_configured());
        let jobs = lib.missing_cover_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].source_game_id, None);
        assert!(!jobs[0].force);

        lib.database()
            .set_extra_value(game.id, "steamGridDbCoverCheckedAt", Some("x".into()))
            .unwrap();
        let job = lib.refetch_artwork(game.id).unwrap();
        assert!(job.force);
        assert!(
            lib.database()
                .get_game(game.id)
                .unwrap()
                .extra
                .get("steamGridDbCoverCheckedAt")
                .is_none(),
            "marker cleared"
        );
    }

    #[test]
    fn fake_games_never_get_artwork_or_metadata_jobs() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        lib.seed_fake_library(20).unwrap();
        let mut settings = lib.settings();
        settings.steamgriddb_api_key = Some("test-key".into());
        settings.igdb_client_id = Some("id".into());
        settings.igdb_client_secret = Some("secret".into());
        lib.save_settings(&settings).unwrap();
        assert!(lib.artwork().is_configured() && lib.igdb().is_configured());

        assert!(lib.missing_cover_jobs().unwrap().is_empty());
        assert!(lib.missing_metadata_jobs().unwrap().is_empty());
        let fake = lib.database().list_games(&Default::default()).unwrap()[0].clone();
        assert!(lib.metadata_job(fake.id).is_err());
        assert!(lib.refetch_artwork(fake.id).is_err());
    }

    #[test]
    fn store_games_without_genres_get_igdb_only_jobs() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        let game = lib
            .database()
            .insert_game(
                &NewGameRecord {
                    name: "Alan Wake 2".into(),
                    source: "epic".into(),
                    source_game_id: Some("Bonsai".into()),
                    ..Default::default()
                },
                Utc::now(),
            )
            .unwrap();
        let store = crate::model::GameMetadata {
            description: Some("From Epic.".into()),
            ..Default::default()
        };
        lib.database()
            .set_metadata(game.id, &store, Utc::now())
            .unwrap();
        let mut settings = lib.settings();
        settings.igdb_client_id = Some("id".into());
        settings.igdb_client_secret = Some("secret".into());
        lib.save_settings(&settings).unwrap();

        let jobs = lib.missing_metadata_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].game_id, game.id);
        assert_eq!(jobs[0].source_game_id, None, "the store was asked already");
    }

    #[test]
    fn igdb_credentials_enable_metadata_jobs_for_manual_games() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        std::fs::write(tmp.path().join("hades.exe"), b"x").unwrap();
        let game = lib
            .add_manual_game(&NewManualGame {
                name: Some("Hades".into()),
                executable: tmp.path().join("hades.exe").display().to_string(),
                args: None,
                working_dir: None,
                cover_image: None,
            })
            .unwrap();
        assert!(lib.missing_metadata_jobs().unwrap().is_empty());
        assert!(lib.metadata_job(game.id).is_err());

        let mut settings = lib.settings();
        settings.igdb_client_id = Some("id".into());
        assert!(!lib.igdb().is_configured(), "the secret is missing");
        settings.igdb_client_secret = Some("secret".into());
        lib.save_settings(&settings).unwrap();
        assert!(lib.igdb().is_configured());
        // Credentials come back through the keyring-backed settings.
        assert_eq!(lib.settings().igdb_client_secret.as_deref(), Some("secret"));

        let jobs = lib.missing_metadata_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].game_id, game.id);
        assert_eq!(jobs[0].source_game_id, None);

        // "IGDB had nothing" is remembered until the user asks for a refresh.
        lib.database()
            .set_extra_value(game.id, IGDB_CHECKED_KEY, Some("x".into()))
            .unwrap();
        assert!(lib.missing_metadata_jobs().unwrap().is_empty());
        assert!(lib.metadata_job(game.id).is_ok());
        assert_eq!(
            lib.missing_metadata_jobs().unwrap().len(),
            1,
            "marker cleared"
        );

        settings.igdb_client_secret = None;
        lib.save_settings(&settings).unwrap();
        assert!(!lib.igdb().is_configured());
        assert!(lib.missing_metadata_jobs().unwrap().is_empty());
    }

    #[test]
    fn scan_failure_is_readable() {
        let tmp = tempfile::tempdir().unwrap();
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(FakeStore(vec![])));
        let (lib, _) = library(tmp.path(), providers);
        let err = lib.scan("fake").unwrap_err().to_string();
        assert_eq!(err, "Fake scan failed: Fake store not installed");
        assert_eq!(lib.scan_all().len(), 1);
    }

    #[test]
    fn games_without_icons_are_listed_for_the_icon_fetcher() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        lib.seed_fake_library(5).unwrap();
        let exe = tmp.path().join("x.exe");
        std::fs::write(&exe, b"").unwrap();
        let game = lib
            .add_manual_game(&NewManualGame {
                name: Some("A".into()),
                executable: exe.display().to_string(),
                args: None,
                working_dir: None,
                cover_image: None,
            })
            .unwrap();
        assert_eq!(
            lib.missing_icon_ids().unwrap(),
            [game.id],
            "fake games skipped"
        );
        lib.db
            .set_icon_path(game.id, Some("images/icons/a.png"))
            .unwrap();
        assert!(lib.missing_icon_ids().unwrap().is_empty());
    }

    #[test]
    fn new_cover_drops_only_icons_made_from_the_old_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        let exe = tmp.path().join("x.exe");
        std::fs::write(&exe, b"").unwrap();
        let add = |name: &str| {
            lib.add_manual_game(&NewManualGame {
                name: Some(name.into()),
                executable: exe.display().to_string(),
                args: None,
                working_dir: None,
                cover_image: None,
            })
            .unwrap()
        };
        let (cover_icon, own_icon) = (add("A"), add("B"));
        for (game, from_cover) in [(&cover_icon, true), (&own_icon, false)] {
            let rel = lib
                .images
                .store_image(ImageKind::Icon, game.id, "png", b"icon")
                .unwrap();
            lib.db.set_icon_path(game.id, Some(&rel)).unwrap();
            if from_cover {
                lib.db
                    .set_extra_value(
                        game.id,
                        crate::images::ICON_FROM_COVER_KEY,
                        Some(serde_json::Value::Bool(true)),
                    )
                    .unwrap();
            }
        }
        let img = tmp.path().join("i.png");
        std::fs::write(&img, b"png").unwrap();
        let a = lib.set_cover(cover_icon.id, Some(&img)).unwrap();
        assert!(
            a.icon_path.is_none(),
            "icon cut from the old cover is dropped"
        );
        assert!(a.extra.get(crate::images::ICON_FROM_COVER_KEY).is_none());
        let b = lib.set_cover(own_icon.id, Some(&img)).unwrap();
        assert!(b.icon_path.is_some(), "store/executable icons stay");
    }

    #[test]
    fn remove_and_cover_management() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        let exe = tmp.path().join("x.exe");
        std::fs::write(&exe, b"").unwrap();
        let game = lib
            .add_manual_game(&NewManualGame {
                name: Some("X".into()),
                executable: exe.display().to_string(),
                args: None,
                working_dir: None,
                cover_image: None,
            })
            .unwrap();
        assert!(game.cover_path.is_none());
        let img = tmp.path().join("i.png");
        std::fs::write(&img, b"png").unwrap();
        assert!(
            lib.set_cover(game.id, Some(&img))
                .unwrap()
                .cover_path
                .is_some()
        );
        assert!(lib.set_cover(game.id, None).unwrap().cover_path.is_none());
        lib.remove_game(game.id).unwrap();
        assert!(matches!(
            lib.get_game(game.id),
            Err(CoreError::GameNotFound(_))
        ));
    }

    #[test]
    fn seed_inserts_requested_count() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        assert_eq!(lib.seed_fake_library(250).unwrap(), 250);
        assert_eq!(lib.database().count_games().unwrap(), 250);
    }

    #[test]
    fn install_folder_is_the_games_install_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (lib, _) = library(tmp.path(), ProviderRegistry::new());
        let mut known = ImportedGame::installed("1", "Known");
        known.install_dir = Some(r"C:\Games\Known".into());
        let unknown = ImportedGame::installed("2", "Unknown");
        lib.database()
            .upsert_imported("steam", &[known, unknown], Utc::now())
            .unwrap();
        let games = lib.list_games(&GameQuery::default()).unwrap();
        let id = |name: &str| games.iter().find(|g| g.name == name).unwrap().id;
        assert_eq!(
            lib.install_folder(id("Known")).unwrap(),
            std::path::PathBuf::from(r"C:\Games\Known")
        );
        assert!(lib.install_folder(id("Unknown")).is_err());
    }
}
