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

/// A yes/no filter that can also be off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TriState {
    #[default]
    Any,
    Yes,
    No,
}

impl TriState {
    /// Whether a value passes the filter.
    pub fn accepts(self, value: bool) -> bool {
        match self {
            TriState::Any => true,
            TriState::Yes => value,
            TriState::No => !value,
        }
    }
}

/// Which games to show by whether they were ever played (playtime or a play count).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PlayedFilter {
    #[default]
    Any,
    Played,
    Never,
}

/// Filter and sort options for listing games.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GameQuery {
    /// Case-insensitive substring match on the name. Never saved in a view.
    pub search: Option<String>,
    pub sort: SortField,
    pub descending: bool,
    /// Provider ids to include; empty means all.
    pub sources: Vec<String>,
    pub favorite: TriState,
    pub hidden: HiddenFilter,
    pub installed: TriState,
    pub played: PlayedFilter,
    /// Category ids; a game matches when it has any of them ([`NO_CATEGORY`](super::NO_CATEGORY)
    /// matches games without one). Empty means all.
    pub categories: Vec<String>,
}
