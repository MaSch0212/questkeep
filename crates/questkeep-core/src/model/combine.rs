use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a game combined from several library entries keeps (the combine screen's choices).
/// Entry ids refer to `entries`; store copies of every entry are always kept, and playtime and
/// play history add up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CombinePlan {
    /// Every entry to combine, the base included.
    pub entries: Vec<String>,
    /// The entry the others join; its copies come first.
    pub base: String,
    pub name_from: String,
    /// Cover, background and icon.
    pub images_from: String,
    /// The user's edited details (the Edit dialog's fields).
    pub details_from: String,
    pub status_from: String,
    pub score_from: String,
    pub favorite: bool,
    /// Whose chosen default launch profile is kept.
    pub profile_from: String,
    /// Entries whose categories are kept (all of them by default).
    pub categories_from: Vec<String>,
    /// Entries whose notes are kept, joined with a blank line in this order.
    pub notes_from: Vec<String>,
    /// Entries whose own links are kept.
    pub links_from: Vec<String>,
    /// Entries whose own launch profiles are kept.
    pub profiles_from: Vec<String>,
}
