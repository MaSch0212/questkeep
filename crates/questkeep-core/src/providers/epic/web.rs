//! Parsers for Epic's library, catalog and playtime services (all need a signed-in account).
//! They are pure functions over response bodies, so they are tested with fixtures.
//!
//! The filtering rules for what counts as a game follow Playnite's Epic library plugin.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use super::super::ProviderError;
use crate::model::GameMetadata;

/// One entry of the account's library (an owned asset).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRecord {
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub catalog_item_id: String,
    /// The id the Epic launcher uses; Questkeep's `source_game_id`.
    #[serde(default)]
    pub app_name: String,
    #[serde(default)]
    pub sandbox_type: Option<String>,
}

impl LibraryRecord {
    /// Records that can be a game: not the Unreal Engine namespace, not a private sandbox, and
    /// with every id present.
    pub fn is_candidate(&self) -> bool {
        self.namespace != "ue"
            && !self.namespace.is_empty()
            && !self.app_name.trim().is_empty()
            && !self.catalog_item_id.is_empty()
            && !self
                .sandbox_type
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("PRIVATE"))
    }
}

/// A library page: records and the cursor of the next page (`None` on the last page).
pub fn parse_library_page(
    body: &str,
) -> Result<(Vec<LibraryRecord>, Option<String>), ProviderError> {
    let unexpected =
        || ProviderError::Parse("Epic returned an unexpected library response.".into());
    let json: Value = serde_json::from_str(body).map_err(|_| unexpected())?;
    let records = json
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(unexpected)?
        .iter()
        .filter_map(|r| serde_json::from_value::<LibraryRecord>(r.clone()).ok())
        .collect();
    let cursor = json
        .get("responseMetadata")
        .and_then(|m| m.get("nextCursor"))
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    Ok((records, cursor))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct KeyImage {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct Category {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseInfo {
    #[serde(default)]
    date_added: Option<String>,
}

/// A catalog item: the store listing of an asset.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    long_description: Option<String>,
    #[serde(default)]
    key_images: Option<Vec<KeyImage>>,
    #[serde(default)]
    categories: Option<Vec<Category>>,
    #[serde(default)]
    main_game_item: Option<Value>,
    #[serde(default)]
    developer: Option<String>,
    #[serde(default)]
    creation_date: Option<String>,
    #[serde(default)]
    release_info: Option<Vec<ReleaseInfo>>,
    #[serde(default)]
    custom_attributes: Option<HashMap<String, Value>>,
}

/// The bulk response: a JSON object keyed by catalog item id. Unreadable entries are skipped.
pub fn parse_catalog(body: &str) -> Result<HashMap<String, CatalogItem>, ProviderError> {
    let json: Value = serde_json::from_str(body).map_err(|_| {
        ProviderError::Parse("Epic returned an unexpected catalog response.".into())
    })?;
    let object = json.as_object().ok_or_else(|| {
        ProviderError::Parse("Epic returned an unexpected catalog response.".into())
    })?;
    Ok(object
        .iter()
        .filter_map(|(id, value)| {
            let item: CatalogItem = serde_json::from_value(value.clone()).ok()?;
            Some((id.clone(), item))
        })
        .collect())
}

impl CatalogItem {
    fn has_category(&self, path: &str) -> bool {
        self.categories
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|c| c.path == path)
    }

    /// Whether this is a game: an `applications` item that is not a DLC (unless launchable), a
    /// digital extra or an Unreal Engine plugin.
    pub fn is_game(&self) -> bool {
        if self.title.trim().is_empty() || !self.has_category("applications") {
            return false;
        }
        if self.main_game_item.as_ref().is_some_and(|m| !m.is_null())
            && !self.has_category("addons/launchable")
        {
            return false;
        }
        !["digitalextras", "plugins", "plugins/engine"]
            .iter()
            .any(|c| self.has_category(c))
    }

    fn image_url(&self, kinds: &[&str]) -> Vec<String> {
        let images = self.key_images.as_deref().unwrap_or_default();
        let mut out: Vec<String> = Vec::new();
        for kind in kinds {
            for image in images.iter().filter(|i| i.kind == *kind) {
                let url = image.url.trim();
                if (url.starts_with("https://") || url.starts_with("http://"))
                    && !out.iter().any(|u| u == url)
                {
                    out.push(url.to_string());
                }
            }
        }
        out
    }

    /// Portrait box art, best first.
    pub fn cover_urls(&self) -> Vec<String> {
        self.image_url(&["DieselGameBoxTall", "OfferImageTall"])
    }

    /// Wide background images, best first.
    pub fn background_urls(&self) -> Vec<String> {
        self.image_url(&["DieselGameBox", "OfferImageWide", "DieselStoreFrontWide"])
    }

    fn attribute(&self, key: &str) -> Option<String> {
        self.custom_attributes
            .as_ref()?
            .get(key)?
            .get("value")?
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    pub fn metadata(&self) -> GameMetadata {
        let text = |s: &Option<String>| {
            s.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let mut developers: Vec<String> = Vec::new();
        for name in [text(&self.developer), self.attribute("developerName")]
            .into_iter()
            .flatten()
        {
            if !developers.contains(&name) {
                developers.push(name);
            }
        }
        let release_date = self
            .release_info
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r.date_added.as_deref())
            .chain(self.creation_date.as_deref())
            .find_map(normalize_date);
        GameMetadata {
            description: text(&self.description).or_else(|| text(&self.long_description)),
            release_date,
            developers,
            publishers: self.attribute("publisherName").into_iter().collect(),
            genres: Vec::new(),
        }
    }
}

/// `2019-05-14T13:00:00.000Z` -> `2019-05-14`.
fn normalize_date(value: &str) -> Option<String> {
    let date = value.trim().get(..10)?;
    let ok = date.char_indices().all(|(i, c)| match i {
        4 | 7 => c == '-',
        _ => c.is_ascii_digit(),
    });
    ok.then(|| date.to_string())
}

/// Playtime per `artifactId` (the appName), in seconds.
pub fn parse_playtime(body: &str) -> Result<HashMap<String, u64>, ProviderError> {
    let json: Value = serde_json::from_str(body).map_err(|_| {
        ProviderError::Parse("Epic returned an unexpected playtime response.".into())
    })?;
    let entries = json.as_array().ok_or_else(|| {
        ProviderError::Parse("Epic returned an unexpected playtime response.".into())
    })?;
    let mut out: HashMap<String, u64> = HashMap::new();
    for entry in entries {
        let (Some(id), Some(seconds)) = (
            entry.get("artifactId").and_then(Value::as_str),
            entry.get("totalTime").and_then(Value::as_u64),
        ) else {
            continue;
        };
        *out.entry(id.to_string()).or_insert(0) += seconds;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARY_PAGE_1: &str = include_str!("../../../tests/fixtures/epic/library_page1.json");
    const LIBRARY_PAGE_2: &str = include_str!("../../../tests/fixtures/epic/library_page2.json");
    const CATALOG: &str = include_str!("../../../tests/fixtures/epic/catalog_bulk.json");
    const PLAYTIME: &str = include_str!("../../../tests/fixtures/epic/playtime.json");

    #[test]
    fn library_pages_and_cursor() {
        let (records, cursor) = parse_library_page(LIBRARY_PAGE_1).unwrap();
        assert_eq!(records.len(), 5);
        assert_eq!(cursor.as_deref(), Some("cursor-2"));
        assert_eq!(records[0].app_name, "Sugar");
        let (records, cursor) = parse_library_page(LIBRARY_PAGE_2).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(cursor, None);
        assert!(parse_library_page("{}").is_err());
        assert!(parse_library_page("nope").is_err());
    }

    #[test]
    fn library_candidates_skip_engine_private_and_empty() {
        let (records, _) = parse_library_page(LIBRARY_PAGE_1).unwrap();
        let candidates: Vec<_> = records
            .iter()
            .filter(|r| r.is_candidate())
            .map(|r| r.app_name.as_str())
            .collect();
        assert_eq!(candidates, ["Sugar", "Dlc"]);
    }

    #[test]
    fn catalog_filtering_follows_playnite() {
        let items = parse_catalog(CATALOG).unwrap();
        let games: Vec<_> = {
            let mut g: Vec<_> = items
                .values()
                .filter(|i| i.is_game())
                .map(|i| i.title.as_str())
                .collect();
            g.sort_unstable();
            g
        };
        // DLC, digital extras, plugins and non-applications are dropped; the launchable add-on
        // stays.
        assert_eq!(games, ["Launchable Addon", "Sugar Game"]);
        assert!(!items["noapp"].is_game());
        assert!(parse_catalog("[]").is_err());
        assert!(parse_catalog("x").is_err());
    }

    #[test]
    fn key_images_map_to_covers_and_backgrounds() {
        let items = parse_catalog(CATALOG).unwrap();
        let game = &items["sugar-item"];
        assert_eq!(
            game.cover_urls(),
            [
                "https://cdn.example/sugar-tall.jpg",
                "https://cdn.example/sugar-offer-tall.jpg"
            ]
        );
        assert_eq!(
            game.background_urls(),
            [
                "https://cdn.example/sugar-wide.jpg",
                "https://cdn.example/sugar-offer-wide.jpg"
            ]
        );
        assert!(items["dlc-item"].cover_urls().is_empty());
    }

    #[test]
    fn metadata_from_catalog_item() {
        let items = parse_catalog(CATALOG).unwrap();
        let meta = items["sugar-item"].metadata();
        assert_eq!(meta.description.as_deref(), Some("A sweet game."));
        assert_eq!(meta.release_date.as_deref(), Some("2020-03-26"));
        assert_eq!(meta.developers, ["Sugar Studio"]);
        assert_eq!(meta.publishers, ["Sugar Publishing"]);
        let sparse = items["noapp"].metadata();
        assert_eq!(sparse.description.as_deref(), Some("Long text."));
        assert_eq!(sparse.release_date.as_deref(), Some("2019-01-01"));
        assert_eq!(CatalogItem::default().metadata(), GameMetadata::default());
    }

    #[test]
    fn playtime_sums_per_artifact() {
        let times = parse_playtime(PLAYTIME).unwrap();
        assert_eq!(times["Sugar"], 5_400 + 600);
        assert_eq!(times["Other"], 60);
        assert_eq!(times.len(), 2);
        assert!(parse_playtime("{}").is_err());
    }
}
