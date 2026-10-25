use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `GameQuery::categories` entry that matches games without any category.
pub const NO_CATEGORY: &str = ":none";

/// A user category (a tag): games and categories are n:m. Stores' own categories (Steam
/// collections, GOG tags, ...) reach it through its `sources`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Category {
    pub id: String,
    pub name: String,
    /// How many games have it.
    pub game_count: u32,
    /// The store categories mapped to it, ordered by store and name.
    pub sources: Vec<CategorySource>,
}

/// A store's own category, by the name the store uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CategorySource {
    /// Provider id (`steam`, ...).
    pub source: String,
    pub name: String,
}
