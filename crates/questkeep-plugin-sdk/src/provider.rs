//! The trait a store plugin implements, and how its methods are reached from the app.

use std::collections::HashMap;

use serde::Serialize;

use crate::model::{Achievements, AddGameOption, GameMetadata, GameNews, ImportedGame, PlayAction};
use crate::settings::StoreSettings;
use crate::wire::{CallArgs, ImageCandidate, Method, ProviderDescriptor};

/// Errors a provider can report. Messages are shown to users.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The store client is not installed or could not be located.
    NotFound(String),
    /// A file could not be read; say which one.
    Io(String),
    Parse(String),
    /// The provider does not support the operation.
    Unsupported,
    /// The operation needs a setting that is missing.
    NotConfigured(String),
    Network(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::NotFound(m)
            | ProviderError::Io(m)
            | ProviderError::Parse(m)
            | ProviderError::NotConfigured(m) => f.write_str(m),
            ProviderError::Unsupported => {
                f.write_str("this provider does not support that operation")
            }
            ProviderError::Network(m) => write!(f, "network error: {m}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<std::io::Error> for ProviderError {
    fn from(err: std::io::Error) -> Self {
        ProviderError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(err: serde_json::Error) -> Self {
        ProviderError::Parse(err.to_string())
    }
}

/// A store that reports games. It mirrors the app's own provider interface method for method
/// (without accounts: plugin stores have no sign-in yet). Methods take `&self`; keep what
/// [`configure`](Self::configure) hands you in a `Cell` or `RefCell`.
///
/// The app asks for [`id`](Self::id), [`name`](Self::name), [`icon_svg`](Self::icon_svg), the
/// `can_*`/`has_*` flags, [`shop_url`](Self::shop_url), [`launcher`](Self::launcher),
/// [`is_installed`](Self::is_installed), [`settings`](Self::settings) and
/// [`add_game_options`](Self::add_game_options) when the plugin is loaded and again after every
/// `configure`, and remembers the answers; the id, name, icon and shop URL are fixed once loaded.
pub trait Provider {
    /// Stable id stored with every game. It must be the plugin's id, or start with it and `-`.
    fn id(&self) -> String;
    /// Human-readable name, e.g. `Example Store`.
    fn name(&self) -> String;
    /// The store's logo: a complete monochrome `<svg>` drawn with `currentColor`. Without one
    /// (the default) the app shows a generic plugin icon.
    fn icon_svg(&self) -> String {
        String::new()
    }
    /// Whether [`scan_installed`](Self::scan_installed) is meaningful for this provider.
    fn can_scan(&self) -> bool {
        true
    }
    /// Games currently installed on this machine.
    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError>;
    /// Whether [`scan_owned`](Self::scan_owned) is implemented.
    fn can_scan_owned(&self) -> bool {
        false
    }
    /// Games owned in the store, installed or not (`installed: false` for the latter). `config`
    /// is this provider's section of the user settings (`null` when absent).
    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        Err(ProviderError::Unsupported)
    }
    /// How to install a game that is owned but not installed (usually a store URI).
    fn install_action(&self, _source_game_id: &str) -> Option<PlayAction> {
        None
    }
    /// Whether [`uninstall_action`](Self::uninstall_action) can work at all on this machine.
    fn can_uninstall(&self) -> bool {
        false
    }
    /// How to hand the uninstall of an installed game to its store client.
    fn uninstall_action(&self, _source_game_id: &str) -> Option<PlayAction> {
        None
    }
    /// Whether [`launcher_page`](Self::launcher_page) can work on this machine.
    fn has_launcher_pages(&self) -> bool {
        false
    }
    /// The store's website where games are bought (an https URL).
    fn shop_url(&self) -> Option<String> {
        None
    }
    /// Opens the game's own page in the store's launcher.
    fn launcher_page(&self, _source_game_id: &str) -> Option<PlayAction> {
        None
    }
    /// The user's own grouping of games in the store: store game id -> category names.
    /// `Ok(None)` means the store has nothing like categories or they could not be read.
    fn scan_categories(
        &self,
        _config: &serde_json::Value,
    ) -> Result<Option<HashMap<String, Vec<String>>>, ProviderError> {
        Ok(None)
    }
    /// Store metadata (description, genres, ...); `Ok(None)` when the store has none.
    fn metadata(&self, _source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        Ok(None)
    }
    /// How many people play the game right now.
    fn player_count(&self, _source_game_id: &str) -> Result<Option<u32>, ProviderError> {
        Ok(None)
    }
    /// The game's latest news (a handful of items).
    fn news(&self, _source_game_id: &str) -> Result<Option<GameNews>, ProviderError> {
        Ok(None)
    }
    /// The user's achievements in the game.
    fn achievements(
        &self,
        _config: &serde_json::Value,
        _source_game_id: &str,
    ) -> Result<Option<Achievements>, ProviderError> {
        Ok(None)
    }
    /// Applies the provider's settings (`null` when unset). Called before every scan.
    fn configure(&self, _config: &serde_json::Value) {}
    /// Where a portrait cover (2:3) for the game may be found, best first.
    fn cover_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// Whether this game's cover candidates are poor stand-ins for real 2:3 box art.
    fn covers_are_low_quality(&self, _source_game_id: &str) -> bool {
        false
    }
    /// Where a wide background image for the game may be found, best first.
    fn background_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// Where a small square game icon may be found, best first.
    fn icon_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// The game's launch with extra command line arguments, or `None` when the store can't pass
    /// arguments to the game.
    fn launch_with_args(&self, _source_game_id: &str, _args: &str) -> Option<PlayAction> {
        None
    }
    /// The action that opens the store's own launcher, or `None` without one. Must be cheap.
    fn launcher(&self) -> Option<PlayAction> {
        None
    }
    /// Whether the store's client is on this PC. Stores that aren't installed are not synced
    /// (unless [`has_owned_source`](Self::has_owned_source)): a store without a client that
    /// reads its games from files returns `true`.
    fn is_installed(&self) -> bool {
        self.launcher().is_some()
    }
    /// Whether owned games can be read without the client (e.g. with a key in `config`).
    fn has_owned_source(&self, _config: &serde_json::Value) -> bool {
        false
    }
    /// The store's own settings; their values arrive as `config`. Only `options` are shown for
    /// plugin stores so far.
    fn settings(&self) -> StoreSettings {
        StoreSettings::default()
    }
    /// Extra ways to add games that this provider offers next to "Add game".
    fn add_game_options(&self) -> Vec<AddGameOption> {
        Vec::new()
    }
    /// Starts one of [`add_game_options`](Self::add_game_options).
    fn run_add_game_option(&self, _option_id: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported)
    }
}

