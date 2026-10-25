//! Amazon Games. On Linux, installed games and the owned library come from the Heroic Games
//! Launcher's Nile runner data, see [`super::heroic`]; Heroic is signed in to Amazon itself, so no
//! account is needed here. On Windows installed games come from the Amazon Games app's own
//! databases, see [`app`], and owned games from an Amazon sign-in, see [`account`] (the app's
//! own entitlement cache is encrypted).
//!
//! The game id is the Amazon product id (`product.id` in Nile's library, Heroic's `app_name`, and
//! `Id` in the Amazon Games app's `DbSet` table: all the same UUID), so a library moved between
//! systems keeps its games.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

mod account;
mod app;

use std::sync::Arc;

use super::heroic::{self, HeroicPaths};
use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::auth::AccountAuth;
use crate::model::{ImportedGame, PlayAction};

pub use account::{AmazonAccount, CODE_PARAM, LOGIN_RETURN_PREFIX};
pub use app::AmazonAppPaths;

pub const AMAZON_SOURCE: &str = "amazon";

#[derive(Debug, Clone, Default)]
pub struct AmazonProvider {
    /// Signed-in account for owned games (Windows; Heroic covers them elsewhere).
    account: Option<Arc<AmazonAccount>>,
    paths: HeroicPaths,
    /// Heroic is only used on Linux; elsewhere this stays `false`.
    enabled: bool,
    /// The Amazon Games app (Windows). Takes the place of Heroic when set.
    app: Option<AmazonAppPaths>,
}

impl AmazonProvider {
    /// Looks at the real environment: the Amazon Games app on Windows, Heroic elsewhere (see
    /// [`HeroicPaths::detect`]).
    pub fn detect() -> Self {
        if cfg!(windows) {
            return Self {
                app: AmazonAppPaths::detect(),
                ..Self::default()
            };
        }
        Self {
            paths: HeroicPaths::detect(),
            enabled: true,
            app: None,
            account: None,
        }
    }

    pub fn with_paths(paths: HeroicPaths) -> Self {
        Self {
            paths,
            enabled: true,
            app: None,
            account: None,
        }
    }

    pub fn with_app(app: AmazonAppPaths) -> Self {
        Self {
            app: Some(app),
            ..Self::default()
        }
    }

    /// Enables account sign-in and the owned-games list (used where Heroic is not).
    pub fn with_account(mut self, account: Arc<AmazonAccount>) -> Self {
        self.account = Some(account);
        self
    }

    fn signed_in_account(&self) -> Option<&AmazonAccount> {
        self.account.as_deref().filter(|a| a.is_signed_in())
    }
}

impl LibraryProvider for AmazonProvider {
    fn id(&self) -> &str {
        AMAZON_SOURCE
    }

    fn name(&self) -> &str {
        "Amazon Games"
    }

