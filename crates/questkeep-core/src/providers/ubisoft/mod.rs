//! Ubisoft Connect provider, Windows only.
//!
//! Installed games come from `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>`
//! (`InstallDir`; the key name is Ubisoft's numeric product id). Names and artwork come from the
//! launcher's own product cache (`config`), which lists every product the launcher knows, owned
//! or not, so it is never used as a list of owned games. The launcher's ownership cache is
//! encrypted, so owned-but-not-installed games would need an online account sign-in, which this
//! provider does not have: only installed games are imported.
//!
//! Games start and uninstall through the `uplay://` protocol of Ubisoft Connect.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

mod config;
mod install;
#[cfg(windows)]
mod registry;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub use install::InstallRecord;

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::model::{ImportedGame, PlayAction};

pub const UBISOFT_SOURCE: &str = "ubisoft";

type Products = Arc<HashMap<u32, config::Product>>;

/// Where install records come from.
enum Installs {
    /// The registry of this machine (Windows only).
    #[cfg(windows)]
    System,
    /// Fixed data, for tests and non-Windows platforms.
    Fixed(Vec<InstallRecord>),
}

pub struct UbisoftProvider {
    installs: Installs,
    /// The launcher's `configurations` file, when known.
    configurations: Option<PathBuf>,
    /// Parsed `configurations`, loaded on first use and again with every scan.
    products: Mutex<Option<Products>>,
}

/// `%LOCALAPPDATA%\Ubisoft Game Launcher\cache\configuration\configurations`.
fn default_configurations() -> Option<PathBuf> {
    Some(
        dirs::data_local_dir()?
            .join("Ubisoft Game Launcher")
            .join("cache")
            .join("configuration")
            .join("configurations"),
    )
}

impl UbisoftProvider {
    /// Reads the installs of this machine.
    pub fn detect() -> Self {
        #[cfg(windows)]
        let installs = Installs::System;
        #[cfg(not(windows))]
        let installs = Installs::Fixed(Vec::new());
        Self::new(installs, default_configurations())
    }

    /// Uses fixed install records and an explicit `configurations` file (tests and tools).
    pub fn with_data(records: Vec<InstallRecord>, configurations: Option<PathBuf>) -> Self {
        Self::new(Installs::Fixed(records), configurations)
    }

    fn new(installs: Installs, configurations: Option<PathBuf>) -> Self {
        Self {
            installs,
            configurations,
            products: Mutex::new(None),
        }
    }

    fn load_installs(&self) -> Result<Vec<InstallRecord>, ProviderError> {
        match &self.installs {
            Installs::Fixed(records) => Ok(records.clone()),
            #[cfg(windows)]
            Installs::System => registry::install_records().ok_or_else(|| {
                ProviderError::NotFound("Ubisoft Connect installation not found".into())
            }),
        }
    }

    /// Reads and parses the product cache. A missing or unreadable file only costs names and
    /// artwork, so it is logged and treated as empty.
    fn read_products(&self) -> Products {
        let Some(path) = &self.configurations else {
            return Products::default();
        };
        let parsed = std::fs::read(path)
            .map_err(|err| err.to_string())
            .and_then(|data| config::parse_configurations(&data));
        match parsed {
            Ok(products) => Arc::new(products),
            Err(err) => {
                tracing::debug!(%err, path = %path.display(), "no Ubisoft Connect product cache");
                Products::default()
            }
        }
    }

    /// The cached products, loading them on first use.
    fn products(&self) -> Products {
        let Ok(mut cache) = self.products.lock() else {
            return self.read_products();
        };
        cache.get_or_insert_with(|| self.read_products()).clone()
    }

    fn product(&self, source_game_id: &str) -> Option<config::Product> {
        let id = source_game_id.parse::<u32>().ok()?;
        self.products().get(&id).cloned()
    }
}

impl LibraryProvider for UbisoftProvider {
    fn id(&self) -> &str {
        UBISOFT_SOURCE
    }

    fn name(&self) -> &str {
        "Ubisoft Connect"
    }

    fn can_scan(&self) -> bool {
        cfg!(windows)
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        let records = self.load_installs()?;
        // A scan is the moment to pick up what the launcher cached since the last one.
        let products = self.read_products();
        if let Ok(mut cache) = self.products.lock() {
            *cache = Some(products.clone());
        }
        let mut games = install::games_from_records(&records, &products);
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    /// Hands the install to Ubisoft Connect, which asks where to put the game.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        (cfg!(windows) && install::is_product_id(source_game_id))
            .then(|| PlayAction::uri(install::install_uri(source_game_id)))
    }

    fn can_uninstall(&self) -> bool {
        cfg!(windows)
    }

    /// Hands the uninstall to Ubisoft Connect, which asks for confirmation (the same command as
    /// the launcher's own Windows uninstall entry). Only for games the launcher has installed.
    fn uninstall_action(&self, source_game_id: &str) -> Option<PlayAction> {
        if !self.can_uninstall() || !install::is_product_id(source_game_id) {
            return None;
        }
        let records = self.load_installs().ok()?;
        records
            .iter()
            .any(|r| r.id.trim() == source_game_id)
            .then(|| PlayAction::uri(install::uninstall_uri(source_game_id)))
    }

