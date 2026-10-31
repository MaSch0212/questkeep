//! Metadata from several sources (stores, IGDB, ...) and how the library picks one value per
//! field: the first source in the user's priority order that has a value wins. Links are the
//! exception and are merged from all sources.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{GameLink, GameMetadata};

/// Source id of metadata from IGDB.
pub const IGDB_SOURCE: &str = "igdb";
/// Source id of time to beat from HowLongToBeat.
pub const HLTB_SOURCE: &str = "hltb";
/// Source id of values measured on this PC (install size on disk). Always first.
pub const MEASURED_SOURCE: &str = "measured";
/// Source id of values the user typed. Always first.
pub const USER_SOURCE: &str = "user";
/// Source id of the metadata stored before per-source storage existed. Always last.
pub const PREVIOUS_SOURCE: &str = "previous";

/// A metadata field that sources can provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MetadataField {
    Description,
    ReleaseDate,
    Developers,
    Publishers,
    Genres,
    Tags,
    Features,
    Series,
    AgeRatings,
    Platforms,
    GameModes,
    Themes,
    Perspectives,
    CriticScore,
    CommunityScore,
    InstallSize,
    TimeToBeat,
}

impl MetadataField {
    pub const ALL: [MetadataField; 17] = [
        Self::Description,
        Self::ReleaseDate,
        Self::Developers,
        Self::Publishers,
        Self::Genres,
        Self::Tags,
        Self::Features,
        Self::Series,
        Self::AgeRatings,
        Self::Platforms,
        Self::GameModes,
        Self::Themes,
        Self::Perspectives,
        Self::CriticScore,
        Self::CommunityScore,
        Self::InstallSize,
        Self::TimeToBeat,
    ];

    /// The list fields, which are stored as shared values and can be filtered and grouped by.
    pub const LISTS: [MetadataField; 11] = [
        Self::Developers,
        Self::Publishers,
        Self::Genres,
        Self::Tags,
        Self::Features,
        Self::Series,
        Self::AgeRatings,
        Self::Platforms,
        Self::GameModes,
        Self::Themes,
        Self::Perspectives,
    ];

    /// Stable key used in the database and settings (the serde name).
    pub fn key(self) -> &'static str {
        match self {
            Self::Description => "description",
            Self::ReleaseDate => "releaseDate",
            Self::Developers => "developers",
            Self::Publishers => "publishers",
            Self::Genres => "genres",
            Self::Tags => "tags",
            Self::Features => "features",
            Self::Series => "series",
            Self::AgeRatings => "ageRatings",
            Self::Platforms => "platforms",
            Self::GameModes => "gameModes",
            Self::Themes => "themes",
            Self::Perspectives => "perspectives",
            Self::CriticScore => "criticScore",
            Self::CommunityScore => "communityScore",
            Self::InstallSize => "installSize",
            Self::TimeToBeat => "timeToBeat",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.key() == key)
    }

    /// The field's value in `meta`, when it has one ("has a value": a non-empty list or text, or
    /// any number; a score of 0 counts).
    fn has_value(self, meta: &GameMetadata) -> bool {
        let text = |t: &Option<String>| t.as_deref().is_some_and(|t| !t.trim().is_empty());
        match self {
            Self::Description => text(&meta.description),
            Self::ReleaseDate => text(&meta.release_date),
            Self::CriticScore => meta.critic_score.is_some(),
            Self::CommunityScore => meta.community_score.is_some(),
            Self::InstallSize => meta.install_size.is_some(),
            Self::TimeToBeat => meta.time_to_beat.as_ref().is_some_and(|t| !t.is_empty()),
            list => !list_of(meta, list).is_empty(),
        }
    }

    /// Copies this field from `from` into `into`.
    fn copy(self, from: &GameMetadata, into: &mut GameMetadata) {
        match self {
            Self::Description => into.description = from.description.clone(),
            Self::ReleaseDate => into.release_date = from.release_date.clone(),
            Self::CriticScore => into.critic_score = from.critic_score.clone(),
            Self::CommunityScore => into.community_score = from.community_score.clone(),
            Self::InstallSize => into.install_size = from.install_size,
            Self::TimeToBeat => into.time_to_beat = from.time_to_beat.clone(),
            list => *list_of_mut(into, list) = list_of(from, list).to_vec(),
        }
    }
}

