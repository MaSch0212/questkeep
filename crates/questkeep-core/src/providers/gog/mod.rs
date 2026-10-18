//! GOG provider: finds games installed by GOG Galaxy or the offline installers, and fetches
//! description and artwork from GOG's public catalogue API. No login is involved.
//!
//! Installed games come from the Windows registry (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games`)
//! plus the `goggame-<id>.info` manifest in each install folder. On Linux they come from Heroic
//! Games Launcher (`super::heroic`) with the same product ids; Lutris is not read for GOG.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

mod api;
mod install;
#[cfg(windows)]
mod registry;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub use install::InstallRecord;

use super::heroic::{self, HeroicPaths};
use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::model::{GameMetadata, ImportedGame, PlayAction};

pub const GOG_SOURCE: &str = "gog";

/// Store lookups kept in memory so cover, background and metadata of one game share a request.
const CACHE_LIMIT: usize = 64;

/// Where install records come from.
enum Installs {
    /// The registry of this machine (Windows only).
    #[cfg(windows)]
    System,
    /// Heroic Games Launcher data (Linux).
    Heroic(HeroicPaths),
    /// Fixed data, for tests.
    Fixed {
        records: Vec<InstallRecord>,
        galaxy_client: Option<PathBuf>,
    },
}

pub struct GogProvider {
    installs: Installs,
    /// Launch through GOG Galaxy when it is installed (cloud saves, overlay, playtime). When
    /// `false`, or when Galaxy is missing, the manifest's primary executable is started directly.
    use_galaxy: bool,
    http: ureq::Agent,
    store_cache: Mutex<HashMap<String, Option<api::StoreGame>>>,
}

impl GogProvider {
    /// Reads the installs of this machine.
    pub fn detect() -> Self {
        #[cfg(windows)]
        let installs = Installs::System;
        #[cfg(not(windows))]
        let installs = Installs::Heroic(HeroicPaths::detect());
        Self::new(installs)
    }

    /// Reads Heroic data from the given directories (any platform; tests and tools).
    pub fn with_heroic(paths: HeroicPaths) -> Self {
        Self::new(Installs::Heroic(paths))
    }

    /// Uses fixed install records (tests and tools).
    pub fn with_records(records: Vec<InstallRecord>, galaxy_client: Option<PathBuf>) -> Self {
        Self::new(Installs::Fixed {
            records,
            galaxy_client,
        })
    }

