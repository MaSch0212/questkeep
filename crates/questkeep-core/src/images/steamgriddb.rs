//! SteamGridDB client (<https://www.steamgriddb.com/api/v2>), used as a fallback artwork source
//! for games whose store has no (good) cover or background.
//!
//! The API key is user-provided, shared behind a lock so changing it in settings takes effect for
//! the running workers, and is only ever sent as a `Authorization: Bearer` header to the API host
//! (never logged, never sent to the image CDN). All API calls go through one global limiter
//! (about two requests per second) no matter how many worker threads call in.
//!
//! Parsing is separate from fetching so it can be tested against fixtures.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::ImageKind;
use super::fetcher::{download, http_agent};
use crate::error::{CoreError, Result};

const API_BASE: &str = "https://www.steamgriddb.com/api/v2";
/// Minimum time between two API requests, across all threads.
const MIN_INTERVAL: Duration = Duration::from_millis(500);
/// Cover sizes SteamGridDB serves that are 2:3 portraits.
const COVER_SIZES: [(u32, u32); 3] = [(600, 900), (342, 482), (660, 930)];
/// Filters sent with every image query.
const SAFE_FILTERS: &str = "types=static&nsfw=false&humor=false&epilepsy=false";
/// How many of the best-ranked images to try downloading before giving up.
const DOWNLOAD_TRIES: usize = 3;

const KEY_REJECTED: &str = "The SteamGridDB API key was rejected.";

/// `games.extra` key recording that SteamGridDB was asked for this kind of image and had nothing,
/// so it isn't asked again on every start. Value: RFC 3339 timestamp.
pub(crate) fn checked_key(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::Cover => "steamGridDbCoverCheckedAt",
        ImageKind::Background => "steamGridDbBackgroundCheckedAt",
        // Icons are never looked up there; the key only keeps the match exhaustive.
        ImageKind::Icon => "steamGridDbIconCheckedAt",
    }
}

/// Identifies a game for lookup.
#[derive(Debug, Clone, Copy)]
pub struct Lookup<'a> {
    pub name: &'a str,
    /// Provider id (`steam`, `epic`, ...).
    pub source: &'a str,
    pub source_game_id: Option<&'a str>,
}

/// Shared handle to the SteamGridDB API. Cheap to clone.
#[derive(Clone)]
pub struct SteamGridDb {
    inner: Arc<Inner>,
}

struct Inner {
    http: ureq::Agent,
    key: RwLock<Option<String>>,
    /// Set when the API answered 401/403, so a bad key costs one request instead of one per
    /// queued game. Cleared when the key changes.
    rejected: AtomicBool,
    /// Earliest instant the next request may start.
    next_slot: Mutex<Instant>,
    interval: Duration,
}

impl std::fmt::Debug for SteamGridDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the key.
        f.debug_struct("SteamGridDb")
            .field("configured", &self.is_configured())
            .finish()
    }
}

