use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Id of the default status new games get.
pub const NOT_PLAYED_STATUS: &str = "not-played";
/// Id of the default status set when a game is started for the first time.
pub const PLAYED_STATUS: &str = "played";
/// Id of the third default status.
pub const BEATEN_STATUS: &str = "beaten";

/// One of the user's completion statuses ("Not Played", "Beaten", ...), in list order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompletionStatus {
    pub id: String,
    pub name: String,
    /// How many library entries have it.
    pub game_count: u32,
}

/// The completion status list and its two automatic rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompletionSettings {
    /// In display (and sort) order.
    pub statuses: Vec<CompletionStatus>,
    /// Status given to games when they are added; `None` for none.
    pub new_game: Option<String>,
    /// Status set when a game is started for the first time, while it still has `new_game`;
    /// `None` to leave it alone.
    pub first_start: Option<String>,
}

/// A value of a list field with the number of library entries that have it (for the value list
/// of a filter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ValueCount {
    /// What a filter stores (the normalised key, a store id, a category or status id).
    pub key: String,
    pub name: String,
    pub count: u32,
}
