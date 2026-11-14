//! Example Questkeep store plugin: the games are listed in `games.json` next to `plugin.json`.

use std::cell::Cell;

use questkeep_plugin_sdk::model::{GameLink, GameMetadata, ImportedGame, PlayAction};
use questkeep_plugin_sdk::settings::{StoreOption, StoreSettings};
use questkeep_plugin_sdk::{ImageCandidate, Provider, ProviderError, export_provider, log};
use serde::Deserialize;

/// The installed plugin folder is mounted read-only at `/plugin`.
const GAMES_FILE: &str = "/plugin/games.json";
const SHOP_URL: &str = "https://example.com/store";
const ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8h16l-1.5 11h-13z"/><path d="M9 8V6a3 3 0 0 1 6 0v2"/></svg>"#;

/// One entry of `games.json`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    name: String,
    exe: Option<String>,
    install_dir: Option<String>,
    installed: Option<bool>,
    playtime_seconds: Option<u64>,
    #[serde(default)]
    demo: bool,
    description: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    developers: Vec<String>,
    release_date: Option<String>,
    cover: Option<String>,
}

impl Entry {
    fn is_installed(&self) -> bool {
        self.installed.unwrap_or(true)
    }

    fn to_game(&self) -> ImportedGame {
        ImportedGame {
            installed: self.is_installed(),
            install_dir: self.install_dir.clone(),
            play_action: self.exe.as_deref().map(PlayAction::file),
            playtime_seconds: self.playtime_seconds,
            ..ImportedGame::installed(&self.id, &self.name)
        }
    }
}

#[derive(Default)]
struct ExampleStore {
    /// The store option "Include demos" (see `settings` and `configure`).
    include_demos: Cell<bool>,
}

impl ExampleStore {
    /// The games to report: demos only while the option is on.
    fn entries(&self) -> Result<Vec<Entry>, ProviderError> {
        let text = std::fs::read_to_string(GAMES_FILE)
            .map_err(|e| ProviderError::Io(format!("could not read games.json: {e}")))?;
        let entries: Vec<Entry> = serde_json::from_str(&text)
            .map_err(|e| ProviderError::Parse(format!("games.json is not valid: {e}")))?;
        let demos = self.include_demos.get();
        Ok(entries.into_iter().filter(|e| demos || !e.demo).collect())
    }

    fn entry(&self, id: &str) -> Option<Entry> {
        self.entries().ok()?.into_iter().find(|e| e.id == id)
    }
}

impl Provider for ExampleStore {
    fn id(&self) -> String {
        "example-store".into()
    }

    fn name(&self) -> String {
        "Example Store".into()
    }

    fn icon_svg(&self) -> String {
        ICON.into()
    }

    /// There is no store client to look for: the games come from a file.
    fn is_installed(&self) -> bool {
        true
    }

    fn shop_url(&self) -> Option<String> {
        Some(SHOP_URL.into())
    }

    fn settings(&self) -> StoreSettings {
        StoreSettings {
            options: vec![StoreOption {
                key: "includeDemos".into(),
                label: "Include demos".into(),
                hint: "Also add the games marked as demo in games.json.".into(),
            }],
            credentials: None,
        }
    }

    fn configure(&self, config: &serde_json::Value) {
        self.include_demos.set(config["includeDemos"] == true);
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        let games: Vec<ImportedGame> = self
            .entries()?
            .iter()
            .filter(|e| e.is_installed())
            .map(Entry::to_game)
            .collect();
        log::info(format!("found {} installed games", games.len()));
        Ok(games)
    }

    fn can_scan_owned(&self) -> bool {
        true
    }

    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        Ok(self.entries()?.iter().map(Entry::to_game).collect())
    }

    fn metadata(&self, source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        let Some(entry) = self.entries()?.into_iter().find(|e| e.id == source_game_id) else {
            return Ok(None);
        };
        Ok(Some(GameMetadata {
            description: entry.description,
            release_date: entry.release_date,
            developers: entry.developers,
            genres: entry.genres,
            links: vec![GameLink::new(
                "Example Store",
                format!("{SHOP_URL}/{source_game_id}"),
            )],
            ..GameMetadata::default()
        }))
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.entry(source_game_id)
            .and_then(|e| e.cover)
            .map(ImageCandidate::Url)
            .into_iter()
            .collect()
    }
}

export_provider!(ExampleStore);
