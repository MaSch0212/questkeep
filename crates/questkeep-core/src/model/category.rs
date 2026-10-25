use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `GameQuery::categories` entry that matches games without any category.
pub const NO_CATEGORY: &str = ":none";

/// A user category (a tag): games and categories are n:m.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Category {
    pub id: String,
    pub name: String,
    /// The store whose import created it (`steam`, ...); `None` when the user created it.
    pub source: Option<String>,
    /// How many games have it.
    pub game_count: u32,
}