/// The values of a list field. Non-list fields have none.
pub fn list_of(meta: &GameMetadata, field: MetadataField) -> &[String] {
    match field {
        MetadataField::Developers => &meta.developers,
        MetadataField::Publishers => &meta.publishers,
        MetadataField::Genres => &meta.genres,
        MetadataField::Tags => &meta.tags,
        MetadataField::Features => &meta.features,
        MetadataField::Series => &meta.series,
        MetadataField::AgeRatings => &meta.age_ratings,
        MetadataField::Platforms => &meta.platforms,
        MetadataField::GameModes => &meta.game_modes,
        MetadataField::Themes => &meta.themes,
        MetadataField::Perspectives => &meta.perspectives,
        _ => &[],
    }
}

fn list_of_mut(meta: &mut GameMetadata, field: MetadataField) -> &mut Vec<String> {
    match field {
        MetadataField::Developers => &mut meta.developers,
        MetadataField::Publishers => &mut meta.publishers,
        MetadataField::Genres => &mut meta.genres,
        MetadataField::Tags => &mut meta.tags,
        MetadataField::Features => &mut meta.features,
        MetadataField::Series => &mut meta.series,
        MetadataField::AgeRatings => &mut meta.age_ratings,
        MetadataField::Platforms => &mut meta.platforms,
        MetadataField::GameModes => &mut meta.game_modes,
        MetadataField::Themes => &mut meta.themes,
        // Only called for list fields.
        _ => &mut meta.perspectives,
    }
}

/// The user's source order: one global list plus optional per-field lists. Empty lists mean
/// "the default order". Sources missing from a list (stores added later) go to its end in
/// default order; `user` and `measured` are always first, `previous` always last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct MetadataPriority {
    pub order: Vec<String>,
    /// Field key -> its own order. A field without an entry uses `order`.
    pub fields: BTreeMap<String, Vec<String>>,
}

/// Fields whose default order puts Steam before IGDB: Steam knows controller support, cloud
/// saves, Metacritic and review percentages that IGDB lacks.
const STEAM_FIRST: [MetadataField; 3] = [
    MetadataField::Features,
    MetadataField::CriticScore,
    MetadataField::CommunityScore,
];

/// The default order: IGDB, Steam, HowLongToBeat, then the other sources alphabetically.
/// `sources` are the movable sources (IGDB, HowLongToBeat and every store).
pub fn default_order(field: Option<MetadataField>, sources: &[String]) -> Vec<String> {
    let mut rest: Vec<String> = sources
        .iter()
        .filter(|s| !["igdb", "steam", HLTB_SOURCE].contains(&s.as_str()))
        .cloned()
        .collect();
    rest.sort();
    let head: &[&str] = if field.is_some_and(|f| STEAM_FIRST.contains(&f)) {
        &["steam", IGDB_SOURCE, HLTB_SOURCE]
    } else {
        &[IGDB_SOURCE, "steam", HLTB_SOURCE]
    };
    head.iter()
        .filter(|h| sources.iter().any(|s| s == *h))
        .map(|h| h.to_string())
        .chain(rest)
        .collect()
}