    fn new(installs: Installs) -> Self {
        Self {
            installs,
            use_galaxy: false,
            http: api::http_agent(),
            store_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Chooses between starting the executable directly (default; works for offline-installer
    /// games Galaxy doesn't know) and launching through Galaxy.
    pub fn with_galaxy_launch(mut self, use_galaxy: bool) -> Self {
        self.use_galaxy = use_galaxy;
        self
    }

    fn load_installs(&self) -> Result<(Vec<InstallRecord>, Option<PathBuf>), ProviderError> {
        match &self.installs {
            Installs::Fixed {
                records,
                galaxy_client,
            } => Ok((records.clone(), galaxy_client.clone())),
            #[cfg(windows)]
            Installs::System => system_installs(),
            Installs::Heroic(_) => Ok((Vec::new(), None)),
        }
    }

    /// The catalogue entry for a game; network results (including "unknown game") are cached.
    fn store_game(&self, id: &str) -> Result<Option<api::StoreGame>, ProviderError> {
        if let Ok(cache) = self.store_cache.lock()
            && let Some(hit) = cache.get(id)
        {
            return Ok(hit.clone());
        }
        let game = api::fetch_game(&self.http, id)?;
        if let Ok(mut cache) = self.store_cache.lock() {
            if cache.len() >= CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(id.to_string(), game.clone());
        }
        Ok(game)
    }
}

#[cfg(windows)]
fn system_installs() -> Result<(Vec<InstallRecord>, Option<PathBuf>), ProviderError> {
    let records = registry::install_records()
        .ok_or_else(|| ProviderError::NotFound("GOG installation not found".into()))?;
    Ok((records, registry::galaxy_client()))
}

impl LibraryProvider for GogProvider {
    fn id(&self) -> &str {
        GOG_SOURCE
    }

    fn name(&self) -> &str {
        "GOG"
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if let Installs::Heroic(paths) = &self.installs {
            return heroic::gog_games(paths);
        }
        let (records, galaxy) = self.load_installs()?;
        let mut games = install::games_from_records(&records, galaxy.as_deref(), self.use_galaxy);
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    /// Listing the owned library needs a GOG login (OAuth), which Questkeep does not do, so only
    /// installed games are imported.
    fn can_scan_owned(&self) -> bool {
        false
    }

    /// Opens the game's page in GOG Galaxy, where it can be installed.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        // Heroic has no install URI; Galaxy only exists on Windows.
        if matches!(self.installs, Installs::Heroic(_)) {
            return None;
        }
        api::is_product_id(source_game_id)
            .then(|| PlayAction::uri(format!("goggalaxy://openGameView/{source_game_id}")))
    }

    fn metadata(&self, source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        Ok(self.store_game(source_game_id)?.map(|game| game.metadata()))
    }

    /// Box art from the catalogue API. This does one small request, which is fine because the
    /// image fetcher calls it from its background worker. Galaxy's local web cache is not used:
    /// its file names are hashes with no stable mapping to product ids.
    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        match self.store_game(source_game_id) {
            Ok(Some(game)) => game
                .cover_urls()
                .into_iter()
                .map(ImageCandidate::Url)
                .collect(),
            Ok(None) => Vec::new(),
            Err(err) => {
                tracing::debug!("GOG cover lookup for {source_game_id} failed: {err}");
                Vec::new()
            }
        }
    }

    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        match self.store_game(source_game_id) {
            Ok(Some(game)) => game
                .background_urls()
                .into_iter()
                .map(ImageCandidate::Url)
                .collect(),
            Ok(None) => Vec::new(),
            Err(err) => {
                tracing::debug!("GOG background lookup for {source_game_id} failed: {err}");
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::ActionKind;

    #[test]
    fn provider_metadata() {
        let p = GogProvider::with_records(vec![], None);
        assert_eq!(p.id(), GOG_SOURCE);
        assert_eq!(p.name(), "GOG");
        assert!(p.can_scan());
        assert!(!p.can_scan_owned());
    }

    #[test]
    fn install_action_opens_galaxy_game_view() {
        let p = GogProvider::with_records(vec![], None);
        let action = p.install_action("1207658924").unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(action.target, "goggalaxy://openGameView/1207658924");
        assert!(p.install_action("1/../x").is_none());
    }

    #[test]
    fn scan_sorts_games_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut records = Vec::new();
        for (id, name) in [("2", "zebra"), ("1", "Alpha")] {
            let dir = tmp.path().join(id);
            fs::create_dir_all(&dir).unwrap();
            records.push(InstallRecord {
                id: id.into(),
                game_name: Some(name.into()),
                path: Some(dir.to_string_lossy().into_owned()),
                exe: Some(dir.join("g.exe").to_string_lossy().into_owned()),
                ..Default::default()
            });
        }
        let games = GogProvider::with_records(records, None)
            .scan_installed()
            .unwrap();
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "zebra"]);
    }

    #[test]
    fn heroic_mode_reports_not_found_without_heroic() {
        let tmp = tempfile::tempdir().unwrap();
        let p = GogProvider::with_heroic(HeroicPaths::discover(tmp.path(), None, None));
        assert!(matches!(
            p.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
        assert!(p.install_action("1207658924").is_none());
    }

    #[test]
    fn heroic_mode_scans_gog_installs() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Games/Cyber");
        fs::create_dir_all(&game).unwrap();
        let hero = tmp.path().join(".config/heroic/gog_store");
        fs::create_dir_all(&hero).unwrap();
        fs::write(
            hero.join("installed.json"),
            serde_json::json!({"installed": [
                {"appName": "123", "install_path": game, "platform": "windows"}
            ]})
            .to_string(),
        )
        .unwrap();
        let p = GogProvider::with_heroic(HeroicPaths::discover(tmp.path(), None, None));
        let games = p.scan_installed().unwrap();
        assert_eq!(games[0].source_game_id, "123");
        assert_eq!(games[0].name, "Cyber");
    }
}