    /// The launcher's thumbnail is about 6:7, not a 2:3 box art.
    fn covers_are_low_quality(&self, _source_game_id: &str) -> bool {
        true
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        url_candidate(self.product(source_game_id).and_then(|p| p.thumb_image))
    }

    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        url_candidate(
            self.product(source_game_id)
                .and_then(|p| p.background_image),
        )
    }

    fn icon_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        url_candidate(self.product(source_game_id).and_then(|p| p.icon_image))
    }
}

fn url_candidate(url: Option<String>) -> Vec<ImageCandidate> {
    url.map(ImageCandidate::Url).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::config::tests::{BROTHERHOOD, TRACKMANIA, encode};
    use super::*;

    fn provider(dir: &Path, configurations: Option<PathBuf>) -> UbisoftProvider {
        UbisoftProvider::with_data(
            vec![InstallRecord {
                id: "5595".into(),
                install_dir: Some(dir.to_string_lossy().into_owned()),
                display_name: Some("Trackmania".into()),
            }],
            configurations,
        )
    }

    fn write_configurations(dir: &Path) -> PathBuf {
        let path = dir.join("configurations");
        std::fs::write(&path, encode(&[(5595, TRACKMANIA), (26, BROTHERHOOD)])).unwrap();
        path
    }

    #[test]
    fn scans_installed_games_with_names_from_the_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let games_dir = tmp.path().join("Trackmania Folder");
        std::fs::create_dir(&games_dir).unwrap();
        let p = provider(&games_dir, Some(write_configurations(tmp.path())));
        let games = p.scan_installed().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Trackmania");
        assert_eq!(
            games[0].play_action.as_ref().unwrap().target,
            "uplay://launch/5595/0"
        );
    }

    #[test]
    fn missing_cache_still_lists_games_by_registry_name() {
        let tmp = tempfile::tempdir().unwrap();
        let p = provider(tmp.path(), Some(tmp.path().join("nope")));
        assert_eq!(p.scan_installed().unwrap()[0].name, "Trackmania");
        assert!(p.cover_candidates("5595").is_empty());
    }

    #[test]
    fn corrupt_cache_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("configurations");
        std::fs::write(&path, [0x0a, 0xff, 0xff]).unwrap();
        let p = provider(tmp.path(), Some(path));
        assert_eq!(p.scan_installed().unwrap().len(), 1);
    }

    #[test]
    fn artwork_candidates_come_from_the_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let p = provider(tmp.path(), Some(write_configurations(tmp.path())));
        assert!(p.covers_are_low_quality("5595"));
        let urls = |c: Vec<ImageCandidate>| -> Vec<String> {
            c.into_iter()
                .map(|c| match c {
                    ImageCandidate::Url(u) => u,
                    ImageCandidate::File(f) => f.to_string_lossy().into_owned(),
                })
                .collect()
        };
        assert_eq!(
            urls(p.cover_candidates("5595")),
            [format!(
                "{}29af1426c8ea0ed58abcc96195c45ef7.jpg",
                config::ASSET_URL_BASE
            )]
        );
        assert_eq!(urls(p.background_candidates("5595")).len(), 1);
        assert_eq!(urls(p.icon_candidates("5595")).len(), 1);
        assert!(p.cover_candidates("999").is_empty());
        assert!(p.cover_candidates("not-a-number").is_empty());
    }

    #[test]
    fn install_and_uninstall_actions_use_the_uplay_protocol() {
        let tmp = tempfile::tempdir().unwrap();
        let p = provider(tmp.path(), None);
        if cfg!(windows) {
            assert_eq!(
                p.install_action("5595").unwrap().target,
                "uplay://install/5595"
            );
            assert_eq!(
                p.uninstall_action("5595").unwrap().target,
                "uplay://uninstall/5595"
            );
            assert!(p.uninstall_action("26").is_none(), "not installed");
            assert!(p.install_action("../x").is_none());
        } else {
            assert!(p.install_action("5595").is_none());
            assert!(p.uninstall_action("5595").is_none());
        }
    }

    #[test]
    fn owned_scan_is_not_supported() {
        let p = UbisoftProvider::with_data(Vec::new(), None);
        assert!(!p.can_scan_owned());
        assert!(matches!(
            p.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::Unsupported)
        ));
    }

    /// Read-only look at this machine:
    /// `cargo test -p questkeep-core ubisoft -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn live_scan() {
        let p = UbisoftProvider::detect();
        println!("{} products in the launcher cache", p.products().len());
        match p.scan_installed() {
            Ok(games) => {
                println!("{} Ubisoft games", games.len());
                for g in games {
                    println!(
                        "{} | {} | {:?} | {:?}",
                        g.name,
                        g.source_game_id,
                        g.install_dir,
                        g.play_action.map(|a| a.target)
                    );
                    println!("  cover {:?}", p.cover_candidates(&g.source_game_id));
                    println!(
                        "  background {:?}",
                        p.background_candidates(&g.source_game_id)
                    );
                    println!("  icon {:?}", p.icon_candidates(&g.source_game_id));
                    println!("  uninstall {:?}", p.uninstall_action(&g.source_game_id));
                }
            }
            Err(err) => println!("scan failed: {err}"),
        }
    }
}