impl MetadataPriority {
    /// The complete order for `field` (or the global one): fixed sources around the user's
    /// order, completed with sources it doesn't mention.
    pub fn order_for(&self, field: Option<MetadataField>, sources: &[String]) -> Vec<String> {
        let own = field
            .and_then(|f| self.fields.get(f.key()))
            .filter(|o| !o.is_empty());
        let chosen = match own {
            Some(order) => order.clone(),
            // A field without its own order follows the global one, except that the Steam-first
            // fields keep their default while the global order is the default too.
            None if self.order.is_empty() => default_order(field, sources),
            None => self.order.clone(),
        };
        let mut seen = HashSet::new();
        let mut out = vec![USER_SOURCE.to_string(), MEASURED_SOURCE.to_string()];
        seen.extend(out.iter().cloned());
        let fixed = [USER_SOURCE, MEASURED_SOURCE, PREVIOUS_SOURCE];
        for source in chosen
            .into_iter()
            .chain(default_order(field, sources))
            .filter(|s| !fixed.contains(&s.as_str()))
        {
            if seen.insert(source.clone()) {
                out.push(source);
            }
        }
        out.push(PREVIOUS_SOURCE.to_string());
        out
    }
}

/// One source's data for a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    pub source: String,
    pub data: GameMetadata,
}

/// The winning value per field and where each came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub data: GameMetadata,
    /// Field key -> source id that provided it.
    pub sources: BTreeMap<String, String>,
}

/// Picks every field from the first source (in `priority` order) that has a value, and merges
/// links from all sources (in the global order, first one wins per URL). List values are
/// normalised (see [`normalize`]) and de-duplicated.
pub fn resolve(
    records: &[SourceRecord],
    priority: &MetadataPriority,
    sources: &[String],
) -> Resolved {
    let mut out = Resolved::default();
    let rank = |order: &[String], source: &str| {
        order
            .iter()
            .position(|s| s == source)
            .unwrap_or(order.len())
    };
    for field in MetadataField::ALL {
        let order = priority.order_for(Some(field), sources);
        let best = records
            .iter()
            .filter(|r| field.has_value(&r.data))
            .min_by_key(|r| rank(&order, &r.source));
        if let Some(record) = best {
            field.copy(&record.data, &mut out.data);
            out.sources
                .insert(field.key().to_string(), record.source.clone());
        }
    }
    for field in MetadataField::LISTS {
        let list = list_of_mut(&mut out.data, field);
        let mut seen = HashSet::new();
        *list = std::mem::take(list)
            .into_iter()
            .map(|v| normalize(field, &v))
            .filter(|v| !v.is_empty() && seen.insert(value_key(v)))
            .collect();
    }
    let order = priority.order_for(None, sources);
    let mut ordered: Vec<&SourceRecord> = records.iter().collect();
    ordered.sort_by_key(|r| rank(&order, &r.source));
    let mut urls = HashSet::new();
    out.data.links = ordered
        .iter()
        .flat_map(|r| r.data.links.iter())
        .filter(|l| !l.url.trim().is_empty() && urls.insert(url_key(&l.url)))
        .cloned()
        .collect::<Vec<GameLink>>();
    out
}

/// A URL compared without scheme, `www.` and trailing slash.
fn url_key(url: &str) -> String {
    let lower = url.trim().to_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    rest.strip_prefix("www.")
        .unwrap_or(rest)
        .trim_end_matches('/')
        .to_string()
}

