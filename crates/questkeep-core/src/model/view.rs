use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::GameQuery;

/// How a view shows its games.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ViewLayout {
    #[default]
    Grid,
    List,
    Details,
}

/// How a view splits its games into groups. A game with several categories appears in each of
/// their groups.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GroupBy {
    #[default]
    None,
    Category,
    Source,
    CompletionStatus,
    Genre,
    Tag,
    Feature,
    Series,
    Platform,
    AgeRating,
    Developer,
    Publisher,
    GameMode,
    Theme,
    Perspective,
    ReleaseYear,
    /// Playtime ranges.
    Playtime,
    /// Time since last played, in buckets.
    LastPlayed,
    /// Install size ranges.
    InstallSize,
    /// Critic score ranges.
    CriticScore,
}

/// A column of the table layout. The UI owns the list of columns; an id it no longer knows is
/// skipped there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewColumn {
    pub id: String,
    /// Width in px set by dragging the column edge; `None` is the column's default width.
    #[serde(default)]
    pub width: Option<u32>,
}

/// A saved view of the library: filters, sort order and layout under a name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryView {
    pub id: String,
    pub name: String,
    /// Name of an icon from the UI's fixed icon set.
    pub icon: String,
    /// Filters and sort order. `search` is always `None`: the keyword filter is never saved.
    pub query: GameQuery,
    pub layout: ViewLayout,
    pub group: GroupBy,
    /// Table columns after Name, in order; empty is the default columns.
    pub columns: Vec<ViewColumn>,
}

/// A named group of views in the sidebar, in display order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewSection {
    pub id: String,
    pub name: String,
    pub views: Vec<LibraryView>,
}

/// A view to create at the end of a section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewView {
    pub section_id: String,
    pub name: String,
    pub icon: String,
    pub query: GameQuery,
    pub layout: ViewLayout,
    #[serde(default)]
    pub group: GroupBy,
    #[serde(default)]
    pub columns: Vec<ViewColumn>,
}

/// Fields of a view to change; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ViewUpdate {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub icon: Option<String>,
    #[ts(optional)]
    pub query: Option<GameQuery>,
    #[ts(optional)]
    pub layout: Option<ViewLayout>,
    #[ts(optional)]
    pub group: Option<GroupBy>,
    #[ts(optional)]
    pub columns: Option<Vec<ViewColumn>>,
}

/// The order of one section and its views, as the sidebar shows them after a drag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SectionOrder {
    pub id: String,
    pub views: Vec<String>,
}

/// What the library remembers about a view between restarts, apart from the view itself: the
/// details panel, the last selected game and the collapsed groups. Missing fields take their
/// defaults, so older stored states keep loading.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ViewUiState {
    /// The details panel floats over the list instead of sitting beside it (grid and table).
    pub panel_floating: bool,
    /// Width of the details panel in pixels (grid and table); `None` is the default width.
    pub panel_width: Option<u32>,
    /// Width of the list in the details layout in pixels; `None` is the default width.
    pub list_width: Option<u32>,
    /// The library entry selected last.
    pub selected: Option<String>,
    /// Collapsed group keys per grouping field (`"source"`, `"category"`, ...).
    pub collapsed: BTreeMap<String, Vec<String>>,
}
