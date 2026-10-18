use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Field used to order the library.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SortField {
    #[default]
    Name,
    LastPlayed,
    Playtime,
    DateAdded,
}

/// Which games to show with respect to the hidden flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HiddenFilter {
    /// Only games that are not hidden.
    #[default]
    Visible,
    /// Only hidden games.
    Hidden,
    /// Everything.
    All,
}

/// Filter and sort options for listing games.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GameQuery {
    /// Case-insensitive substring match on the name.
    pub search: Option<String>,
    pub sort: SortField,
    pub descending: bool,
    /// Provider ids to include; empty means all.
    pub sources: Vec<String>,
    pub favorites_only: bool,
    pub hidden: HiddenFilter,
    pub installed_only: bool,
}
