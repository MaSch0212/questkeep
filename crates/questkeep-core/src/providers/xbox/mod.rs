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
mod game_pass;
mod manifest;
mod scan;

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::auth::{AccountAuth, Http, UreqHttp};
use crate::model::{GameMetadata, ImportedGame, PlayAction};
use crate::platform::{self, AppxPackage};

pub use account::{LOGIN_REDIRECT_PREFIX, XboxAccount};

pub const XBOX_SOURCE: &str = "xbox";

/// Where the provider gets packages from; replaceable in tests.
type PackageSource = fn() -> Vec<AppxPackage>;

/// Setting key (under `providers.xbox`) for leaving PC Game Pass titles out of the owned list.
pub const HIDE_GAME_PASS: &str = "hideGamePass";

#[derive(Debug)]
pub struct XboxProvider {
    packages: PackageSource,
    catalog: Arc<catalog::Catalog>,
    /// Signed-in account, when the app supplies token storage.
    account: Option<Arc<XboxAccount>>,
    /// Set from the `hideGamePass` setting before each scan ([`LibraryProvider::configure`]).
    hide_game_pass: AtomicBool,
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

    /// `http` is used for the Store and Game Pass catalog lookups.
    fn new(packages: PackageSource, http: Arc<dyn Http>) -> Self {
        Self {
            packages,
            catalog: Arc::new(catalog::Catalog::new(http)),
            account: None,
            hide_game_pass: AtomicBool::new(false),
        }
    }

    fn entries(&self) -> Vec<scan::GameEntry> {
        scan::scan_packages(&(self.packages)())
    }

    /// Drops the not-installed games that are currently in PC Game Pass. The title history
    /// cannot tell a purchase from a Game Pass play, so a bought game that is also in Game Pass
    /// is left out too while it is not installed. Each game costs one (cached) Store catalog
    /// lookup; when the Game Pass list or a lookup fails, games are kept.
    fn without_game_pass(
        &self,
        games: Vec<ImportedGame>,
        installed: &HashSet<String>,
    ) -> Vec<ImportedGame> {
        let game_pass = match self.catalog.game_pass_pc_games() {
            Ok(ids) => ids,
            Err(err) => {
                tracing::warn!(%err, "could not read the Game Pass catalog; keeping all titles");
                return games;
            }
        };
        games
            .into_iter()
            .filter(|game| {
                if installed.contains(&game.source_game_id) {
                    return true;
                }
                let store_id = match self.catalog.product(&game.source_game_id) {
                    Ok(product) => product.and_then(|p| p.store_id.clone()),
                    Err(err) => {
                        tracing::debug!(game = %game.source_game_id, %err, "catalog lookup failed");
                        None
                    }
                };
                !store_id.is_some_and(|id| game_pass.contains(&id.to_ascii_uppercase()))
            })
            .collect()
    }
}

impl LibraryProvider for XboxProvider {
    fn id(&self) -> &str {
        XBOX_SOURCE
    }

    fn name(&self) -> &str {
        "Xbox"
    }

    fn icon_svg(&self) -> &'static str {
        crate::providers::icons::XBOX
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
        let mut installed_ids = HashSet::new();
        for game in &mut games {
            if let Some(entry) = installed.iter().find(|e| {
                e.game
                    .source_game_id
                    .eq_ignore_ascii_case(&game.source_game_id)
            }) {
                game.source_game_id = entry.game.source_game_id.clone();
                installed_ids.insert(game.source_game_id.clone());
            }
        }
        if self.hide_game_pass.load(Ordering::Relaxed) {
            games = self.without_game_pass(games, &installed_ids);
        }
        Ok(games)
    }

    /// `{ "hideGamePass": true }` leaves PC Game Pass titles out of the owned list.
    fn configure(&self, config: &serde_json::Value) {
        let hide = config
            .get(HIDE_GAME_PASS)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        self.hide_game_pass.store(hide, Ordering::Relaxed);
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

    /// The Store catalog's posters are real 2:3 art and win over SteamGridDB; a game without one
    /// only has its square package logo, so SteamGridDB goes first. The catalog answer is cached,
    /// so this costs no extra request.
    fn covers_are_low_quality(&self, source_game_id: &str) -> bool {
        self.catalog.covers(source_game_id).is_empty()
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
        assert!(!p.covers_are_low_quality(id));
        assert!(
            p.covers_are_low_quality("Contoso.Unknown_8wekyb3d8bbwe"),
            "no poster: logo only"
        );
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
    fn game_pass_titles_are_left_out_unless_installed() {
        let handler = |req: &HttpRequest| {
            if req.url.starts_with("https://catalog.gamepass.com/") {
                response(200, r#"[{"siglId":"x"},{"id":"9PP5G1F0C2B6"}]"#)
            } else if req.url.contains("value=Contoso.Racer_8wekyb3d8bbwe") {
                response(200, CATALOG)
            } else {
                response(200, r#"{"Products":[]}"#)
            }
        };
        let p = XboxProvider::new(Vec::new, Arc::new(FakeHttp::new(handler)));
        let owned = |id: &str| ImportedGame {
            installed: false,
            ..ImportedGame::installed(id, id)
        };
        let games = vec![
            owned("Contoso.Racer_8wekyb3d8bbwe"), // in Game Pass (fixture product 9PP5G1F0C2B6)
            owned("Contoso.Bought_8wekyb3d8bbwe"), // not in the catalog
        ];
        let kept = p.without_game_pass(games.clone(), &HashSet::new());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].source_game_id, "Contoso.Bought_8wekyb3d8bbwe");
        let installed = HashSet::from(["Contoso.Racer_8wekyb3d8bbwe".to_string()]);
        assert_eq!(p.without_game_pass(games.clone(), &installed).len(), 2);

        // The Game Pass list is unreachable: nothing is hidden.
        let offline = provider(Vec::new);
        assert_eq!(offline.without_game_pass(games, &HashSet::new()).len(), 2);

        p.configure(&serde_json::json!({ "hideGamePass": true }));
        assert!(p.hide_game_pass.load(Ordering::Relaxed));
        p.configure(&serde_json::Value::Null);
        assert!(!p.hide_game_pass.load(Ordering::Relaxed));
    }

    #[test]
    fn builtin_registration_supports_account_sign_in() {
        let registry = crate::providers::ProviderRegistry::with_builtin(
            Arc::new(MemoryTokenStore::new()),
            None,
            None,
        );
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
