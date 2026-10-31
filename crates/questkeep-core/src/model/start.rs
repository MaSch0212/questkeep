use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Game;

/// Everything the start page shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartPage {
    pub recently_played: Vec<ShelfGame>,
    pub recently_added: Vec<ShelfGame>,
    /// Playtime per local day, oldest first, the last entry is today.
    pub playtime: Vec<DayPlaytime>,
    /// Installed and owned games per store, most owned first.
    pub stores: Vec<StoreInstalls>,
}

/// A game on a start page shelf with the date its badge shows (last played or added).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShelfGame {
    pub game: Game,
    pub date: DateTime<Utc>,
}

/// Credited playtime on one local calendar day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DayPlaytime {
    pub date: NaiveDate,
    #[ts(type = "number")]
    pub seconds: u64,
}

/// How many of the games a store holds are installed. A game in several stores counts once in
/// each of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreInstalls {
    pub source: String,
    pub installed: u32,
    pub owned: u32,
}

/// Credited playtime in one local week (Monday to Sunday).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WeekPlaytime {
    /// The week's Monday.
    pub start: NaiveDate,
    #[ts(type = "number")]
    pub seconds: u64,
}

/// A game's recent play activity (game page).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameActivity {
    /// Oldest first; the last entry is the current week.
    pub weeks: Vec<WeekPlaytime>,
    /// Play sessions that ran in these weeks.
    pub sessions: u32,
}