/// The answers the app remembers (the plugin's `describe` export).
pub fn descriptor<P: Provider + ?Sized>(provider: &P) -> ProviderDescriptor {
    ProviderDescriptor {
        id: provider.id(),
        name: provider.name(),
        icon_svg: provider.icon_svg(),
        can_scan: provider.can_scan(),
        can_scan_owned: provider.can_scan_owned(),
        can_uninstall: provider.can_uninstall(),
        has_launcher_pages: provider.has_launcher_pages(),
        shop_url: provider.shop_url(),
        launcher: provider.launcher(),
        installed: provider.is_installed(),
        settings: provider.settings(),
        add_game_options: provider.add_game_options(),
    }
}

/// [`descriptor`] as JSON.
pub fn describe<P: Provider + ?Sized>(provider: &P) -> String {
    serde_json::to_string(&descriptor(provider)).unwrap_or_default()
}

/// Runs one provider method from its wire form (the plugin's `call` export).
pub fn dispatch<P: Provider + ?Sized>(
    provider: &P,
    method: &str,
    args: &str,
) -> Result<String, ProviderError> {
    let method = Method::from_name(method).ok_or(ProviderError::Unsupported)?;
    let a: CallArgs = if args.trim().is_empty() {
        CallArgs::default()
    } else {
        serde_json::from_str(args)
            .map_err(|e| ProviderError::Parse(format!("invalid call arguments: {e}")))?
    };
    let id = a.game_id.as_str();
    match method {
        Method::ScanInstalled => json(&provider.scan_installed()?),
        Method::ScanOwned => json(&provider.scan_owned(&a.config)?),
        Method::ScanCategories => json(&provider.scan_categories(&a.config)?),
        Method::InstallAction => json(&provider.install_action(id)),
        Method::UninstallAction => json(&provider.uninstall_action(id)),
        Method::LauncherPage => json(&provider.launcher_page(id)),
        Method::LaunchWithArgs => json(&provider.launch_with_args(id, &a.args)),
        Method::Metadata => json(&provider.metadata(id)?),
        Method::PlayerCount => json(&provider.player_count(id)?),
        Method::News => json(&provider.news(id)?),
        Method::Achievements => json(&provider.achievements(&a.config, id)?),
        Method::CoverCandidates => json(&provider.cover_candidates(id)),
        Method::CoversAreLowQuality => json(&provider.covers_are_low_quality(id)),
        Method::BackgroundCandidates => json(&provider.background_candidates(id)),
        Method::IconCandidates => json(&provider.icon_candidates(id)),
        Method::HasOwnedSource => json(&provider.has_owned_source(&a.config)),
        Method::Configure => {
            provider.configure(&a.config);
            json(&())
        }
        Method::RunAddGameOption => json(&provider.run_add_game_option(&a.option_id)?),
    }
}

