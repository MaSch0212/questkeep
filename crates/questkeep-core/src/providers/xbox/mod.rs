//! Xbox / Microsoft Store provider (PC Game Pass and Store games), Windows only.
//!
//! Installed games are found from the current user's registered AppX/MSIX packages (registry, no
//! PowerShell and no Xbox app involved). A package is a game when its folder contains
//! `MicrosoftGame.config` (GDK titles installed to `XboxGames\<Game>\Content`). Legacy UWP games
//! below `C:\Program Files\WindowsApps` are not readable without elevation and are skipped.
//!
//! Real portrait covers, backgrounds and metadata come from the public Microsoft Store display
//! catalog (catalog), found through the package family name. Signing in to a Microsoft/Xbox
//! account (ccount, optional) adds the owned PC titles and their playtime.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

mod account;
mod catalog;
mod manifest;
mod scan;

use std::sync::Arc;

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::auth::{AccountAuth, Http, UreqHttp};
use crate::model::{GameMetadata, ImportedGame, PlayAction};
use crate::platform::{self, AppxPackage};

pub use account::{LOGIN_REDIRECT_PREFIX, XboxAccount};

pub const XBOX_SOURCE: &str = "xbox";

/// Where the provider gets packages from; replaceable in tests.
type PackageSource = fn() -> Vec<AppxPackage>;

#[derive(Debug, Clone)]
pub struct XboxProvider {
    packages: PackageSource,
    catalog: Arc<catalog::Catalog>,
    /// Signed-in account, when the app supplies token storage.
    account: Option<Arc<XboxAccount>>,
}

impl XboxProvider {
    /// Reads the packages registered on this machine and uses the public Store catalog.
    pub fn detect() -> Self {
        Self::new(platform::appx_packages, Arc::new(UreqHttp::new()))
    }

    /// Enables account sign-in and the owned-games list.
    pub fn with_account(mut self, account: Arc<XboxAccount>) -> Self {
        self.account = Some(account);
        self
    }

    /// `http` is used for the Store catalog lookups.
    fn new(packages: PackageSource, http: Arc<dyn Http>) -> Self {
        Self {
            packages,
            catalog: Arc::new(catalog::Catalog::new(http)),
            account: None,
        }
    }

    fn entries(&self) -> Vec<scan::GameEntry> {
        scan::scan_packages(&(self.packages)())
    }
}

impl LibraryProvider for XboxProvider {
    fn id(&self) -> &str {
        XBOX_SOURCE
    }

    fn name(&self) -> &str {
        "Xbox"
    }

    fn can_scan(&self) -> bool {
        cfg!(windows)
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if !cfg!(windows) {
            return Err(ProviderError::NotFound(
                "Xbox games are only available on Windows.".into(),
            ));
        }
        Ok(self.entries().into_iter().map(|e| e.game).collect())
    }

    fn can_scan_owned(&self) -> bool {
        self.account.is_some()
    }

