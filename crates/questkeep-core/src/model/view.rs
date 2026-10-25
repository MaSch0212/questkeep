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

/// A saved view of the library: filters, sort order and layout under a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
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
}

/// A named group of views in the sidebar, in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewSection {
    pub id: String,
    pub name: String,
    pub views: Vec<LibraryView>,
}

/// A view to create at the end of a section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewView {
    pub section_id: String,
    pub name: String,
    pub icon: String,
    pub query: GameQuery,
    pub layout: ViewLayout,
}

/// Fields of a view to change; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
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
}

/// The order of one section and its views, as the sidebar shows them after a drag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SectionOrder {
    pub id: String,
    pub views: Vec<String>,
}