/// The comparison key of a list value: lowercase letters and digits only, so "Co-op", "Co-Op"
/// and "co op" are one value.
pub fn value_key(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '+')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Known spellings mapped to one name, per field (compared by [`value_key`]).
const ALIASES: &[(MetadataField, &str, &str)] = &[
    (MetadataField::Features, "singleplayer", "Single-player"),
    (MetadataField::Features, "multiplayer", "Multiplayer"),
    (MetadataField::Features, "multiplayer", "Multi-player"),
    (MetadataField::Features, "coop", "Co-op"),
    (MetadataField::Features, "cooperative", "Co-op"),
    (MetadataField::Features, "onlinecoop", "Online Co-op"),
    (MetadataField::Features, "lancoop", "LAN Co-op"),
    (
        MetadataField::Features,
        "sharedsplitscreencoop",
        "Split-screen Co-op",
    ),
    (MetadataField::Features, "splitscreen", "Split-screen"),
    (MetadataField::Features, "onlinepvp", "Online PvP"),
    (MetadataField::Features, "pvp", "PvP"),
    (
        MetadataField::Features,
        "fullcontrollersupport",
        "Controller support",
    ),
    (
        MetadataField::Features,
        "controllersupport",
        "Controller support",
    ),
    (
        MetadataField::Features,
        "partialcontrollersupport",
        "Partial controller support",
    ),
    (MetadataField::Features, "steamcloud", "Cloud saves"),
    (MetadataField::Features, "cloudsaves", "Cloud saves"),
    (MetadataField::Features, "steamachievements", "Achievements"),
    (MetadataField::Features, "achievements", "Achievements"),
    (
        MetadataField::Features,
        "steamtradingcards",
        "Trading cards",
    ),
    (MetadataField::Features, "steamworkshop", "Workshop"),
    (MetadataField::Features, "vrsupport", "VR support"),
    (MetadataField::Features, "vronly", "VR only"),
    (MetadataField::Features, "battleroyale", "Battle Royale"),
    (
        MetadataField::Features,
        "massivelymultiplayeronlinemmo",
        "MMO",
    ),
    (MetadataField::GameModes, "singleplayer", "Single-player"),
    (MetadataField::GameModes, "multiplayer", "Multiplayer"),
    (MetadataField::GameModes, "cooperative", "Co-op"),
    (MetadataField::GameModes, "splitscreen", "Split-screen"),
    (
        MetadataField::GameModes,
        "massivelymultiplayeronlinemmo",
        "MMO",
    ),
    (MetadataField::Platforms, "win", "Windows"),
    (MetadataField::Platforms, "windows", "Windows"),
    (MetadataField::Platforms, "pcmicrosoftwindows", "Windows"),
    (MetadataField::Platforms, "mac", "macOS"),
    (MetadataField::Platforms, "osx", "macOS"),
    (MetadataField::Platforms, "macos", "macOS"),
    (MetadataField::Platforms, "linux", "Linux"),
    (MetadataField::Platforms, "steamos", "Linux"),
    (MetadataField::Tags, "coop", "Co-op"),
    (MetadataField::Tags, "singleplayer", "Singleplayer"),
    (MetadataField::Tags, "roguelite", "Roguelite"),
];

