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
    ReleaseDate,
    PlayCount,
    CriticScore,
    CommunityScore,
    UserScore,
    TimeToBeat,
    InstallSize,
    CompletionStatus,
    Developer,
    Publisher,
    Series,
}

/// A field filtered by a list of values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ListField {
    /// Values are provider ids; a combined game matches with any of its stores.
    Source,
    /// Values are category ids.
    Category,
    /// Values are completion status ids.
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
}

impl ListField {
    /// The metadata field behind a value list field (`None` for store, category and status).
    pub fn metadata_field(self) -> Option<super::MetadataField> {
        use super::MetadataField as F;
        Some(match self {
            Self::Source | Self::Category | Self::CompletionStatus => return None,
            Self::Genre => F::Genres,
            Self::Tag => F::Tags,
            Self::Feature => F::Features,
            Self::Series => F::Series,
            Self::Platform => F::Platforms,
            Self::AgeRating => F::AgeRatings,
            Self::Developer => F::Developers,
            Self::Publisher => F::Publishers,
            Self::GameMode => F::GameModes,
            Self::Theme => F::Themes,
            Self::Perspective => F::Perspectives,
        })
    }
}

/// How a list filter's values combine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ListMode {
    /// The game has at least one of the values.
    #[default]
    Any,
    /// The game has every value.
    All,
    /// The game has none of the values.
    None,
}

/// `ListFilter::values` entry that stands for "the game has no value at all".
pub const NO_VALUE: &str = ":none";

/// Games by the values of a list field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListFilter {
    pub field: ListField,
    #[serde(default)]
    pub mode: ListMode,
    /// Value keys (see [`super::ValueCount::key`]), or [`NO_VALUE`].
    #[serde(default)]
    pub values: Vec<String>,
    /// Games that do not match instead.
    #[serde(default, skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub negate: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A field filtered by its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TextField {
    Name,
    SortingName,
    Description,
    Notes,
    InstallDir,
}

/// How a text filter compares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TextOp {
    #[default]
    Contains,
    StartsWith,
    EndsWith,
    /// The whole text.
    Equals,
    /// A regular expression (Rust `regex` syntax); one that does not compile matches nothing.
    Matches,
}

/// Games by a text field. Games without the text (no notes, ...) never match, unless negated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TextFilter {
    pub field: TextField,
    #[serde(default)]
    pub op: TextOp,
    /// Empty filters nothing.
    #[serde(default)]
    pub value: String,
    /// Case-insensitive unless set.
    #[serde(default, skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub case_sensitive: bool,
    /// Games that do not match instead.
    #[serde(default, skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub negate: bool,
}

/// A field filtered by a range of numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RangeField {
    ReleaseYear,
    /// Seconds.
    Playtime,
    PlayCount,
    /// Unix time in seconds.
    LastPlayed,
    /// Unix time in seconds.
    DateAdded,
    CriticScore,
    CommunityScore,
    UserScore,
    /// Main story, seconds.
    TimeToBeat,
    /// Bytes.
    InstallSize,
}

/// Games whose value lies in `[min, max]`; a missing bound is open. Games without a value never
/// match, except that a playtime or play count of 0 is a value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RangeFilter {
    pub field: RangeField,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// Games that do not match instead (games without a value included).
    #[serde(default, skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub negate: bool,
}

/// Whether a group needs all or any of its items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MatchMode {
    #[default]
    All,
    Any,
}

/// Filters combined with "and" (all) or "or" (any). Groups nest.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct FilterGroup {
    pub mode: MatchMode,
    pub items: Vec<FilterNode>,
    /// Games that do not match the group instead.
    #[serde(skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub negate: bool,
}

/// One entry of a [`FilterGroup`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum FilterNode {
    Group(FilterGroup),
    List(ListFilter),
    Range(RangeFilter),
    Text(TextFilter),
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

/// Which entries to show by type. Games is the default, so views that predate apps hide them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum KindFilter {
    #[default]
    Games,
    Apps,
    Both,
}

/// Filter and sort options for listing games.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
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
    pub kind: KindFilter,
    /// Category ids; a game matches when it has any of them ([`NO_CATEGORY`](super::NO_CATEGORY)
    /// matches games without one). Empty means all.
    pub categories: Vec<String>,
    /// Further filters, combined with "and" with the ones above.
    pub filter: FilterGroup,
}
