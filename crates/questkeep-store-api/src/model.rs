//! What stores report: games, how to start them, their metadata. Plain serde data shared with the
//! UI through ts-rs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Store metadata for one game, as returned by a provider. Every field is optional: a store fills
/// what it knows, the library picks per field from the sources in the user's priority order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GameMetadata {
    /// Plain text (providers strip HTML).
    pub description: Option<String>,
    /// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`.
    pub release_date: Option<String>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    /// User or store tags ("Roguelike", "Great Soundtrack", ...).
    pub tags: Vec<String>,
    /// What the game supports: "Single-player", "Co-op", "Controller support", "Cloud saves", ...
    pub features: Vec<String>,
    /// Franchise or series names.
    pub series: Vec<String>,
    /// "PEGI 16", "ESRB Teen", "USK 12", ...
    pub age_ratings: Vec<String>,
    /// "Windows", "macOS", "Linux", console names.
    pub platforms: Vec<String>,
    pub game_modes: Vec<String>,
    pub themes: Vec<String>,
    pub perspectives: Vec<String>,
    /// Press score, 0-100.
    pub critic_score: Option<Score>,
    /// Player score, 0-100 (e.g. the share of positive reviews).
    pub community_score: Option<Score>,
    #[ts(type = "number | null")]
    pub install_size: Option<u64>,
    pub time_to_beat: Option<TimeToBeat>,
    /// Pages about the game (store page, wiki, community, socials). Merged across sources.
    pub links: Vec<GameLink>,
}

/// A 0-100 score and who gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Score {
    pub value: u8,
    /// Shown next to the score, e.g. `Metacritic` or `Steam reviews`.
    pub source: String,
}

/// How long the game takes, in seconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct TimeToBeat {
    #[ts(type = "number | null")]
    pub main: Option<u64>,
    #[ts(type = "number | null")]
    pub extra: Option<u64>,
    #[ts(type = "number | null")]
    pub complete: Option<u64>,
}

impl TimeToBeat {
    pub fn is_empty(&self) -> bool {
        self.main.is_none() && self.extra.is_none() && self.complete.is_none()
    }
}

/// A web page about a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameLink {
    pub name: String,
    pub url: String,
}

impl GameLink {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
        }
    }
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
    /// Whether the store lists it as an app (software, tool) rather than a game; `None` when
    /// the store doesn't say (the entry keeps what it had, new ones are games).
    #[serde(default)]
    pub app: Option<bool>,
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
            app: None,
        }
    }
}

/// A news item about a game (game page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewsItem {
    pub title: String,
    pub url: String,
    /// Unix seconds.
    #[ts(type = "number | null")]
    pub date: Option<i64>,
    /// Who wrote it, e.g. "Steam Community Announcements" or "PC Gamer".
    pub source: String,
    /// Plain text, markup removed, at most a few hundred characters.
    pub snippet: String,
    /// Image URL, when the item has one.
    pub image: Option<String>,
}

/// The latest news of a game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameNews {
    /// Newest first.
    pub items: Vec<NewsItem>,
    /// The store's page with all news for the game.
    pub all_url: Option<String>,
}

/// An achievement the user unlocked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Achievement {
    pub name: String,
    pub description: Option<String>,
    /// Icon URL.
    pub icon: Option<String>,
    /// Unix seconds; `None` when the store doesn't say.
    #[ts(type = "number | null")]
    pub unlocked_at: Option<i64>,
    /// Share of all players that have it (0-100), when known.
    pub percent: Option<f32>,
}

/// The user's progress in a game's achievements.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Achievements {
    /// How many the game has.
    pub total: u32,
    /// The unlocked ones, most recent first.
    pub unlocked: Vec<Achievement>,
}