impl SteamGridDb {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_interval(api_key, MIN_INTERVAL)
    }

    fn with_interval(api_key: Option<String>, interval: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                http: http_agent(),
                key: RwLock::new(normalize_key(api_key)),
                rejected: AtomicBool::new(false),
                next_slot: Mutex::new(Instant::now()),
                interval,
            }),
        }
    }

    /// Replaces the key (called when settings are saved). Blank means "none".
    pub fn set_api_key(&self, api_key: Option<String>) {
        *self
            .inner
            .key
            .write()
            .unwrap_or_else(PoisonError::into_inner) = normalize_key(api_key);
        self.inner.rejected.store(false, Ordering::Relaxed);
    }

    /// Whether a key is set and has not been rejected by the API since it was entered.
    pub fn is_configured(&self) -> bool {
        self.key().is_some() && !self.inner.rejected.load(Ordering::Relaxed)
    }

    fn key(&self) -> Option<String> {
        self.inner
            .key
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Blocks until this thread owns the next request slot.
    fn throttle(&self) {
        let wait = {
            let mut next = self
                .inner
                .next_slot
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let now = Instant::now();
            let start = (*next).max(now);
            *next = start + self.inner.interval;
            start - now
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    /// GET `path` (relative to the API root). `Ok(None)` on 404.
    fn get(&self, path: &str) -> Result<Option<String>> {
        let key = self.key().ok_or_else(|| {
            CoreError::Invalid("Add a SteamGridDB API key in the settings first.".into())
        })?;
        if self.inner.rejected.load(Ordering::Relaxed) {
            return Err(CoreError::Invalid(KEY_REJECTED.into()));
        }
        self.throttle();
        let result = self
            .inner
            .http
            .get(format!("{API_BASE}{path}"))
            .header("Authorization", format!("Bearer {key}"))
            .call();
        match result {
            Ok(response) => response
                .into_body()
                .read_to_string()
                .map(Some)
                .map_err(|err| CoreError::Network(format!("SteamGridDB: {err}"))),
            Err(ureq::Error::StatusCode(404)) => Ok(None),
            Err(ureq::Error::StatusCode(401 | 403)) => {
                self.inner.rejected.store(true, Ordering::Relaxed);
                Err(CoreError::Invalid(KEY_REJECTED.into()))
            }
            Err(ureq::Error::StatusCode(429)) => Err(CoreError::Network(
                "SteamGridDB rate limit reached, try again later.".into(),
            )),
            Err(err) => Err(CoreError::Network(format!("SteamGridDB: {err}"))),
        }
    }

    /// Finds the SteamGridDB game id: by platform id when the store is known to the API,
    /// otherwise (or when that finds nothing) by name search.
    pub fn resolve_game(&self, lookup: &Lookup<'_>) -> Result<Option<u64>> {
        if let (Some(platform), Some(id)) = (platform_name(lookup.source), lookup.source_game_id)
            && !id.is_empty()
            && let Some(body) = self.get(&format!("/games/{platform}/{}", encode(id)))?
            && let Some(found) = parse_game_id(&body)?
        {
            return Ok(Some(found));
        }
        if lookup.name.trim().is_empty() {
            return Ok(None);
        }
        let Some(body) = self.get(&format!("/search/autocomplete/{}", encode(lookup.name)))? else {
            return Ok(None);
        };
        Ok(best_match(&parse_search(&body)?, lookup.name))
    }

    /// Download URLs for a game's artwork, best first.
    pub fn image_urls(&self, game_id: u64, kind: ImageKind) -> Result<Vec<String>> {
        let (path, sizes): (String, Option<&[(u32, u32)]>) = match kind {
            ImageKind::Cover => (
                format!("/grids/game/{game_id}?dimensions=600x900,342x482,660x930&{SAFE_FILTERS}"),
                Some(&COVER_SIZES),
            ),
            ImageKind::Background => (format!("/heroes/game/{game_id}?{SAFE_FILTERS}"), None),
            // Tray icons come from local sources and the stores, never from SteamGridDB.
            ImageKind::Icon => return Ok(Vec::new()),
        };
        let Some(body) = self.get(&path)? else {
            return Ok(Vec::new());
        };
        Ok(rank_images(parse_images(&body)?, sizes))
    }

    /// Looks the game up and downloads its best cover or background. `Ok(None)` means
    /// SteamGridDB has nothing suitable; errors are transient or key problems and should not be
    /// remembered as "checked".
    pub fn fetch_image(
        &self,
        http: &ureq::Agent,
        lookup: &Lookup<'_>,
        kind: ImageKind,
    ) -> Result<Option<(&'static str, Vec<u8>)>> {
        let Some(game_id) = self.resolve_game(lookup)? else {
            return Ok(None);
        };
        let urls = self.image_urls(game_id, kind)?;
        Ok(urls
            .iter()
            .take(DOWNLOAD_TRIES)
            .find_map(|url| download(http, url)))
    }
}

fn normalize_key(key: Option<String>) -> Option<String> {
    key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// SteamGridDB's platform name for `GET /games/{platform}/{id}`, when our provider maps to one.
/// The API documents `steam`, `origin`, `egs`, `bnet`, `uplay`, `flashpoint` and `eshop`; GOG and
/// Xbox are not among them, so those use name search. Epic ids are best effort: a miss falls
/// back to name search too.
pub(crate) fn platform_name(source: &str) -> Option<&'static str> {
    match source {
        "steam" => Some("steam"),
        "epic" => Some("egs"),
        _ => None,
    }
}

/// Percent-encodes everything except RFC 3986 unreserved characters (safe for a path segment).
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    success: bool,
    data: Option<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct SearchHit {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Image {
    pub url: String,
    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub upvotes: i64,
    #[serde(default)]
    pub downvotes: i64,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub humor: bool,
    #[serde(default)]
    pub epilepsy: bool,
}

fn parse_envelope<T: for<'de> Deserialize<'de>>(body: &str) -> Result<Option<T>> {
    let envelope: Envelope<T> = serde_json::from_str(body).map_err(|err| {
        CoreError::Network(format!("SteamGridDB sent an unexpected reply: {err}"))
    })?;
    Ok(if envelope.success {
        envelope.data
    } else {
        None
    })
}

pub(crate) fn parse_search(body: &str) -> Result<Vec<SearchHit>> {
    Ok(parse_envelope::<Vec<SearchHit>>(body)?.unwrap_or_default())
}

pub(crate) fn parse_game_id(body: &str) -> Result<Option<u64>> {
    Ok(parse_envelope::<SearchHit>(body)?.map(|hit| hit.id))
}

pub(crate) fn parse_images(body: &str) -> Result<Vec<Image>> {
    Ok(parse_envelope::<Vec<Image>>(body)?.unwrap_or_default())
}

/// Exact case-insensitive name match first, else the first result.
pub(crate) fn best_match(hits: &[SearchHit], name: &str) -> Option<u64> {
    let wanted = name.trim().to_lowercase();
    hits.iter()
        .find(|h| h.name.trim().to_lowercase() == wanted)
        .or_else(|| hits.first())
        .map(|h| h.id)
}

/// Drops unsafe or wrongly sized images and orders the rest by net votes, then score. The sort
/// is stable, so ties keep the API's own order.
pub(crate) fn rank_images(mut images: Vec<Image>, sizes: Option<&[(u32, u32)]>) -> Vec<String> {
    images.retain(|i| {
        !(i.nsfw || i.humor || i.epilepsy)
            && !i.url.is_empty()
            && sizes.is_none_or(|sizes| {
                // Missing dimensions: trust the query's `dimensions` filter.
                (i.width == 0 && i.height == 0) || sizes.contains(&(i.width, i.height))
            })
    });
    images.sort_by_key(|i| std::cmp::Reverse((i.upvotes - i.downvotes, i.score)));
    images.into_iter().map(|i| i.url).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = r#"{"success":true,"data":[
        {"id":5,"name":"Half-Life 2: Episode One","types":["steam"],"verified":true},
        {"id":9,"name":"half-life 2","types":["steam"],"verified":true}]}"#;
    const GAME: &str =
        r#"{"success":true,"data":{"id":5247,"name":"Portal","types":["steam"],"verified":true}}"#;
    const GRIDS: &str = r#"{"page":0,"total":4,"limit":50,"success":true,"data":[
        {"id":1,"score":0,"style":"alternate","width":600,"height":900,"nsfw":false,"humor":false,"epilepsy":false,"upvotes":1,"downvotes":0,"url":"https://cdn2.steamgriddb.com/grid/a.png"},
        {"id":2,"score":0,"style":"alternate","width":600,"height":900,"nsfw":true,"humor":false,"epilepsy":false,"upvotes":99,"downvotes":0,"url":"https://cdn2.steamgriddb.com/grid/nsfw.png"},
        {"id":3,"score":0,"style":"alternate","width":920,"height":430,"nsfw":false,"humor":false,"epilepsy":false,"upvotes":50,"downvotes":0,"url":"https://cdn2.steamgriddb.com/grid/wide.png"},
        {"id":4,"score":0,"style":"alternate","width":342,"height":482,"nsfw":false,"humor":false,"epilepsy":false,"upvotes":7,"downvotes":2,"url":"https://cdn2.steamgriddb.com/grid/b.jpg"},
        {"id":5,"score":0,"style":"alternate","width":600,"height":900,"nsfw":false,"humor":true,"epilepsy":false,"upvotes":40,"downvotes":0,"url":"https://cdn2.steamgriddb.com/grid/funny.png"}]}"#;

    #[test]
    fn parses_search_and_picks_exact_name_first() {
        let hits = parse_search(SEARCH).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(best_match(&hits, "Half-Life 2"), Some(9));
        assert_eq!(best_match(&hits, "Half"), Some(5), "first result otherwise");
        assert_eq!(best_match(&[], "x"), None);
    }

    #[test]
    fn parses_game_lookup() {
        assert_eq!(parse_game_id(GAME).unwrap(), Some(5247));
        assert_eq!(
            parse_game_id(r#"{"success":false,"errors":["Game not found"]}"#).unwrap(),
            None
        );
    }

    #[test]
    fn ranks_covers_by_votes_and_filters_unsafe_or_wrong_size() {
        let images = parse_images(GRIDS).unwrap();
        assert_eq!(images.len(), 5);
        let urls = rank_images(images, Some(&COVER_SIZES));
        assert_eq!(
            urls,
            vec![
                "https://cdn2.steamgriddb.com/grid/b.jpg",
                "https://cdn2.steamgriddb.com/grid/a.png"
            ],
            "net votes 5 beats 1; nsfw, humor and wide images are dropped"
        );
    }

    #[test]
    fn heroes_are_not_size_filtered_and_empty_data_is_ok() {
        let urls = rank_images(parse_images(GRIDS).unwrap(), None);
        assert_eq!(urls.len(), 2 + 1, "only unsafe images dropped");
        assert!(
            parse_images(r#"{"success":true,"data":[]}"#)
                .unwrap()
                .is_empty()
        );
        assert!(parse_images("<html>").is_err());
    }

    #[test]
    fn encodes_path_segments() {
        assert_eq!(encode("Half-Life 2"), "Half-Life%202");
        assert_eq!(encode("Tom & Jerry/ß"), "Tom%20%26%20Jerry%2F%C3%9F");
    }

    #[test]
    fn maps_known_platforms() {
        assert_eq!(platform_name("steam"), Some("steam"));
        assert_eq!(platform_name("epic"), Some("egs"));
        assert_eq!(platform_name("gog"), None);
        assert_eq!(platform_name("manual"), None);
    }

    #[test]
    fn key_handling_and_debug_do_not_leak() {
        let api = SteamGridDb::new(Some("  ".into()));
        assert!(!api.is_configured());
        api.set_api_key(Some(" secret-key ".into()));
        assert!(api.is_configured());
        assert_eq!(api.key().as_deref(), Some("secret-key"));
        assert!(!format!("{api:?}").contains("secret"));
        api.set_api_key(None);
        assert!(!api.is_configured());
        assert!(matches!(
            api.resolve_game(&Lookup {
                name: "x",
                source: "manual",
                source_game_id: None
            }),
            Err(CoreError::Invalid(_))
        ));
    }

    #[test]
    fn throttle_spaces_requests() {
        let api = SteamGridDb::with_interval(None, Duration::from_millis(40));
        let start = Instant::now();
        api.throttle();
        api.throttle();
        api.throttle();
        assert!(start.elapsed() >= Duration::from_millis(80));
    }
}
