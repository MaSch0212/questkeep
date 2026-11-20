use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

pub use questkeep_store_api::model::{
    Achievement, Achievements, ActionKind, CatalogHit, GameLink, GameMetadata, GameNews,
    ImportedGame, NewsItem, PlayAction, Score, TimeToBeat, TrackingMode,
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

/// `games.extra` key of a game's [`MetadataLink`].
pub const METADATA_LINK_KEY: &str = "metadataLink";
/// `games.extra` key of the name the user wants metadata and artwork searched by.
pub const METADATA_NAME_KEY: &str = "metadataName";

/// A name without a trailing release year in brackets ("Mass Effect 2 (2010)"), and that year.
fn split_year(name: &str) -> (&str, Option<i32>) {
    let trimmed = name.trim();
    let split = trimmed.strip_suffix(')').and_then(|head| {
        let open = head.rfind('(')?;
        let year: i32 = head[open + 1..].trim().parse().ok()?;
        let base = head[..open].trim_end();
        ((1950..=2100).contains(&year) && !base.is_empty()).then_some((base, year))
    });
    match split {
        Some((base, year)) => (base, Some(year)),
        None => (trimmed, None),
    }
}

/// The store or IGDB game a game added by hand takes its metadata and images from, picked by
/// the user from a search. Stored in `extra`, so refreshing metadata keeps using it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MetadataLink {
    /// `igdb` or a store id (`steam`, `gog`, ...).
    pub source: String,
    /// The game's id there.
    pub id: String,
    /// Its name there, for showing the link.
    pub name: String,
}

impl Game {
    /// The name the user set for metadata and artwork searches, if any.
    pub fn metadata_name(&self) -> Option<String> {
        self.extra
            .get(METADATA_NAME_KEY)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
    }

    /// The name to search metadata and artwork by: the user's metadata name, else the name
    /// without a trailing "(2010)" (some stores add the release year, which searches miss).
    pub fn lookup_name(&self) -> String {
        self.metadata_name()
            .unwrap_or_else(|| split_year(&self.name).0.to_string())
    }

    /// The release year for searches: the known release date's, else a "(2010)" in the name.
    pub fn lookup_year(&self) -> Option<i32> {
        self.release_date
            .as_deref()
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse().ok())
            .or_else(|| split_year(&self.name).1)
    }

    /// The game's metadata link, when it has a valid one.
    pub fn metadata_link(&self) -> Option<MetadataLink> {
        let value = self.extra.get(METADATA_LINK_KEY)?;
        serde_json::from_value(value.clone()).ok()
    }

    /// The IGDB game the user linked this game to.
    pub fn linked_igdb_id(&self) -> Option<u64> {
        self.metadata_link()
            .filter(|l| l.source == super::IGDB_SOURCE)
            .and_then(|l| l.id.parse().ok())
    }

    /// The store game a game added by hand is linked to: its store and id there.
    pub fn linked_store_game(&self) -> Option<(String, String)> {
        if self.source != MANUAL_SOURCE {
            return None;
        }
        self.metadata_link()
            .filter(|l| l.source != super::IGDB_SOURCE)
            .map(|l| (l.source, l.id))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_trailing_release_year() {
        assert_eq!(
            split_year("Mass Effect 2 (2010)"),
            ("Mass Effect 2", Some(2010))
        );
        assert_eq!(split_year(" Doom (1993) "), ("Doom", Some(1993)));
        assert_eq!(split_year("Doom"), ("Doom", None));
        assert_eq!(split_year("Game (Classic)"), ("Game (Classic)", None));
        assert_eq!(split_year("(2010)"), ("(2010)", None));
        assert_eq!(split_year("Game (12)"), ("Game (12)", None));
    }

    #[test]
    fn lookup_name_prefers_the_users_metadata_name() {
        let mut game: Game = serde_json::from_str(
            r#"{
            "id": "00000000-0000-0000-0000-000000000000", "name": "Mass Effect 3 (2012)",
            "sortingName": "", "source": "ea", "sourceGameId": "1", "installed": false,
            "installDir": null, "playAction": null, "playtimeSeconds": 0, "playCount": 0,
            "lastPlayed": null, "dateAdded": "2026-01-01T00:00:00Z", "favorite": false,
            "hidden": false, "kind": "game", "detectLaunches": true, "ignoredProcesses": [],
            "coverPath": null, "iconPath": null, "backgroundPath": null, "description": null,
            "releaseDate": null, "developers": [], "publishers": [], "genres": [], "tags": [],
            "features": [], "series": [], "ageRatings": [], "platforms": [], "gameModes": [],
            "themes": [], "perspectives": [], "criticScore": null, "communityScore": null,
            "installSize": null, "timeToBeat": null, "links": [], "fieldSources": {},
            "completionStatus": null, "userScore": null, "notes": null, "categories": [],
            "copies": [], "metadataUpdatedAt": null, "extra": {}
        }"#,
        )
        .unwrap();
        assert_eq!(game.lookup_name(), "Mass Effect 3");
        assert_eq!(game.lookup_year(), Some(2012));
        game.release_date = Some("2012-03-06".into());
        assert_eq!(game.lookup_year(), Some(2012));
        game.extra = serde_json::json!({ METADATA_NAME_KEY: "  Mass Effect 3: Legendary  " });
        assert_eq!(
            game.metadata_name().as_deref(),
            Some("Mass Effect 3: Legendary")
        );
        assert_eq!(game.lookup_name(), "Mass Effect 3: Legendary");
        game.extra = serde_json::json!({ METADATA_NAME_KEY: " " });
        assert_eq!(game.metadata_name(), None);
    }
}
