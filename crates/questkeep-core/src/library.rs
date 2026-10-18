//! `Library`: the facade the Tauri shell talks to. Wires database, providers, images and events.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::db::{Database, NewGameRecord};
use crate::error::{CoreError, Result};
use crate::events::{CoreEvent, EventBus};
use crate::images::{CoverJob, ImageCache};
use crate::model::{
    Game, GameQuery, GameUpdate, MANUAL_SOURCE, NewManualGame, PlayAction, ScanSummary,
    display_name_from_path,
};
use crate::paths::DataPaths;
use crate::providers::{ProviderInfo, ProviderRegistry};
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
}

impl Library {
    /// Opens the library in `paths` (creating directories and the database as needed).
    pub fn open(paths: DataPaths, providers: ProviderRegistry, events: EventBus) -> Result<Self> {
        paths.ensure_dirs()?;
        let db = Arc::new(Database::open(&paths.database_file())?);
        Ok(Self::with_database(paths, db, providers, events))
    }

    /// Builds a library around an existing database (tests use an in-memory one).
    pub fn with_database(
        paths: DataPaths,
        db: Arc<Database>,
        providers: ProviderRegistry,
        events: EventBus,
    ) -> Self {
        Self {
            images: ImageCache::new(paths.clone()),
            settings: SettingsStore::new(paths.settings_file()),
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
        self.settings.save(settings)
    }

    /// Converts stored relative image paths into absolute ones for the UI; drops missing files.
    fn resolve_images(&self, mut game: Game) -> Game {
        game.cover_path = game
            .cover_path
            .as_deref()
            .and_then(|p| self.images.resolve(p))
            .map(|p| p.display().to_string());
        game.icon_path = game
            .icon_path
            .as_deref()
            .and_then(|p| self.images.resolve(p))
            .map(|p| p.display().to_string());
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
            match self.images.import_cover(game.id, Path::new(&cover)) {
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
            Some(path) => Some(self.images.import_cover(id, path)?),
            None => {
                self.images.remove_covers(id)?;
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
        if let Err(err) = self.images.remove_covers(id) {
            tracing::warn!(%err, %id, "could not delete cached covers");
        }
        self.events.publish(CoreEvent::GameRemoved { id });
        Ok(())
    }

    pub fn provider_infos(&self) -> Vec<ProviderInfo> {
        self.providers.infos()
    }

    /// Runs a provider's installed-games scan and merges the result into the library.
    pub fn scan(&self, provider_id: &str) -> Result<ScanSummary> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or_else(|| CoreError::UnknownProvider(provider_id.to_string()))?;
        let games = provider
            .scan_installed()
            .map_err(|err| CoreError::Provider {
                provider: provider.name().to_string(),
                message: err.to_string(),
            })?;
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

    /// Cover-fetch jobs for store games that don't have a cover yet.
    pub fn missing_cover_jobs(&self) -> Result<Vec<CoverJob>> {
        Ok(self
            .db
            .games_without_cover()?
            .into_iter()
            .filter(|(_, source, _)| self.providers.get(source).is_some())
            .map(|(game_id, source, source_game_id)| CoverJob {
                game_id,
                source,
                source_game_id,
            })
            .collect())
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::model::ImportedGame;
    use crate::providers::{LibraryProvider, ProviderError};

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
        (Library::with_database(paths, db, providers, events), seen)
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
            source_game_id: "1".into(),
            name: "Fake Game".into(),
            install_dir: None,
            play_action: Some(PlayAction::uri("fake://1")),
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
