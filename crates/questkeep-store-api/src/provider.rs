//! The interface a store implements to plug into Questkeep.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::auth::AccountAuth;
use crate::model::{Achievements, GameMetadata, GameNews, ImportedGame, PlayAction};
use crate::settings::StoreSettings;

/// Errors a provider can report. Messages are shown to users.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The store client is not installed or could not be located.
    #[error("{0}")]
    NotFound(String),
    #[error("could not read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Parse(String),
    #[error("this provider does not support that operation")]
    Unsupported,
    /// The operation needs user configuration (e.g. an API key) that is missing.
    #[error("{0}")]
    NotConfigured(String),
    #[error("network error: {0}")]
    Network(String),
}

/// A store integration that reports games. Implementations must be plain data in, plain data out
/// (no UI or database access) so they can later run as sandboxed plugins.
pub trait LibraryProvider: Send + Sync {
    /// Stable id stored in `games.source`, e.g. `steam`.
    fn id(&self) -> &str;
    /// Human-readable name, e.g. `Steam`.
    fn name(&self) -> &str;
    /// The store's logo: a complete monochrome `<svg>` drawn with `currentColor`.
    fn icon_svg(&self) -> &'static str;
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
    /// Games owned in the store account, installed or not. Entries for games that are not
    /// installed must have `installed: false`. `config` is this provider's section of the user
    /// settings (`settings.providers.<id>`, `null` when absent), e.g. an API key. Return
    /// `NotConfigured` when required configuration is missing.
    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        Err(ProviderError::Unsupported)
    }
    /// How to install a game that is owned but not installed (usually a store URI such as
    /// `steam://install/440`). `None` when the provider can't install games.
    fn install_action(&self, _source_game_id: &str) -> Option<PlayAction> {
        None
    }
    /// Whether [`uninstall_action`](Self::uninstall_action) can work at all on this machine (the
    /// UI only offers Uninstall for such providers). Individual games may still fail, e.g. when
    /// their uninstaller is missing.
    fn can_uninstall(&self) -> bool {
        false
    }
    /// How to hand the uninstall of an installed game to its store client (e.g. the URI
    /// `steam://uninstall/440`, or the game's own uninstaller). The store or uninstaller asks the
    /// user to confirm; Questkeep never deletes files itself. `None` when this game can't be
    /// uninstalled from Questkeep.
    fn uninstall_action(&self, _source_game_id: &str) -> Option<PlayAction> {
        None
    }
    /// The user's own grouping of games in the store (Steam collections, GOG Galaxy tags, Lutris
    /// categories): store game id -> category names. Called after every scan with the same
    /// `config` as [`scan_owned`](Self::scan_owned). `Ok(None)` (the default) means the store has
    /// nothing like categories, or they couldn't be read this time; the library then leaves the
    /// imported categories alone. `Ok(Some(map))` replaces this store's imported categories: games
    /// missing from the map lose them. Categories the user assigned in Questkeep are never touched.
    fn scan_categories(
        &self,
        _config: &serde_json::Value,
    ) -> Result<Option<HashMap<String, Vec<String>>>, ProviderError> {
        Ok(None)
    }
    /// Store metadata (description, genres, ...). Called from a background thread with at most
    /// one request in flight per provider; `Ok(None)` means the store has no data for this game.
    fn metadata(&self, _source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        Ok(None)
    }
    /// How many people play the game right now. Called only while its game page is open;
    /// `Ok(None)` (the default) when the store can't tell.
    fn player_count(&self, _source_game_id: &str) -> Result<Option<u32>, ProviderError> {
        Ok(None)
    }
    /// The game's latest news (a handful of items). Called only while its game page is open;
    /// `Ok(None)` (the default) when the store has no news feed.
    fn news(&self, _source_game_id: &str) -> Result<Option<GameNews>, ProviderError> {
        Ok(None)
    }
    /// The signed-in user's achievements in the game, with the same `config` as
    /// [`scan_owned`](Self::scan_owned). `Ok(None)` (the default) when the store has none for
    /// it or no account or key is set up to read them.
    fn achievements(
        &self,
        _config: &serde_json::Value,
        _source_game_id: &str,
    ) -> Result<Option<Achievements>, ProviderError> {
        Ok(None)
    }
    /// Applies the provider's settings (`providers.<id>` in settings.json, `Null` when unset).
    /// Called before every scan, so a changed setting takes effect with the next scan.
    fn configure(&self, _config: &serde_json::Value) {}
    /// Account sign-in for this store, when the provider supports it (see [`crate::auth`]).
    fn account(&self) -> Option<&dyn AccountAuth> {
        None
    }
    /// Where a portrait cover (2:3) for the game may be found, best first. Local files are
    /// preferred over downloads by convention.
    fn cover_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// Whether this game's cover candidates are poor stand-ins for real 2:3 box art (e.g. square
    /// app logos). When true and a SteamGridDB key is configured, SteamGridDB is tried first and
    /// the provider's candidates only serve as fallback. Called right before
    /// [`cover_candidates`](Self::cover_candidates) for the same game.
    fn covers_are_low_quality(&self, _source_game_id: &str) -> bool {
        false
    }
    /// Where a wide background / hero image for the game may be found, best first.
    fn background_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// Where a small square game icon may be found, best first (local files before downloads).
    /// Called from a background thread, only for games that still lack an icon. The image
    /// fetcher decodes each candidate, rejects non-square ones and scales the first valid one to
    /// 32x32, so a candidate that turns out unusable is simply skipped. When this is empty (the
    /// default) the icon comes from the executable or, last, from the cover.
    fn icon_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// The game's launch with extra command line arguments, for launch profiles, or `None` when
    /// the store can't pass arguments to the game (the default). Only asked for store URI
    /// launches; an executable launch takes arguments directly.
    fn launch_with_args(&self, _source_game_id: &str, _args: &str) -> Option<PlayAction> {
        None
    }
    /// The action that opens the store's own launcher (a store URI or the client's executable),
    /// or `None` when the launcher isn't installed on this PC or the provider has none. Must be
    /// cheap: the UI asks every time it shows the "Open launcher" menu.
    fn launcher(&self) -> Option<PlayAction> {
        None
    }
    /// Whether the store's client is on this PC. Stores that aren't installed are not synced
    /// (unless [`has_owned_source`](Self::has_owned_source)) and offer no add-game options.
    fn is_installed(&self) -> bool {
        self.launcher().is_some()
    }
    /// Whether owned games can be read without the client: a signed-in account (or, for Steam, a
    /// Web API key in `config`).
    fn has_owned_source(&self, _config: &serde_json::Value) -> bool {
        self.account().is_some_and(|a| a.status().signed_in)
    }
    /// The store's own settings (see [`StoreSettings`]); their values arrive as `config`.
    fn settings(&self) -> StoreSettings {
        StoreSettings::default()
    }
    /// Extra ways to add games that this provider offers next to "Add game", e.g. the launcher's
    /// own "new instance" dialog. Empty by default.
    fn add_game_options(&self) -> Vec<AddGameOption> {
        Vec::new()
    }
    /// Starts one of [`add_game_options`](Self::add_game_options) (opens the launcher's dialog)
    /// and returns without waiting for the user. The library watches the installed scan for the
    /// game the user adds there.
    fn run_add_game_option(&self, _option_id: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported)
    }
}

/// An extra way to add games, shown in the menu of the "Add game" button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddGameOption {
    /// Stable within the provider.
    pub id: String,
    /// Menu text, e.g. `New Minecraft instance`.
    pub label: String,
}

/// A possible source for a game image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageCandidate {
    File(std::path::PathBuf),
    Url(String),
}
