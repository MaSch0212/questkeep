use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

pub use questkeep_store_api::model::{
    Achievement, Achievements, ActionKind, GameLink, GameMetadata, GameNews, ImportedGame,
    NewsItem, PlayAction, Score, TimeToBeat, TrackingMode,
};

/// Provider id used for games added by hand.
pub const MANUAL_SOURCE: &str = "manual";

/// Whether a library entry is a game or an app (store tools, software, anything marked by hand).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GameKind {
    #[default]
    Game,
    App,
}

impl GameKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Game => "game",
            Self::App => "app",
        }
    }

    pub fn parse(text: &str) -> Self {
        if text == "app" { Self::App } else { Self::Game }
    }
}

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
    /// Game or app: the user's choice if they made one, else what the store says.
    pub kind: GameKind,
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
    pub tags: Vec<String>,
    pub features: Vec<String>,
    pub series: Vec<String>,
    pub age_ratings: Vec<String>,
    pub platforms: Vec<String>,
    pub game_modes: Vec<String>,
    pub themes: Vec<String>,
    pub perspectives: Vec<String>,
    pub critic_score: Option<Score>,
    pub community_score: Option<Score>,
    #[ts(type = "number | null")]
    pub install_size: Option<u64>,
    pub time_to_beat: Option<TimeToBeat>,
    /// Pages about the game from all sources.
    pub links: Vec<GameLink>,
    /// Metadata field key (see [`super::MetadataField`]) -> id of the source its value came from.
    pub field_sources: BTreeMap<String, String>,
    /// Id of the user's completion status (see [`super::CompletionStatus`]); `None` for none.
    pub completion_status: Option<String>,
    /// The user's own 0-100 score.
    pub user_score: Option<u8>,
    pub notes: Option<String>,
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
    /// Marks the entry as a game or an app; wins over what the store says.
    #[ts(optional)]
    pub kind: Option<GameKind>,
}

/// One change to several library entries at once ("Change all" with several games selected).
/// `None` and empty lists leave that part alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct BulkChange {
    #[ts(optional)]
    pub favorite: Option<bool>,
    #[ts(optional)]
    pub hidden: Option<bool>,
    #[ts(optional)]
    pub kind: Option<GameKind>,
    /// Categories every entry gets.
    pub add_categories: Vec<String>,
    /// Categories taken off every entry.
    pub remove_categories: Vec<String>,
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