    fn icon_svg(&self) -> &'static str {
        crate::providers::icons::AMAZON
    }

    fn can_scan(&self) -> bool {
        self.enabled
            || self.app.as_ref().is_some_and(AmazonAppPaths::has_data)
            || self.signed_in_account().is_some()
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if let Some(app) = &self.app {
            return app.installed_games();
        }
        if !self.enabled {
            return Err(ProviderError::NotFound(
                "Amazon Games are read from the Amazon Games app on Windows and from Heroic on Linux"
                    .into(),
            ));
        }
        heroic::nile_games(&self.paths, false)
    }

    /// Heroic's library, or the signed-in account (the Amazon Games app keeps its entitlements
    /// encrypted with DPAPI, so they cannot be read locally).
    fn can_scan_owned(&self) -> bool {
        self.account.is_some() || (self.enabled && !heroic::nile_library(&self.paths).is_empty())
    }

    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        if self.enabled {
            return heroic::nile_games(&self.paths, true);
        }
        match &self.account {
            Some(account) => account.fetch_owned(),
            None => Err(ProviderError::Unsupported),
        }
    }

    fn account(&self) -> Option<&dyn AccountAuth> {
        self.account.as_deref().map(|a| a as &dyn AccountAuth)
    }

    /// The Amazon Games app (Windows), when its executable exists.
    fn launcher(&self) -> Option<PlayAction> {
        self.app.as_ref()?.launcher_action()
    }

    /// Windows only; Heroic has no install URI.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        self.app.as_ref()?.install_action(source_game_id)
    }

    fn can_uninstall(&self) -> bool {
        self.app.as_ref().is_some_and(AmazonAppPaths::has_client)
    }

    /// Opens the Amazon Games app, where the game is uninstalled (it has no uninstall URI).
    fn uninstall_action(&self, _source_game_id: &str) -> Option<PlayAction> {
        self.app.as_ref()?.uninstall_action()
    }

    /// Heroic's Amazon `art_cover` is a store background or icon, not 2:3 box art, and the Amazon
    /// Games app only has icons and wide backgrounds, so SteamGridDB goes first when configured.
    fn covers_are_low_quality(&self, _source_game_id: &str) -> bool {
        true
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        if self.app.is_some() {
            // No portrait art locally; the wide image is a background, the icon an icon.
            return Vec::new();
        }
        heroic::nile_library(&self.paths)
            .into_iter()
            .find(|e| e.id == source_game_id)
            .and_then(|e| e.cover)
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
            .map(ImageCandidate::Url)
            .into_iter()
            .collect()
    }

    /// The app's store-page cache, then the artwork of the last owned-games scan.
    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.app
            .as_ref()
            .and_then(|a| a.product_art(source_game_id).background)
            .or_else(|| self.account.as_ref()?.art(source_game_id).background)
            .map(ImageCandidate::Url)
            .into_iter()
            .collect()
    }

    fn icon_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.app
            .as_ref()
            .and_then(|a| a.product_art(source_game_id).icon)
            .or_else(|| self.account.as_ref()?.art(source_game_id).icon)
            .map(ImageCandidate::Url)
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn launcher_is_the_app_executable_when_it_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AmazonAppPaths::below(tmp.path());
        assert!(AmazonProvider::with_app(paths.clone()).launcher().is_none());
        write(&tmp.path().join("Amazon Games/App/Amazon Games.exe"), "x");
        let action = AmazonProvider::with_app(paths).launcher().unwrap();
        assert_eq!(action.kind, crate::model::ActionKind::File);
        assert!(action.target.ends_with("Amazon Games.exe"));
        assert!(AmazonProvider::default().launcher().is_none());
    }

    #[test]
    fn scans_installed_owned_and_covers() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let hero = home.join(".config/heroic");
        let fez = home.join("Games/Fez");
        fs::create_dir_all(&fez).unwrap();
        write(
            &hero.join("nile_config/nile/installed.json"),
            &serde_json::json!([{"id": "p1", "version": "1", "path": fez}]).to_string(),
        );
        write(
            &hero.join("store_cache/nile_library.json"),
            r#"{"library": [
                {"app_name": "p1", "title": "Fez", "art_cover": "https://i/fez.jpg"},
                {"app_name": "p2", "title": "Ball", "art_cover": "file:///x.jpg"}
            ]}"#,
        );
        let p = AmazonProvider::with_paths(HeroicPaths::discover(home, None, None));
        assert_eq!((p.id(), p.name()), ("amazon", "Amazon Games"));
        assert!(p.can_scan() && p.can_scan_owned());
        assert_eq!(p.scan_installed().unwrap().len(), 1);
        let owned = p.scan_owned(&serde_json::Value::Null).unwrap();
        assert_eq!(owned.len(), 2);
        assert!(!owned[0].installed && owned[1].installed);
        assert_eq!(
            p.cover_candidates("p1"),
            vec![ImageCandidate::Url("https://i/fez.jpg".into())]
        );
        assert!(p.cover_candidates("p2").is_empty());
        assert!(p.cover_candidates("nope").is_empty());
        // Heroic has no install or uninstall URI and no extra art.
        assert!(p.install_action("p2").is_none() && !p.can_uninstall());
        assert!(p.uninstall_action("p1").is_none());
        assert!(p.background_candidates("p1").is_empty() && p.icon_candidates("p1").is_empty());
    }

    #[test]
    fn without_heroic_or_data() {
        let p = AmazonProvider::default();
        assert!(!p.can_scan() && !p.can_scan_owned());
        assert!(matches!(
            p.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
        assert!(matches!(
            p.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::Unsupported)
        ));
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".config/heroic")).unwrap();
        let p = AmazonProvider::with_paths(HeroicPaths::discover(tmp.path(), None, None));
        assert!(!p.can_scan_owned());
        assert!(p.scan_installed().unwrap().is_empty());
    }

    #[test]
    fn amazon_games_app_provider() {
        let tmp = tempfile::tempdir().unwrap();
        let app = AmazonAppPaths::below(tmp.path());
        let p = AmazonProvider::with_app(app.clone());
        assert!(!p.can_scan() && !p.can_uninstall());
        assert!(matches!(
            p.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));

        let root = tmp.path().join("Amazon Games");
        fs::create_dir_all(root.join("Data/Games/Sql")).unwrap();
        write(&root.join("App/Amazon Games.exe"), "");
        assert!(p.can_scan() && p.can_uninstall());
        assert!(p.scan_installed().unwrap().is_empty());
        // Owned games are not readable locally.
        assert!(!p.can_scan_owned());
        assert!(matches!(
            p.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::Unsupported)
        ));
        assert_eq!(
            p.install_action("abc").unwrap().target,
            "amazon-games://install/abc"
        );
        assert!(
            p.uninstall_action("abc")
                .unwrap()
                .target
                .ends_with("Amazon Games.exe")
        );
        assert!(p.covers_are_low_quality("abc") && p.cover_candidates("abc").is_empty());
        assert!(p.background_candidates("abc").is_empty() && p.icon_candidates("abc").is_empty());
    }

    /// Reads the real Amazon Games app data of this machine (read-only) and prints it:
    /// `cargo test -p questkeep-core amazon::tests::live -- --ignored --nocapture`
    #[test]
    #[ignore = "reads the Amazon Games app of the machine it runs on"]
    #[allow(clippy::print_stdout)]
    fn live_scan() {
        let p = AmazonProvider::detect();
        println!(
            "can_scan={} can_uninstall={}",
            p.can_scan(),
            p.can_uninstall()
        );
        let games = p.scan_installed().unwrap();
        println!("{} installed game(s)", games.len());
        for g in &games {
            println!(
                "{} | {} | {:?} | {:?}",
                g.source_game_id, g.name, g.install_dir, g.play_action
            );
            println!(
                "  background {:?}",
                p.background_candidates(&g.source_game_id)
            );
            println!("  icon {:?}", p.icon_candidates(&g.source_game_id));
        }
        let art = |id: &str| (p.background_candidates(id), p.icon_candidates(id));
        println!(
            "art for the cached product: {:?}",
            art("62f505d5-9210-4784-9094-17cdc868f6da")
        );
        println!("uninstall: {:?}", p.uninstall_action("x"));
        println!("install: {:?}", p.install_action("x"));
    }
}