/// The canonical spelling of a list value: trimmed, an alias table for known variants, and the
/// Steam prefix dropped from Steam's feature names ("Steam Cloud" became "Cloud saves" above).
pub fn normalize(field: MetadataField, value: &str) -> String {
    let trimmed = value.trim();
    let key = value_key(trimmed);
    if let Some((_, _, name)) = ALIASES.iter().find(|(f, k, _)| *f == field && *k == key) {
        return (*name).to_string();
    }
    if field == MetadataField::Features
        && let Some(rest) = trimmed.strip_prefix("Steam ")
    {
        return rest.to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Score;

    fn sources() -> Vec<String> {
        ["epic", "gog", "igdb", "steam", "xbox", "hltb"]
            .map(String::from)
            .to_vec()
    }

    fn record(source: &str, data: GameMetadata) -> SourceRecord {
        SourceRecord {
            source: source.into(),
            data,
        }
    }

    #[test]
    fn default_order_is_igdb_steam_then_alphabetical_with_fixed_ends() {
        let order = MetadataPriority::default().order_for(Some(MetadataField::Genres), &sources());
        assert_eq!(
            order,
            [
                "user", "measured", "igdb", "steam", "hltb", "epic", "gog", "xbox", "previous"
            ]
        );
        let order =
            MetadataPriority::default().order_for(Some(MetadataField::Features), &sources());
        assert_eq!(&order[2..4], ["steam", "igdb"]);
    }

    #[test]
    fn custom_orders_are_completed_with_missing_sources() {
        let priority = MetadataPriority {
            order: vec!["gog".into(), "previous".into(), "steam".into()],
            fields: BTreeMap::from([("tags".into(), vec!["xbox".into()])]),
        };
        let global = priority.order_for(Some(MetadataField::Genres), &sources());
        assert_eq!(
            global,
            [
                "user", "measured", "gog", "steam", "igdb", "hltb", "epic", "xbox", "previous"
            ]
        );
        // With a custom global order, Steam-first fields follow it too.
        let features = priority.order_for(Some(MetadataField::Features), &sources());
        assert_eq!(&features[2..4], ["gog", "steam"]);
        let tags = priority.order_for(Some(MetadataField::Tags), &sources());
        assert_eq!(&tags[2..4], ["xbox", "igdb"]);
    }

    #[test]
    fn first_source_with_a_value_wins_per_field_and_links_merge() {
        let steam = GameMetadata {
            description: Some("Steam text".into()),
            genres: vec!["Action".into()],
            features: vec!["Steam Cloud".into(), "Full controller support".into()],
            critic_score: Some(Score {
                value: 0,
                source: "Metacritic".into(),
            }),
            links: vec![
                GameLink::new("Store", "https://store.steampowered.com/app/1/"),
                GameLink::new("Wiki", "https://example.org/wiki"),
            ],
            ..GameMetadata::default()
        };
        let igdb = GameMetadata {
            description: None,
            genres: vec!["Roguelike".into(), "roguelike".into()],
            features: vec!["Single player".into()],
            critic_score: Some(Score {
                value: 90,
                source: "IGDB".into(),
            }),
            links: vec![
                GameLink::new("Official", "http://www.example.com"),
                GameLink::new("Steam", "http://store.steampowered.com/app/1"),
            ],
            ..GameMetadata::default()
        };
        let previous = GameMetadata {
            description: Some("Old".into()),
            release_date: Some("2020".into()),
            ..GameMetadata::default()
        };
        let resolved = resolve(
            &[
                record("previous", previous),
                record("steam", steam),
                record("igdb", igdb),
            ],
            &MetadataPriority::default(),
            &sources(),
        );
        assert_eq!(resolved.data.description.as_deref(), Some("Steam text"));
        assert_eq!(resolved.data.release_date.as_deref(), Some("2020"));
        assert_eq!(
            resolved.data.genres,
            ["Roguelike"],
            "IGDB first, de-duplicated"
        );
        assert_eq!(
            resolved.data.features,
            ["Cloud saves", "Controller support"]
        );
        assert_eq!(
            resolved.data.critic_score.as_ref().map(|s| s.value),
            Some(0)
        );
        assert_eq!(resolved.sources["criticScore"], "steam");
        assert_eq!(resolved.sources["genres"], "igdb");
        assert_eq!(resolved.sources["releaseDate"], "previous");
        let urls: Vec<&str> = resolved
            .data
            .links
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        assert_eq!(urls, ["Official", "Steam", "Wiki"]);
    }

    #[test]
    fn normalizes_known_spellings() {
        assert_eq!(normalize(MetadataField::Features, " Co-Op "), "Co-op");
        assert_eq!(
            normalize(MetadataField::Features, "Single-player"),
            "Single-player"
        );
        assert_eq!(
            normalize(MetadataField::Features, "Singleplayer"),
            "Single-player"
        );
        assert_eq!(
            normalize(MetadataField::Features, "Steam Leaderboards"),
            "Leaderboards"
        );
        assert_eq!(
            normalize(MetadataField::Platforms, "PC (Microsoft Windows)"),
            "Windows"
        );
        assert_eq!(normalize(MetadataField::Genres, "Co-op"), "Co-op");
        assert_eq!(value_key("Co-Op"), value_key("co op"));
    }
}
