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
use crate::images::{CoverJob, ImageCache, ImageKind, MetadataJob, SteamGridDb};
use crate::model::{
    ActionKind, Game, GameQuery, GameUpdate, ImportedGame, MANUAL_SOURCE, NewManualGame,
    PlayAction, ScanSummary, display_name_from_path,
};
use crate::paths::DataPaths;
use crate::providers::{ProviderError, ProviderInfo, ProviderRegistry};
use crate::settings::{Settings, SettingsStore};

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
        Self {
            images: ImageCache::new(paths.clone()),
            artwork: SteamGridDb::new(settings.load().steamgriddb_api_key),
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

    pub fn settings(&self) -> Settings {
        self.settings.load()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.settings.save(settings)?;
        self.artwork
            .set_api_key(settings.steamgriddb_api_key.clone());
        Ok(())
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

    pub fn get_game(&self, id: Uuid) -> Result<Game> {
        Ok(self.resolve_images(self.db.get_game(id)?))
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
        self.providers.infos()
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
    /// scanned.
    pub fn scan(&self, provider_id: &str) -> Result<ScanSummary> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or_else(|| CoreError::UnknownProvider(provider_id.to_string()))?;
        let owned = if provider.can_scan_owned() {
            match provider.scan_owned(self.settings().provider_config(provider_id)) {
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
        let summary = self.db.upsert_imported(provider.id(), &games, Utc::now())?;
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

    /// Scans every provider that supports scanning; failures are logged, not fatal.
    pub fn scan_all(&self) -> Vec<(String, Result<ScanSummary>)> {
        self.providers
            .infos()
            .into_iter()
            .filter(|p| p.can_scan)
            .map(|p| {
                let result = self.scan(&p.id);
                if let Err(err) = &result {
                    tracing::warn!(provider = %p.id, %err, "scan failed");
                }
                (p.id, result)
            })
            .collect()
    }

    /// Cover-fetch jobs for games that don't have a cover yet. Store games always get a job (their
    /// provider is asked first). Manual games and games of unregistered providers can only be
    /// served by SteamGridDB, so they get a job only when an API key is configured.
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
                has_key || (source_game_id.is_some() && self.providers.get(source).is_some())
            })
            .map(|(game_id, source, source_game_id)| CoverJob {
                game_id,
                source,
                source_game_id,
                force: false,
            })
            .collect())
    }

    /// Prepares fetching a game's cover and background from SteamGridDB again, replacing the
    /// current ones: clears the "nothing found" markers and returns the job for the cover
    /// fetcher (which also handles the background). Fails when no API key is configured.
    pub fn refetch_artwork(&self, id: Uuid) -> Result<CoverJob> {
        let game = self.db.get_game(id)?;
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

    /// Metadata jobs for store games whose metadata was never fetched.
    pub fn missing_metadata_jobs(&self) -> Result<Vec<MetadataJob>> {
        Ok(self
            .db
            .games_needing_metadata()?
            .into_iter()
            .filter(|(_, source, _)| self.providers.get(source).is_some())
            .map(|(game_id, source, source_game_id)| MetadataJob {
                game_id,
                source,
                source_game_id,
            })
            .collect())
    }

    /// Metadata job for one game (to refresh it on demand); `None` for games without a store.
    pub fn metadata_job(&self, id: Uuid) -> Result<Option<MetadataJob>> {
        let game = self.db.get_game(id)?;
        Ok(match game.source_game_id {
            Some(source_game_id) if self.providers.get(&game.source).is_some() => {
                Some(MetadataJob {
                    game_id: id,
                    source: game.source,
                    source_game_id,
                })
            }
            _ => None,
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
    pub fn install_game(&self, id: Uuid) -> Result<()> {
        let action = self.install_action(id)?;
        let started = match action.kind {
            ActionKind::Uri => crate::platform::open_uri(&action.target),
            ActionKind::File => std::process::Command::new(&action.target)
                .spawn()
                .map(|_| ()),
        };
        started.map_err(|err| CoreError::Launch(format!("Could not start the install: {err}")))
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
    use std::sync::Mutex;

    use super::*;
    use crate::providers::LibraryProvider;

    struct FakeStore(Vec<ImportedGame>);
    impl LibraryProvider for FakeStore {
        fn id(&self) -> &str {
            "fake"
        }
        fn name(&self) -> &str {
            "Fake"
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
        assert!(matches!(
            lib.scan("nope"),
            Err(CoreError::UnknownProvider(_))
        ));
    }

    /// Store whose client is missing but whose account lists owned games when configured.
    struct AccountStore;
    impl LibraryProvider for AccountStore {
        fn id(&self) -> &str {
            "acct"
        }
        fn name(&self) -> &str {
            "Account"
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
        let jobs = lib.missing_metadata_jobs().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].game_id, installed.id, "installed games first");
        assert!(lib.metadata_job(owned.id).unwrap().is_some());
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
        fn id(&self) -> &str {
            "acct"
        }
        fn name(&self) -> &str {
            "Account"
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
}