fn json<T: Serialize>(value: &T) -> Result<String, ProviderError> {
    serde_json::to_string(value)
        .map_err(|e| ProviderError::Parse(format!("could not encode the answer: {e}")))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[derive(Default)]
    struct Fake {
        demos: Cell<bool>,
    }

    impl Provider for Fake {
        fn id(&self) -> String {
            "fake".into()
        }
        fn name(&self) -> String {
            "Fake".into()
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            let mut games = vec![ImportedGame::installed("1", "One")];
            if self.demos.get() {
                games.push(ImportedGame::installed("2", "Demo"));
            }
            Ok(games)
        }
        fn configure(&self, config: &serde_json::Value) {
            self.demos.set(config["demos"] == true);
        }
        fn install_action(&self, id: &str) -> Option<PlayAction> {
            Some(PlayAction::uri(format!("fake://install/{id}")))
        }
    }

    #[test]
    fn dispatches_by_name_with_json() {
        let fake = Fake::default();
        let games: Vec<ImportedGame> =
            serde_json::from_str(&dispatch(&fake, "scan_installed", "").unwrap()).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(
            dispatch(&fake, "configure", r#"{"config":{"demos":true}}"#).unwrap(),
            "null"
        );
        let games: Vec<ImportedGame> =
            serde_json::from_str(&dispatch(&fake, "scan_installed", "{}").unwrap()).unwrap();
        assert_eq!(games.len(), 2);
        let action: Option<PlayAction> =
            serde_json::from_str(&dispatch(&fake, "install_action", r#"{"gameId":"9"}"#).unwrap())
                .unwrap();
        assert_eq!(action.unwrap().target, "fake://install/9");
    }

    #[test]
    fn defaults_and_errors() {
        let fake = Fake::default();
        assert_eq!(
            dispatch(&fake, "scan_owned", "{}"),
            Err(ProviderError::Unsupported)
        );
        assert_eq!(
            dispatch(&fake, "no_such", "{}"),
            Err(ProviderError::Unsupported)
        );
        assert!(matches!(
            dispatch(&fake, "metadata", "nonsense"),
            Err(ProviderError::Parse(_))
        ));
        assert_eq!(dispatch(&fake, "metadata", "{}").unwrap(), "null");
        let d = descriptor(&fake);
        assert_eq!(d.id, "fake");
        assert!(d.can_scan && !d.installed && !d.can_scan_owned);
    }
}
