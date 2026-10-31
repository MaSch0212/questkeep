use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Provider id used for games added by hand.
pub const MANUAL_SOURCE: &str = "manual";

/// A game in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Game {
    /// Stable id (UUID v4), never reused.
    pub id: Uuid,
    pub name: String,
    /// Derived key used for ordering (leading article stripped, numbers padded).
    pub sorting_name: String,
    /// Provider id that owns this game (`manual`, `steam`, ...).
    pub source: String,
    /// The store's own id (Steam app id, ...). `None` for manual games.
    pub source_game_id: Option<String>,
    /// `false` when the provider no longer reports the game as installed.
    pub installed: bool,
    pub install_dir: Option<String>,
    pub play_action: Option<PlayAction>,
    #[ts(type = "number")]
    pub playtime_seconds: u64,
    #[ts(type = "number")]
    pub play_count: u64,
    pub last_played: Option<DateTime<Utc>>,
    pub date_added: DateTime<Utc>,
    pub favorite: bool,
    pub hidden: bool,
    /// Whether Questkeep notices this game when it is started outside Questkeep (from a store
    /// client, a mod manager, ...) and records the session.
    pub detect_launches: bool,
    /// Program names (case-insensitive, `.exe` optional, `*` as wildcard) that don't count as
    /// playing this game, e.g. its launcher. Applies to launches from Questkeep too.
    pub ignored_processes: Vec<String>,
    /// Absolute path of the cached cover image when returned by the library API.
    pub cover_path: Option<String>,
    /// Absolute path of the cached icon when returned by the library API.
    pub icon_path: Option<String>,
    /// Absolute path of the cached background (hero) image when returned by the library API.
    pub background_path: Option<String>,
    /// Plain-text description from store metadata.
    pub description: Option<String>,
    /// Release date as `YYYY-MM-DD`, `YYYY-MM` or `YYYY`.
    pub release_date: Option<String>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    /// Ids of the game's categories, in no particular order.
    pub categories: Vec<String>,
    /// The stores this game is in: its own row first unless reordered, then the copies combined
    /// into it, in play order (the first installed one plays). `installed`, playtime and last
    /// played above cover all of them. One entry for a game in a single store.
    pub copies: Vec<GameCopy>,
    /// When store metadata was last fetched; `None` means never.
    pub metadata_updated_at: Option<DateTime<Utc>>,
    /// Free-form data for future fields and plugins.
    #[ts(type = "Record<string, unknown>")]
    pub extra: serde_json::Value,
}

/// One store's copy of a library entry (see [`Game::copies`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameCopy {
    /// The copy's own game id (the entry's id for its own row).
    pub id: Uuid,
    pub source: String,
    pub source_game_id: Option<String>,
    pub installed: bool,
    #[ts(type = "number")]
    pub playtime_seconds: u64,
    pub last_played: Option<DateTime<Utc>>,
}

/// Store metadata for one game, as returned by a provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameMetadata {
    /// Plain text (providers strip HTML).
    pub description: Option<String>,
    /// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`.
    pub release_date: Option<String>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
}

/// How a game is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ActionKind {
    /// Start an executable.
    File,
    /// Open a URI with the system handler (e.g. `steam://rungameid/440`).
    Uri,
}

/// How the launcher decides that a game is still running.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TrackingMode {
    /// Process tree for `file` actions, install directory for `uri` actions.
    #[default]
    Default,
    /// The started process and all of its descendants.
    Process,
    /// Only the started process.
    OriginalProcess,
    /// Any process whose executable lives under `tracking_path` (or the install dir).
    Directory,
    /// Any process whose executable file name equals `tracking_path`.
    Name,
}

/// The single play action of a game. Stored as JSON so it can grow into a list later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlayAction {
    pub kind: ActionKind,
    /// Executable path for `file`, URI for `uri`.
    pub target: String,
    /// Command-line arguments as typed by the user.
    #[serde(default)]
    pub args: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub tracking: TrackingMode,
    #[serde(default)]
    pub tracking_path: Option<String>,
}

impl PlayAction {
    pub fn file(target: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::File,
            target: target.into(),
            args: None,
            working_dir: None,
            tracking: TrackingMode::Default,
            tracking_path: None,
        }
    }

    pub fn uri(target: impl Into<String>) -> Self {
        Self {
            kind: ActionKind::Uri,
            ..Self::file(target)
        }
    }
}

/// A game as reported by a library provider scan. Plain data so providers can later run as
/// sandboxed plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportedGame {
    pub source_game_id: String,
    pub name: String,
    /// `false` for games that are owned but not installed (from `scan_owned`).
    #[serde(default = "default_true")]
    pub installed: bool,
    pub install_dir: Option<String>,
    pub play_action: Option<PlayAction>,
    /// Playtime known to the store. Merged as the maximum with Questkeep's own tracking.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub playtime_seconds: Option<u64>,
    /// Last played according to the store. Merged as the later of both values.
    #[serde(default)]
    pub last_played: Option<DateTime<Utc>>,
}

fn default_true() -> bool {
    true
}

impl ImportedGame {
    /// An installed game with the required fields; everything else empty.
    pub fn installed(source_game_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            source_game_id: source_game_id.into(),
            name: name.into(),
            installed: true,
            install_dir: None,
            play_action: None,
            playtime_seconds: None,
            last_played: None,
        }
    }
}

/// Input for adding a game by hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewManualGame {
    /// Display name. When empty, it is derived from the executable path.
    #[serde(default)]
    pub name: Option<String>,
    pub executable: String,
    #[serde(default)]
    pub args: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Image file to copy into the cover cache.
    #[serde(default)]
    pub cover_image: Option<String>,
}

/// Partial update of user-editable fields. `None` leaves a field unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub favorite: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub detect_launches: Option<bool>,
    #[serde(default)]
    pub ignored_processes: Option<Vec<String>>,
    #[serde(default)]
    pub play_action: Option<PlayAction>,
}

/// Result of applying a provider scan to the library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanSummary {
    pub added: u32,
    pub updated: u32,
    /// Games of this source that were not found anymore and are now marked as not installed.
    pub uninstalled: u32,
    #[serde(skip)]
    #[ts(skip)]
    pub changed_ids: Vec<Uuid>,
}