    /// The PC titles of the signed-in Xbox account as not-installed games. Ids are the package
    /// family names; where one is already installed (same name, any letter case) the installed
    /// game's id is used so both entries merge.
    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        let Some(account) = &self.account else {
            return Err(ProviderError::Unsupported);
        };
        let mut games = account.fetch_owned()?;
        let installed = self.entries();
        for game in &mut games {
            if let Some(entry) = installed.iter().find(|e| {
                e.game
                    .source_game_id
                    .eq_ignore_ascii_case(&game.source_game_id)
            }) {
                game.source_game_id = entry.game.source_game_id.clone();
            }
        }
        Ok(games)
    }

    fn account(&self) -> Option<&dyn AccountAuth> {
        self.account.as_deref().map(|a| a as &dyn AccountAuth)
    }

    /// Description, developer, publisher, release date and genre from the Store catalog.
    fn metadata(&self, source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        Ok(self
            .catalog
            .product(source_game_id)?
            .map(|product| product.metadata.clone()))
    }

    /// Opens the game's Microsoft Store page, from where it can be installed; harmless for games
    /// that are already installed.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        let valid = !source_game_id.is_empty()
            && source_game_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        valid.then(|| PlayAction::uri(format!("ms-windows-store://pdp/?PFN={source_game_id}")))
    }

    fn can_uninstall(&self) -> bool {
        cfg!(windows)
    }

    /// Opens Windows' "Installed apps" settings page; the user picks the game there and Windows
    /// asks for confirmation. Nothing is uninstalled silently.
    fn uninstall_action(&self, source_game_id: &str) -> Option<PlayAction> {
        let valid = !source_game_id.is_empty()
            && source_game_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        (valid && self.can_uninstall()).then(|| PlayAction::uri("ms-settings:appsfeatures"))
    }

    /// `false`: the Store catalog's posters are real 2:3 art and win over SteamGridDB. The trait
    /// cannot say this per game, so for a game without catalog poster the square package logo
    /// (the last candidate) is used before SteamGridDB is asked.
    fn covers_are_low_quality(&self) -> bool {
        false
    }

    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.catalog.backgrounds(source_game_id)
    }

    /// The Store catalog's portrait poster, then (installed games only) the package's logo tile.
    /// Logos are square (150x150 up to 480x480), so they are only a fallback.
    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        let mut out = self.catalog.covers(source_game_id);
        let Some(entry) = self
            .entries()
            .into_iter()
            .find(|e| e.game.source_game_id == source_game_id)
        else {
            return out;
        };
        for declared in &entry.logos {
            if let Some(file) = scan::resolve_logo(declared) {
                let candidate = ImageCandidate::File(file);
                if !out.contains(&candidate) {
                    out.push(candidate);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::auth::testing::{FakeHttp, response};
    use crate::auth::{HttpRequest, MemoryTokenStore};

    const CATALOG: &str = include_str!("../../../tests/fixtures/xbox/catalog_product.json");

    /// A provider whose Store catalog is unreachable.
    fn provider(packages: PackageSource) -> XboxProvider {
        XboxProvider::new(packages, Arc::new(FakeHttp::new(|_| Err("offline".into()))))
    }

    fn provider_with_catalog(packages: PackageSource) -> XboxProvider {
        let handler = |req: &HttpRequest| {
            if req.url.contains("value=Contoso.Racer_8wekyb3d8bbwe") {
                response(200, CATALOG)
            } else {
                response(200, r#"{"Products":[]}"#)
            }
        };
        XboxProvider::new(packages, Arc::new(FakeHttp::new(handler)))
    }

    fn fixture_packages() -> Vec<AppxPackage> {
        // Leaked temp dir path: fn pointers cannot capture, so tests use a fixed layout below.
        let root = std::env::temp_dir().join("questkeep-xbox-provider-test");
        let content = root.join("Content");
        let _ = fs::create_dir_all(content.join("Assets"));
        let _ = fs::write(
            content.join("MicrosoftGame.config"),
            r#"<Game><ShellVisuals DefaultDisplayName="Racer" Square150x150Logo="Assets\Logo.png"/></Game>"#,
        );
        let _ = fs::write(content.join("Assets").join("Logo.scale-100.png"), "x");
        let _ = fs::write(content.join("Assets").join("Logo.scale-200.png"), "x");
        vec![AppxPackage {
            full_name: "Contoso.Racer_1.0.0.0_x64__8wekyb3d8bbwe".into(),
            root: content,
            app_ids: vec!["Game".into()],
        }]
    }

    #[test]
    fn describes_itself() {
        let p = provider(Vec::new);
        assert_eq!(p.id(), "xbox");
        assert_eq!(p.name(), "Xbox");
        assert!(!p.can_scan_owned());
        assert_eq!(p.can_scan(), cfg!(windows));
    }

    #[test]
    fn install_action_opens_store_page() {
        let p = provider(Vec::new);
        let a = p.install_action("Contoso.Racer_8wekyb3d8bbwe").unwrap();
        assert_eq!(
            a.target,
            "ms-windows-store://pdp/?PFN=Contoso.Racer_8wekyb3d8bbwe"
        );
        assert!(p.install_action("").is_none());
        assert!(p.install_action("a&b=c").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn uninstall_action_opens_installed_apps_settings() {
        let p = provider(Vec::new);
        assert!(p.can_uninstall());
        let a = p.uninstall_action("Contoso.Racer_8wekyb3d8bbwe").unwrap();
        assert_eq!(a.target, "ms-settings:appsfeatures");
        assert!(p.uninstall_action("a&b=c").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn scans_and_finds_covers() {
        let p = provider(fixture_packages);
        let games = p.scan_installed().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Racer");
        let covers = p.cover_candidates("Contoso.Racer_8wekyb3d8bbwe");
        assert_eq!(covers.len(), 1);
        match &covers[0] {
            ImageCandidate::File(f) => assert!(f.ends_with("Logo.scale-200.png")),
            other => panic!("unexpected {other:?}"),
        }
        assert!(p.cover_candidates("Nope_abc").is_empty());
    }

    #[cfg(not(windows))]
    #[test]
    fn scan_is_unavailable_off_windows() {
        let p = provider(fixture_packages);
        assert!(matches!(
            p.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
    }

    #[test]
    fn store_catalog_supplies_covers_backgrounds_and_metadata() {
        let p = provider_with_catalog(Vec::new);
        let id = "Contoso.Racer_8wekyb3d8bbwe";
        let covers = p.cover_candidates(id);
        assert_eq!(
            covers.len(),
            1,
            "poster only, and no package without a scan"
        );
        assert!(matches!(&covers[0], ImageCandidate::Url(u) if u.contains("apps.3823.")));
        assert!(!p.covers_are_low_quality());
        let backgrounds = p.background_candidates(id);
        assert!(matches!(&backgrounds[0], ImageCandidate::Url(u) if u.contains("apps.47030.")));
        let meta = p.metadata(id).unwrap().unwrap();
        assert_eq!(meta.developers, ["343 Industries"]);
        assert_eq!(meta.release_date.as_deref(), Some("2021-11-15"));
        // Not in the catalog: no data, not an error.
        assert_eq!(p.metadata("Other.Game_abc").unwrap(), None);
        assert!(p.cover_candidates("Other.Game_abc").is_empty());
        // Catalog unreachable: metadata is an error (retried later), images are just absent.
        let offline = provider(Vec::new);
        assert!(matches!(
            offline.metadata(id),
            Err(ProviderError::Network(_))
        ));
        assert!(offline.cover_candidates(id).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn package_logo_is_the_fallback_after_the_poster() {
        let p = provider_with_catalog(fixture_packages);
        let covers = p.cover_candidates("Contoso.Racer_8wekyb3d8bbwe");
        assert_eq!(covers.len(), 2);
        assert!(matches!(covers[0], ImageCandidate::Url(_)));
        assert!(matches!(covers[1], ImageCandidate::File(_)));
    }

    #[test]
    fn builtin_registration_supports_account_sign_in() {
        let registry =
            crate::providers::ProviderRegistry::with_builtin(Arc::new(MemoryTokenStore::new()));
        let xbox = registry.get("xbox").unwrap();
        assert!(xbox.can_scan_owned());
        let status = xbox.account().unwrap().status();
        assert_eq!(status.provider, "xbox");
        assert!(!status.signed_in && status.supports_code_sign_in && status.code_url.is_some());
        // Signed out: the owned scan is silently skipped by Library (NotConfigured).
        assert!(matches!(
            xbox.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::NotConfigured(_))
        ));
        // Without an account the provider reports no owned scan.
        assert!(!provider(Vec::new).can_scan_owned());
        assert!(provider(Vec::new).account().is_none());
    }
    /// Read-only look at this machine: `cargo test -p questkeep-core xbox -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn live_scan() {
        let packages = platform::appx_packages();
        println!("{} registered packages", packages.len());
        let p = XboxProvider::detect();
        match p.scan_installed() {
            Ok(games) => {
                println!("{} Xbox games", games.len());
                for g in games {
                    println!(
                        "{} | {} | {:?} | {:?}",
                        g.name,
                        g.source_game_id,
                        g.install_dir,
                        g.play_action.map(|a| a.target)
                    );
                }
            }
            Err(e) => println!("error: {e}"),
        }
    }
}
