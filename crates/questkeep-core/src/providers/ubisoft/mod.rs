//! Ubisoft Connect provider, Windows only.
//!
//! Installed games come from `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>`
//! (`InstallDir`; the key name is Ubisoft's numeric product id). Names and artwork come from the
//! launcher's own product cache (`config`), which lists every product the launcher knows, owned
//! or not. The launcher's ownership cache is encrypted and Ubisoft has no sign-in for other
//! applications, so games that are not installed only come from that product cache, filtered the
//! way Playnite does (see [`config::catalogue_games`]), and only when the user turns that on
//! (`importCatalogue` in the provider settings): it may list games the user does not own.
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
/// Provider setting that turns on importing the launcher's product cache.
pub const IMPORT_CATALOGUE: &str = "importCatalogue";

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

/// `UbisoftConnect.exe` below the launcher's install folder, when the file exists.
#[cfg(any(windows, test))]
fn client_in(dir: &str) -> Option<PathBuf> {
    let dir = dir.trim().trim_end_matches(['\\', '/']);
    if dir.is_empty() {
        return None;
    }
    let exe = PathBuf::from(dir).join("UbisoftConnect.exe");
    exe.is_file().then_some(exe)
}

/// The Ubisoft Connect client: `InstallDir` of `HKLM\SOFTWARE\Ubisoft\Launcher`, else the
/// install location of its "Apps" entry.
#[cfg(windows)]
fn system_client() -> Option<PathBuf> {
    crate::platform::registry_value(r"HKEY_LOCAL_MACHINE\SOFTWARE\Ubisoft\Launcher\InstallDir")
        .and_then(|dir| client_in(&dir))
        .or_else(|| {
            crate::platform::uninstall_entries()
                .into_iter()
                .filter(|e| e.display_name.as_deref() == Some("Ubisoft Connect"))
                .find_map(|e| client_in(e.install_location.as_deref()?))
        })
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

    fn icon_svg(&self) -> &'static str {
        crate::providers::icons::UBISOFT
    }

    fn can_scan(&self) -> bool {
        cfg!(windows)
    }

    /// Whether it runs is decided by the setting, see [`scan_owned`](Self::scan_owned).
    fn can_scan_owned(&self) -> bool {
        true
    }

    /// The games of the launcher's product cache, when `config` has `"importCatalogue": true`.
    fn scan_owned(&self, config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        if config
            .get(IMPORT_CATALOGUE)
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            return Err(ProviderError::NotConfigured(
                "Importing the Ubisoft Connect catalogue is turned off.".into(),
            ));
        }
        let products = self.read_products();
        if let Ok(mut cache) = self.products.lock() {
            *cache = Some(products.clone());
        }
        let mut games: Vec<ImportedGame> = config::catalogue_games(&products)
            .into_iter()
            .filter_map(|p| {
                let name = install::clean_name(p.name.as_deref()?);
                let mut game = ImportedGame::installed(p.id.to_string(), name);
                game.installed = false;
                Some(game)
            })
            .collect();
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
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

    /// Starts Ubisoft Connect (Windows; found through the registry).
    fn launcher(&self) -> Option<PlayAction> {
        match &self.installs {
            #[cfg(windows)]
            Installs::System => {
                let exe = system_client()?;
                Some(PlayAction::file(exe.to_string_lossy()))
            }
            Installs::Fixed(_) => None,
        }
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

    #[test]
    fn finds_the_client_in_the_install_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_string_lossy().into_owned();
        assert_eq!(client_in(&dir), None);
        assert_eq!(client_in(""), None);
        std::fs::write(tmp.path().join("UbisoftConnect.exe"), "x").unwrap();
        assert_eq!(
            client_in(&format!("{dir}/")),
            Some(tmp.path().join("UbisoftConnect.exe"))
        );
    }

    #[test]
    fn launcher_is_none_for_fixed_data() {
        assert!(
            UbisoftProvider::with_data(Vec::new(), None)
                .launcher()
                .is_none()
        );
    }

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
    fn catalogue_import_is_opt_in_and_filtered() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("configurations");
        let dlc = "root:\n  name: Trackmania Club Access\n  is_ulc: yes\n  start_game:\n    x: 1\n";
        let steam = "root:\n  name: Sold Elsewhere\n  start_game:\n    x: 1\n  third_party_platform:\n    name: Steam\n";
        std::fs::write(
            &path,
            encode(&[(5595, TRACKMANIA), (26, BROTHERHOOD), (7, dlc), (8, steam)]),
        )
        .unwrap();
        let p = provider(tmp.path(), Some(path));
        assert!(p.can_scan_owned());
        assert!(matches!(
            p.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::NotConfigured(_))
        ));
        let games = p
            .scan_owned(&serde_json::json!({ "importCatalogue": true }))
            .unwrap();
        let names: Vec<_> = games
            .iter()
            .map(|g| (g.name.as_str(), g.installed))
            .collect();
        assert_eq!(
            names,
            [
                ("Assassin's Creed Brotherhood", false),
                ("Trackmania", false)
            ]
        );
    }

    /// Read-only look at this machine:
    /// `cargo test -p questkeep-core ubisoft -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn live_scan() {
        let p = UbisoftProvider::detect();
        println!("{} products in the launcher cache", p.products().len());
        let products = p.products();
        let catalogue = config::catalogue_games(&products);
        println!("{} catalogue games", catalogue.len());
        for product in catalogue {
            println!("  {} | {:?}", product.id, product.name);
        }
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
