//! The public Microsoft Store display catalog: portrait covers, backgrounds and metadata for a
//! package family name. No account is needed.
//!
//! `GET https://displaycatalog.mp.microsoft.com/v7.0/products/lookup?alternateId=PackageFamilyName
//! &value=<pfn>&market=US&languages=en-us&fieldsTemplate=details` answers with
//! `Products[].LocalizedProperties[].Images[]` (each with `ImagePurpose`, `Uri`, `Width`,
//! `Height`), the title, developer, publisher and description, `MarketProperties[]
//! .OriginalReleaseDate` and `Properties.Category`. Unknown packages answer 200 with an empty
//! `Products` list. Results are cached in memory for the run (one request per game, shared by
//! covers, backgrounds and metadata).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Deserialize;

use super::super::{ImageCandidate, ProviderError};
use crate::auth::{Http, HttpRequest, url_encode};
use crate::model::GameMetadata;

const LOOKUP_URL: &str = "https://displaycatalog.mp.microsoft.com/v7.0/products/lookup";

/// A poster (2:3) is accepted as a cover when its aspect ratio (width / height) is within this
/// distance of 2/3.
const COVER_RATIO_TOLERANCE: f64 = 0.06;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct LookupResponse {
    #[serde(default)]
    products: Vec<RawProduct>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct RawProduct {
    #[serde(default)]
    product_id: Option<String>,
    #[serde(default)]
    localized_properties: Vec<RawLocalized>,
    #[serde(default)]
    market_properties: Vec<RawMarket>,
    #[serde(default)]
    properties: RawProperties,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct RawLocalized {
    #[serde(default)]
    product_title: Option<String>,
    #[serde(default)]
    developer_name: Option<String>,
    #[serde(default)]
    publisher_name: Option<String>,
    #[serde(default)]
    product_description: Option<String>,
    #[serde(default)]
    images: Vec<RawImage>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct RawMarket {
    #[serde(default)]
    original_release_date: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct RawProperties {
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    categories: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
struct RawImage {
    #[serde(default)]
    image_purpose: Option<String>,
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

/// One store image with its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogImage {
    pub purpose: String,
    pub url: String,
    pub width: u32,
    pub height: u32,
}

impl CatalogImage {
    fn is_portrait_cover(&self) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }
        let ratio = f64::from(self.width) / f64::from(self.height);
        (ratio - 2.0 / 3.0).abs() <= COVER_RATIO_TOLERANCE
    }
}

/// What the catalog knows about one game.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CatalogProduct {
    /// The Store id ("BigId"), e.g. `9PP5G1F0C2B6`.
    pub store_id: Option<String>,
    pub title: Option<String>,
    pub images: Vec<CatalogImage>,
    pub metadata: GameMetadata,
}

impl CatalogProduct {
    fn images_of<'a>(&'a self, purpose: &'a str) -> impl Iterator<Item = &'a CatalogImage> {
        self.images
            .iter()
            .filter(move |i| i.purpose.eq_ignore_ascii_case(purpose))
    }

    /// `Poster` (2:3) first, then a `BoxArt` that is close to 2:3 (usually it is square and is
    /// skipped). Empty when the store has no portrait art for the game.
    pub fn cover_urls(&self) -> Vec<String> {
        let posters = self
            .images_of("Poster")
            .chain(self.images_of("BoxArt"))
            .filter(|i| i.is_portrait_cover());
        let mut out: Vec<String> = Vec::new();
        for image in posters {
            if !out.contains(&image.url) {
                out.push(image.url.clone());
            }
        }
        out
    }

    /// `SuperHeroArt`, `TitledHeroArt`, then the first screenshot.
    pub fn background_urls(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let wide = ["SuperHeroArt", "TitledHeroArt"]
            .into_iter()
            .flat_map(|p| self.images_of(p))
            .chain(self.images_of("Screenshot").take(1));
        for image in wide {
            if !out.contains(&image.url) {
                out.push(image.url.clone());
            }
        }
        out
    }
}

/// `//host/path` becomes `https://host/path`; anything that is not https afterwards is dropped.
fn absolute_url(uri: &str) -> Option<String> {
    let uri = uri.trim();
    let url = if let Some(rest) = uri.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        uri.to_string()
    };
    url.starts_with("https://").then_some(url)
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `2021-11-15T18:00:00Z` becomes `2021-11-15`; the catalog's "unknown" placeholders (year 1601
/// and similar) are dropped.
fn release_date(raw: &str) -> Option<String> {
    let date = raw.trim().get(..10)?;
    let bytes = date.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && date
            .chars()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit());
    let year: u32 = date.get(..4)?.parse().ok()?;
    (shaped && year >= 1970).then(|| date.to_string())
}

fn push_unique(list: &mut Vec<String>, value: Option<String>) {
    if let Some(value) = value
        && !list.iter().any(|v| v.eq_ignore_ascii_case(&value))
    {
        list.push(value);
    }
}

/// The first product of a lookup response; `Ok(None)` when the catalog does not know the package.
pub fn parse_lookup(body: &str) -> Result<Option<CatalogProduct>, ProviderError> {
    let response: LookupResponse = serde_json::from_str(body).map_err(|_| {
        ProviderError::Parse("The Microsoft Store returned an unexpected response.".into())
    })?;
    let Some(raw) = response.products.into_iter().next() else {
        return Ok(None);
    };
    let localized = raw.localized_properties.first();
    let images = localized
        .map(|l| {
            l.images
                .iter()
                .filter_map(|i| {
                    Some(CatalogImage {
                        purpose: i.image_purpose.clone()?,
                        url: absolute_url(i.uri.as_deref()?)?,
                        width: i.width,
                        height: i.height,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut genres: Vec<String> = Vec::new();
    for category in raw.properties.categories.iter().flatten() {
        push_unique(
            &mut genres,
            Some(category.trim().to_string()).filter(|c| !c.is_empty()),
        );
    }
    push_unique(&mut genres, clean(&raw.properties.category));

    let mut developers = Vec::new();
    let mut publishers = Vec::new();
    push_unique(
        &mut developers,
        localized.and_then(|l| clean(&l.developer_name)),
    );
    push_unique(
        &mut publishers,
        localized.and_then(|l| clean(&l.publisher_name)),
    );

    Ok(Some(CatalogProduct {
        store_id: clean(&raw.product_id),
        title: localized.and_then(|l| clean(&l.product_title)),
        images,
        metadata: GameMetadata {
            description: localized.and_then(|l| clean(&l.product_description)),
            release_date: raw
                .market_properties
                .iter()
                .find_map(|m| m.original_release_date.as_deref().and_then(release_date)),
            developers,
            publishers,
            genres,
        },
    }))
}

/// A package family name is safe to put into a URL when it only holds these characters.
pub fn is_valid_pfn(pfn: &str) -> bool {
    !pfn.is_empty()
        && pfn.len() <= 128
        && pfn
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn lookup_url(pfn: &str) -> String {
    format!(
        "{LOOKUP_URL}?alternateId=PackageFamilyName&value={}&market=US&languages=en-us&fieldsTemplate=details",
        url_encode(pfn)
    )
}

/// Looks packages up in the catalog, caching answers (including "not in the catalog") for the
/// life of the value. Failures are not cached, so they are retried.
pub struct Catalog {
    http: Arc<dyn Http>,
    cache: Mutex<HashMap<String, Option<Arc<CatalogProduct>>>>,
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog").finish_non_exhaustive()
    }
}

impl Catalog {
    pub fn new(http: Arc<dyn Http>) -> Self {
        Self {
            http,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn product(&self, pfn: &str) -> Result<Option<Arc<CatalogProduct>>, ProviderError> {
        if !is_valid_pfn(pfn) {
            return Ok(None);
        }
        let key = pfn.to_ascii_lowercase();
        if let Some(hit) = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(hit.clone());
        }
        let response = self
            .http
            .send(&HttpRequest::get(lookup_url(pfn)).header("Accept", "application/json"))
            .map_err(ProviderError::Network)?;
        let product = match response.status {
            404 => None,
            _ if response.ok() => parse_lookup(&response.body)?.map(Arc::new),
            status => {
                return Err(ProviderError::Network(format!(
                    "The Microsoft Store returned HTTP {status}"
                )));
            }
        };
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, product.clone());
        Ok(product)
    }

    /// The Store ids currently in PC Game Pass (not cached: the list changes over time and is
    /// read once per owned scan).
    pub fn game_pass_pc_games(&self) -> Result<HashSet<String>, ProviderError> {
        super::game_pass::fetch_pc_games(self.http.as_ref())
    }

    /// Cover candidates; lookup failures only mean "none" here (logged).
    pub fn covers(&self, pfn: &str) -> Vec<ImageCandidate> {
        self.images(pfn, CatalogProduct::cover_urls)
    }

    pub fn backgrounds(&self, pfn: &str) -> Vec<ImageCandidate> {
        self.images(pfn, CatalogProduct::background_urls)
    }

    fn images(&self, pfn: &str, pick: fn(&CatalogProduct) -> Vec<String>) -> Vec<ImageCandidate> {
        match self.product(pfn) {
            Ok(Some(product)) => pick(&product)
                .into_iter()
                .map(ImageCandidate::Url)
                .collect(),
            Ok(None) => Vec::new(),
            Err(err) => {
                tracing::debug!(package = pfn, %err, "Xbox catalog lookup failed");
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::auth::testing::{FakeHttp, response};

    const FIXTURE: &str = include_str!("../../../tests/fixtures/xbox/catalog_product.json");
    const PFN: &str = "Microsoft.254428597CFE2_8wekyb3d8bbwe";

    #[test]
    fn parses_images_and_metadata() {
        let product = parse_lookup(FIXTURE).unwrap().unwrap();
        assert_eq!(product.store_id.as_deref(), Some("9PP5G1F0C2B6"));
        assert_eq!(product.title.as_deref(), Some("Halo Infinite"));
        let covers = product.cover_urls();
        assert_eq!(
            covers.len(),
            1,
            "the square BoxArt is not a cover: {covers:?}"
        );
        assert!(covers[0].starts_with("https://store-images.s-microsoft.com/image/apps.3823."));
        let backgrounds = product.background_urls();
        assert!(backgrounds[0].contains("apps.47030."), "SuperHeroArt first");
        assert!(
            backgrounds[1].contains("apps.20857."),
            "TitledHeroArt second"
        );
        assert_eq!(backgrounds.len(), 3, "plus one screenshot");
        let meta = &product.metadata;
        assert_eq!(meta.release_date.as_deref(), Some("2021-11-15"));
        assert_eq!(meta.developers, ["343 Industries"]);
        assert_eq!(meta.publishers, ["Xbox Game Studios"]);
        assert_eq!(meta.genres, ["Shooter"]);
        assert!(
            meta.description
                .as_deref()
                .is_some_and(|d| d.starts_with("Free-to-Play Multiplayer:"))
        );
    }

    #[test]
    fn near_portrait_box_art_is_accepted_and_odd_ratios_are_not() {
        let image = |purpose: &str, w, h| CatalogImage {
            purpose: purpose.into(),
            url: format!("https://x.test/{purpose}"),
            width: w,
            height: h,
        };
        let product = CatalogProduct {
            images: vec![
                image("BoxArt", 600, 900),
                image("Poster", 800, 800),
                image("BrandedKeyArt", 584, 800),
            ],
            ..CatalogProduct::default()
        };
        assert_eq!(product.cover_urls(), ["https://x.test/BoxArt"]);
        assert!(CatalogProduct::default().cover_urls().is_empty());
    }

    #[test]
    fn unknown_packages_and_bad_bodies() {
        assert_eq!(
            parse_lookup(r#"{"BigIds":[],"Products":[],"TotalResultCount":0}"#).unwrap(),
            None
        );
        assert!(parse_lookup("<html>").is_err());
        // Sparse products still parse; placeholder dates are dropped.
        let sparse = parse_lookup(
            r#"{"Products":[{"MarketProperties":[{"OriginalReleaseDate":"1601-01-01T00:00:00Z"}]}]}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(sparse.metadata, GameMetadata::default());
        assert!(sparse.images.is_empty());
    }

    #[test]
    fn release_dates() {
        assert_eq!(
            release_date("2021-11-15T18:00:00Z").as_deref(),
            Some("2021-11-15")
        );
        assert_eq!(release_date("1601-01-01T00:00:00Z"), None);
        assert_eq!(release_date("soon"), None);
        assert_eq!(release_date(""), None);
    }

    #[test]
    fn lookup_url_encodes_the_package_family_name() {
        assert_eq!(
            lookup_url(PFN),
            format!(
                "https://displaycatalog.mp.microsoft.com/v7.0/products/lookup?alternateId=PackageFamilyName&value={PFN}&market=US&languages=en-us&fieldsTemplate=details"
            )
        );
        assert!(is_valid_pfn(PFN));
        assert!(!is_valid_pfn("a&b=c"));
        assert!(!is_valid_pfn(""));
    }

    #[test]
    fn lookups_are_cached_and_failures_are_not() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let catalog = Catalog::new(Arc::new(FakeHttp::new(move |req| {
            let n = seen.fetch_add(1, Ordering::SeqCst);
            assert!(req.url.contains(PFN) || req.url.contains("Unknown.Game_abc"));
            if req.url.contains("Unknown.Game_abc") {
                response(200, r#"{"Products":[]}"#)
            } else if n == 0 {
                response(503, "")
            } else {
                response(200, FIXTURE)
            }
        })));
        assert!(matches!(
            catalog.product(PFN),
            Err(ProviderError::Network(_))
        ));
        assert!(catalog.product(PFN).unwrap().is_some());
        assert_eq!(catalog.covers(PFN).len(), 1);
        assert_eq!(catalog.backgrounds(&PFN.to_uppercase()).len(), 3);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "success is cached, any case"
        );
        assert!(catalog.product("Unknown.Game_abc").unwrap().is_none());
        assert!(catalog.covers("Unknown.Game_abc").is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 3, "'unknown' is cached too");
        // Invalid ids never reach the network.
        assert!(catalog.product("a b").unwrap().is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    /// One anonymous read-only request: `cargo test -p questkeep-core xbox::catalog -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn live_lookup() {
        let catalog = Catalog::new(Arc::new(crate::auth::UreqHttp::new()));
        let product = catalog.product(PFN).unwrap().unwrap();
        println!("{:?} {:?}", product.title, product.cover_urls());
        println!("{:?}", product.metadata);
        println!("{:?}", catalog.product("Nope.Nothing_abc123").unwrap());
    }
}
