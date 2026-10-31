//! Steam web access: owned games (Web API, with the user's key or a web session token) and
//! store metadata. Fetching
//! and parsing are separate so the parsers are tested with fixtures and never touch the network.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::DateTime;
use serde_json::Value;

use questkeep_store_api::ProviderError;
use questkeep_store_api::model::{GameLink, GameMetadata, ImportedGame, PlayAction, Score};

/// Why a request failed, without any URL (which may contain the API key).
pub(crate) enum HttpFailure {
    Status(u16),
    Other(String),
}

pub(crate) fn http_get(url: &str, secret: Option<&str>) -> Result<String, HttpFailure> {
    let http = questkeep_store_api::http_agent();
    let mut response = http.get(url).call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => HttpFailure::Status(code),
        other => HttpFailure::Other(redact(&other.to_string(), secret)),
    })?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| HttpFailure::Other(redact(&e.to_string(), secret)))
}

/// Removes the secret (and its URL-encoded form) from a message.
fn redact(message: &str, secret: Option<&str>) -> String {
    match secret.filter(|s| !s.is_empty()) {
        Some(secret) => message.replace(secret, "<redacted>"),
        None => message.to_string(),
    }
}

/// How a Web API request is authorized.
pub enum Credential<'a> {
    /// The user's Web API key.
    Key(&'a str),
    /// The `webapi_token` of a signed-in store session.
    Token(&'a str),
}

/// Why an owned-games request failed.
pub enum OwnedError {
    /// HTTP 401/403: the key or token is not (or no longer) accepted.
    Rejected,
    Other(ProviderError),
}

/// Fetches the owned games of a Steam account.
pub fn fetch_owned(
    credential: Credential<'_>,
    steam_id: &str,
) -> Result<Vec<ImportedGame>, OwnedError> {
    let (param, secret) = match credential {
        Credential::Key(key) => ("key", key),
        Credential::Token(token) => ("access_token", token),
    };
    let url = format!(
        "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/?{param}={}&steamid={}&include_appinfo=1&include_played_free_games=1&format=json",
        encode(secret),
        encode(steam_id)
    );
    match http_get(&url, Some(secret)) {
        Ok(body) => parse_owned_games(&body).map_err(OwnedError::Other),
        Err(HttpFailure::Status(401 | 403)) => Err(OwnedError::Rejected),
        Err(HttpFailure::Status(code)) => Err(OwnedError::Other(ProviderError::Network(format!(
            "Steam Web API returned HTTP {code}"
        )))),
        Err(HttpFailure::Other(message)) => Err(OwnedError::Other(ProviderError::Network(message))),
    }
}

/// Fetches store metadata; `Ok(None)` when the store has no data for the app. The app details
/// decide the result; tags and the review score are best effort (a failure only leaves them out).
pub fn fetch_metadata(app_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
    if app_id.is_empty() || !app_id.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let url = format!("https://store.steampowered.com/api/appdetails?appids={app_id}&l=english");
    let parsed = match http_get(&url, None) {
        Ok(body) => parse_app_details(app_id, &body)?,
        Err(HttpFailure::Status(429)) => {
            return Err(ProviderError::Network("rate limited".into()));
        }
        Err(HttpFailure::Status(code)) => {
            return Err(ProviderError::Network(format!(
                "Steam store returned HTTP {code}"
            )));
        }
        Err(HttpFailure::Other(message)) => return Err(ProviderError::Network(message)),
    };
    let Some(mut meta) = parsed else {
        return Ok(None);
    };
    match fetch_store_item(app_id) {
        Ok(Some(item)) => {
            meta.community_score = item.community_score;
            if !item.tag_ids.is_empty() {
                meta.tags = tag_names(&item.tag_ids);
            }
        }
        Ok(None) => {}
        Err(e) => tracing::debug!("Steam tags and reviews for {app_id} unavailable: {e}"),
    }
    Ok(Some(meta))
}

/// What `IStoreBrowseService/GetItems` adds to the app details.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoreItem {
    /// Tag ids, most voted first.
    pub tag_ids: Vec<u32>,
    pub community_score: Option<Score>,
}

fn fetch_store_item(app_id: &str) -> Result<Option<StoreItem>, ProviderError> {
    let input = format!(
        r#"{{"ids":[{{"appid":{app_id}}}],"context":{{"language":"english","country_code":"US"}},"data_request":{{"include_reviews":true,"include_tag_count":20}}}}"#
    );
    let url = format!(
        "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/?input_json={}",
        encode(&input)
    );
    match http_get(&url, None) {
        Ok(body) => Ok(parse_store_item(app_id, &body)),
        Err(HttpFailure::Status(code)) => Err(ProviderError::Network(format!(
            "Steam store returned HTTP {code}"
        ))),
        Err(HttpFailure::Other(message)) => Err(ProviderError::Network(message)),
    }
}

/// Store item types that are apps rather than games (`EStoreAppType`): movie, software,
/// video, music, tool. Game is 0; demos, mods and DLC stay games.
const APP_TYPES: [u64; 5] = [3, 6, 7, 11, 13];
/// Apps per `GetItems` request.
const TYPE_BATCH: usize = 100;

/// Whether an app id is an app (software, tool) rather than a game, cached per process: an
/// app's type doesn't change. Ids the store doesn't list are left out.
static APP_KINDS: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

/// Sets `app` on every scanned game the Steam store knows (one request per hundred unknown ids).
/// A failed request leaves those games as they are.
pub fn mark_apps(games: &mut [ImportedGame]) {
    let Ok(mut cache) = APP_KINDS.lock() else {
        return;
    };
    let known = cache.get_or_insert_with(HashMap::new);
    let missing: Vec<&str> = games
        .iter()
        .map(|g| g.source_game_id.as_str())
        .filter(|id| !known.contains_key(*id) && id.parse::<u32>().is_ok())
        .collect();
    for batch in missing.chunks(TYPE_BATCH) {
        match fetch_app_kinds(batch) {
            Ok(kinds) => known.extend(kinds),
            Err(err) => tracing::debug!("Steam app types unavailable: {err}"),
        }
    }
    for game in games.iter_mut() {
        game.app = known.get(&game.source_game_id).copied();
    }
}

fn fetch_app_kinds(ids: &[&str]) -> Result<Vec<(String, bool)>, ProviderError> {
    let list: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"{{"appid":{id}}}"#))
        .collect();
    let input = format!(
        r#"{{"ids":[{}],"context":{{"language":"english","country_code":"US"}},"data_request":{{}}}}"#,
        list.join(",")
    );
    let url = format!(
        "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/?input_json={}",
        encode(&input)
    );
    match http_get(&url, None) {
        Ok(body) => Ok(parse_app_kinds(&body)),
        Err(HttpFailure::Status(code)) => Err(ProviderError::Network(format!(
            "Steam store returned HTTP {code}"
        ))),
        Err(HttpFailure::Other(message)) => Err(ProviderError::Network(message)),
    }
}

/// App id and "is an app" for every item of a `GetItems` response the store found.
pub fn parse_app_kinds(body: &str) -> Vec<(String, bool)> {
    let Ok(json) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    json.get("response")
        .and_then(|r| r.get("store_items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("success").and_then(Value::as_u64) == Some(1))
                .filter_map(|item| {
                    let id = item.get("appid")?.as_u64()?;
                    let kind = item.get("type").and_then(Value::as_u64).unwrap_or(0);
                    Some((id.to_string(), APP_TYPES.contains(&kind)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Tag id to name, fetched once per process (it only changes when Steam adds tags).
static TAG_NAMES: Mutex<Option<HashMap<u32, String>>> = Mutex::new(None);

fn tag_names(ids: &[u32]) -> Vec<String> {
    let Ok(mut cache) = TAG_NAMES.lock() else {
        return Vec::new();
    };
    if cache.is_none() {
        let url = "https://api.steampowered.com/IStoreService/GetTagList/v1/?language=english";
        match http_get(url, None) {
            Ok(body) => {
                let names = parse_tag_list(&body);
                if !names.is_empty() {
                    *cache = Some(names);
                }
            }
            Err(HttpFailure::Status(code)) => {
                tracing::debug!("Steam tag list returned HTTP {code}");
            }
            Err(HttpFailure::Other(message)) => {
                tracing::debug!("Steam tag list unavailable: {message}");
            }
        }
    }
    cache
        .as_ref()
        .map(|names| resolve_tags(ids, names))
        .unwrap_or_default()
}

/// Tag names for ids, in order; unknown ids are skipped.
pub fn resolve_tags(ids: &[u32], names: &HashMap<u32, String>) -> Vec<String> {
    ids.iter().filter_map(|id| names.get(id).cloned()).collect()
}

/// Maps a `GetTagList` response to tag id and name.
pub fn parse_tag_list(body: &str) -> HashMap<u32, String> {
    let Ok(json) = serde_json::from_str::<Value>(body) else {
        return HashMap::new();
    };
    json.get("response")
        .and_then(|r| r.get("tags"))
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|t| {
                    let id = u32::try_from(t.get("tagid")?.as_u64()?).ok()?;
                    let name = t.get("name")?.as_str()?.trim();
                    (!name.is_empty()).then(|| (id, name.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Tags and review summary of `app_id` from a `GetItems` response (asked with `include_reviews`
/// and `include_tag_count`). `None` when the item is missing or unsuccessful. The review summary
/// comes from here because the `appreviews` endpoint now redirects to the store front page.
pub fn parse_store_item(app_id: &str, body: &str) -> Option<StoreItem> {
    let json: Value = serde_json::from_str(body).ok()?;
    let item = json
        .get("response")?
        .get("store_items")?
        .as_array()?
        .iter()
        .find(|item| {
            item.get("appid")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .as_deref()
                == Some(app_id)
        })?;
    if item.get("success").and_then(Value::as_u64) != Some(1) {
        return None;
    }
    let as_id = |v: &Value| v.as_u64().and_then(|id| u32::try_from(id).ok());
    let tag_ids = match item.get("tagids").and_then(Value::as_array) {
        Some(ids) => ids.iter().filter_map(as_id).collect(),
        None => item
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(|t| t.get("tagid").and_then(as_id))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let community_score = item
        .get("reviews")
        .and_then(|r| r.get("summary_filtered"))
        .and_then(|summary| {
            let count = summary.get("review_count")?.as_u64()?;
            let percent = u8::try_from(summary.get("percent_positive")?.as_u64()?).ok()?;
            (count >= MIN_REVIEWS && percent <= 100).then(|| Score {
                value: percent,
                source: "Steam reviews".into(),
            })
        });
    Some(StoreItem {
        tag_ids,
        community_score,
    })
}

/// Fewer reviews than this say nothing about a game.
const MIN_REVIEWS: u64 = 10;

/// Hosts of Steam's community image CDN, tried in order for an app icon.
const ICON_HOSTS: [&str; 2] = [
    "cdn.cloudflare.steamstatic.com",
    "cdn.akamai.steamstatic.com",
];

/// URLs of the app's small square icon (the one Steam shows in its library list), best first.
/// Its file name is a hash that only the store knows, so it is looked up through the keyless
/// `IStoreBrowseService/GetItems` API. Empty when the store has no icon for the app.
pub fn fetch_icon_urls(app_id: &str) -> Result<Vec<String>, ProviderError> {
    if app_id.is_empty() || !app_id.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(Vec::new());
    }
    let input = format!(
        r#"{{"ids":[{{"appid":{app_id}}}],"context":{{"language":"english","country_code":"US"}},"data_request":{{"include_assets":true}}}}"#
    );
    let url = format!(
        "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/?input_json={}",
        encode(&input)
    );
    match http_get(&url, None) {
        Ok(body) => Ok(parse_community_icon(app_id, &body)
            .map(|hash| icon_urls(app_id, &hash))
            .unwrap_or_default()),
        Err(HttpFailure::Status(code)) => Err(ProviderError::Network(format!(
            "Steam store returned HTTP {code}"
        ))),
        Err(HttpFailure::Other(message)) => Err(ProviderError::Network(message)),
    }
}

/// The `community_icon` hash of `app_id` from a `GetItems` response; `None` when missing or not a
/// plain hex hash (it becomes part of a URL).
pub fn parse_community_icon(app_id: &str, body: &str) -> Option<String> {
    let json: Value = serde_json::from_str(body).ok()?;
    json.get("response")?
        .get("store_items")?
        .as_array()?
        .iter()
        .find(|item| {
            item.get("appid")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .as_deref()
                == Some(app_id)
        })?
        .get("assets")?
        .get("community_icon")?
        .as_str()
        .filter(|hash| !hash.is_empty() && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
}

fn icon_urls(app_id: &str, hash: &str) -> Vec<String> {
    ICON_HOSTS
        .iter()
        .map(|host| format!("https://{host}/steamcommunity/public/images/apps/{app_id}/{hash}.jpg"))
        .collect()
}

/// Percent-encodes everything except unreserved characters.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Maps a `GetOwnedGames` response body to games.
pub fn parse_owned_games(body: &str) -> Result<Vec<ImportedGame>, ProviderError> {
    let json: Value = serde_json::from_str(body).map_err(|_| {
        ProviderError::Parse("Steam returned an unexpected response for the game list.".into())
    })?;
    let response = json
        .get("response")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ProviderError::Parse("Steam returned an unexpected response for the game list.".into())
        })?;
    let Some(games) = response.get("games").and_then(Value::as_array) else {
        if response.get("game_count").and_then(Value::as_u64) == Some(0) {
            return Ok(Vec::new());
        }
        // Private profiles answer with an empty `response` object.
        return Err(ProviderError::Parse(
            "Steam returned no games. Set \"Game details\" in your Steam profile privacy settings to Public, and check the Steam ID.".into(),
        ));
    };

    let mut owned = Vec::new();
    for entry in games {
        let Some(app_id) = entry.get("appid").and_then(Value::as_u64) else {
            continue;
        };
        let id = app_id.to_string();
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map(super::scan::clean_name)
            .unwrap_or_default();
        if name.is_empty() || super::scan::is_tool_app(&id, &name) {
            continue;
        }
        let minutes = entry
            .get("playtime_forever")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let last_played = entry
            .get("rtime_last_played")
            .and_then(Value::as_i64)
            .filter(|&t| t > 0)
            .and_then(|t| DateTime::from_timestamp(t, 0));
        owned.push(ImportedGame {
            installed: false,
            play_action: Some(PlayAction::uri(format!("steam://rungameid/{id}"))),
            playtime_seconds: Some(minutes.saturating_mul(60)),
            last_played,
            ..ImportedGame::installed(id, name)
        });
    }
    owned.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
    Ok(owned)
}

/// Maps an `appdetails` response body (for one app id) to metadata.
pub fn parse_app_details(app_id: &str, body: &str) -> Result<Option<GameMetadata>, ProviderError> {
    let json: Value = serde_json::from_str(body)
        .map_err(|_| ProviderError::Parse("Steam returned unreadable store data.".into()))?;
    let Some(entry) = json.get(app_id) else {
        return Ok(None);
    };
    if entry.get("success").and_then(Value::as_bool) != Some(true) {
        return Ok(None);
    }
    let Some(data) = entry.get("data") else {
        return Ok(None);
    };

    let text = |key: &str| {
        data.get(key)
            .and_then(Value::as_str)
            .map(strip_html)
            .filter(|s| !s.is_empty())
    };
    let description = text("short_description")
        .or_else(|| text("about_the_game").map(|s| truncate_at_word(&s, ABOUT_MAX_CHARS)));
    let release_date = data
        .get("release_date")
        .and_then(|r| r.get("date"))
        .and_then(Value::as_str)
        .and_then(parse_release_date);
    let strings = |key: &str| -> Vec<String> {
        data.get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };
    let genres = data
        .get("genres")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|g| g.get("description").and_then(Value::as_str))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let mut features: Vec<String> = Vec::new();
    for name in data
        .get("categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("description").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !features.iter().any(|f| f == name) {
            features.push(name.to_string());
        }
    }
    let platforms = [("windows", "Windows"), ("mac", "macOS"), ("linux", "Linux")]
        .into_iter()
        .filter(|(key, _)| {
            data.get("platforms")
                .and_then(|p| p.get(*key))
                .and_then(Value::as_bool)
                == Some(true)
        })
        .map(|(_, name)| name.to_string())
        .collect();
    let critic_score = data
        .get("metacritic")
        .and_then(|m| m.get("score"))
        .and_then(Value::as_u64)
        .and_then(|s| u8::try_from(s).ok())
        .filter(|&s| s <= 100)
        .map(|value| Score {
            value,
            source: "Metacritic".into(),
        });
    let has_achievements = data
        .get("achievements")
        .and_then(|a| a.get("total"))
        .and_then(Value::as_u64)
        .is_some_and(|t| t > 0);
    let has_workshop = features.iter().any(|f| f == "Steam Workshop");
    let website = data
        .get("website")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|w| w.starts_with("http://") || w.starts_with("https://"));
    let links = steam_links(app_id, has_achievements, has_workshop, website);

    Ok(Some(GameMetadata {
        description,
        release_date,
        developers: strings("developers"),
        publishers: strings("publishers"),
        genres,
        features,
        platforms,
        critic_score,
        links,
        ..GameMetadata::default()
    }))
}

fn steam_links(
    app_id: &str,
    achievements: bool,
    workshop: bool,
    website: Option<&str>,
) -> Vec<GameLink> {
    let mut links = vec![
        GameLink::new(
            "Steam store page",
            format!("https://store.steampowered.com/app/{app_id}"),
        ),
        GameLink::new(
            "Community hub",
            format!("https://steamcommunity.com/app/{app_id}"),
        ),
        GameLink::new(
            "Discussions",
            format!("https://steamcommunity.com/app/{app_id}/discussions/"),
        ),
        GameLink::new(
            "Guides",
            format!("https://steamcommunity.com/app/{app_id}/guides/"),
        ),
        GameLink::new(
            "News",
            format!("https://store.steampowered.com/news/app/{app_id}"),
        ),
        GameLink::new(
            "PCGamingWiki",
            format!("https://pcgamingwiki.com/api/appid.php?appid={app_id}"),
        ),
    ];
    if achievements {
        links.push(GameLink::new(
            "Achievements",
            format!("https://steamcommunity.com/stats/{app_id}/achievements"),
        ));
    }
    if workshop {
        links.push(GameLink::new(
            "Workshop",
            format!("https://steamcommunity.com/app/{app_id}/workshop/"),
        ));
    }
    if let Some(site) = website {
        links.push(GameLink::new("Official website", site));
    }
    links
}

/// Longest description taken from `about_the_game` when there is no short one.
const ABOUT_MAX_CHARS: usize = 600;

/// Cuts at a word boundary (preferring a paragraph end) and appends an ellipsis if shortened.
fn truncate_at_word(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    let cut = match cut.rfind("\n\n") {
        Some(pos) if pos > max_chars / 2 => &cut[..pos],
        _ => cut
            .rfind(char::is_whitespace)
            .map_or(cut.as_str(), |p| &cut[..p]),
    };
    format!("{}\u{2026}", cut.trim_end())
}

/// Converts store HTML to plain text: block tags become line breaks, other tags are dropped,
/// common entities are decoded and whitespace is normalized.
pub fn strip_html(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '<' {
            text.push(c);
            continue;
        }
        let mut tag = String::new();
        let mut closed = false;
        for t in chars.by_ref() {
            if t == '>' {
                closed = true;
                break;
            }
            tag.push(t);
        }
        if !closed {
            // A stray `<` without a closing `>` is just text.
            text.push('<');
            text.push_str(&tag);
            break;
        }
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match name.as_str() {
            "br" | "p" | "div" | "h1" | "h2" | "h3" | "h4" | "ul" | "ol" => text.push('\n'),
            "li" if !tag.starts_with('/') => text.push_str("\n- "),
            "li" => {}
            _ => {}
        }
    }
    let decoded = decode_entities(&text);
    let mut lines: Vec<String> = Vec::new();
    for line in decoded.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() && lines.last().is_none_or(String::is_empty) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest[1..]
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| decode_entity(&rest[1..1 + end]).map(|c| (c, end + 2)));
        match decoded {
            Some((c, consumed)) => {
                out.push(c);
                rest = &rest[consumed..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "ndash" => Some('\u{2013}'),
        "mdash" => Some('\u{2014}'),
        "hellip" => Some('\u{2026}'),
        "lsquo" => Some('\u{2018}'),
        "rsquo" => Some('\u{2019}'),
        "ldquo" => Some('\u{201c}'),
        "rdquo" => Some('\u{201d}'),
        "trade" => Some('\u{2122}'),
        "reg" => Some('\u{ae}'),
        "copy" => Some('\u{a9}'),
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Parses store release dates (`21 Feb, 2019`, `Feb 21, 2019`, `Feb 2019`, `2019`) into
/// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`. Anything else (`Coming soon`, `TBA`) gives `None`.
pub fn parse_release_date(text: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let mut year: Option<u32> = None;
    let mut month: Option<usize> = None;
    let mut day: Option<u32> = None;
    for token in text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
    {
        if token.bytes().all(|b| b.is_ascii_digit()) {
            let value: u32 = token.parse().ok()?;
            if token.len() == 4 && year.is_none() {
                year = Some(value);
            } else if token.len() <= 2 && day.is_none() && (1..=31).contains(&value) {
                day = Some(value);
            } else {
                return None;
            }
        } else {
            let lower = token.to_ascii_lowercase();
            let index = MONTHS
                .iter()
                .position(|m| lower.len() >= 3 && lower.starts_with(m))?;
            if month.replace(index + 1).is_some() {
                return None;
            }
        }
    }
    let year = year.filter(|y| (1950..=2100).contains(y))?;
    match (month, day) {
        (Some(m), Some(d)) => {
            chrono::NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, u32::try_from(m).ok()?, d)?;
            Some(format!("{year:04}-{m:02}-{d:02}"))
        }
        (Some(m), None) => Some(format!("{year:04}-{m:02}")),
        (None, None) => Some(format!("{year:04}")),
        (None, Some(_)) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_app_types_from_store_items() {
        // Recorded from GetItems (2026-10-31): TF2 is a game, Wallpaper Engine and Blender are
        // software, the redistributables aren't in the store.
        let body = r#"{"response":{"store_items":[
            {"item_type":0,"id":440,"success":1,"visible":true,"name":"Team Fortress 2","appid":440,"type":0},
            {"item_type":0,"id":431960,"success":1,"visible":true,"name":"Wallpaper Engine","appid":431960,"type":6},
            {"item_type":0,"id":228980,"success":15},
            {"item_type":0,"id":365670,"success":1,"visible":true,"name":"Blender","appid":365670,"type":6}
        ]}}"#;
        assert_eq!(
            parse_app_kinds(body),
            vec![
                ("440".to_string(), false),
                ("431960".to_string(), true),
                ("365670".to_string(), true)
            ]
        );
        assert!(parse_app_kinds("nope").is_empty());
    }

    #[test]
    fn maps_owned_games() {
        let body = r#"{"response":{"game_count":4,"games":[
            {"appid":620,"name":"Portal\u2122 2","playtime_forever":90,"rtime_last_played":1700000000},
            {"appid":440,"name":"Team Fortress 2","playtime_forever":0,"rtime_last_played":0},
            {"appid":1,"playtime_forever":5},
            {"appid":228980,"name":"Steamworks Common Redistributables"}
        ]}}"#;
        let games = parse_owned_games(body).unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].name, "Portal 2");
        assert_eq!(games[0].source_game_id, "620");
        assert!(!games[0].installed);
        assert_eq!(games[0].playtime_seconds, Some(5400));
        assert_eq!(games[0].last_played.unwrap().timestamp(), 1_700_000_000);
        assert_eq!(
            games[0].play_action,
            Some(PlayAction::uri("steam://rungameid/620"))
        );
        assert_eq!(games[0].install_dir, None);
        assert_eq!(games[1].name, "Team Fortress 2");
        assert_eq!(games[1].playtime_seconds, Some(0));
        assert_eq!(games[1].last_played, None);
    }

    #[test]
    fn private_profile_is_a_readable_error() {
        let err = parse_owned_games(r#"{"response":{}}"#).unwrap_err();
        assert!(matches!(&err, ProviderError::Parse(m) if m.contains("Public")));
        assert!(
            parse_owned_games(r#"{"response":{"game_count":0}}"#)
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            parse_owned_games("<html>"),
            Err(ProviderError::Parse(_))
        ));
        assert!(matches!(
            parse_owned_games("{}"),
            Err(ProviderError::Parse(_))
        ));
    }

    #[test]
    fn maps_app_details() {
        let body = r#"{"620":{"success":true,"data":{
            "short_description":"Portal 2 &amp; <b>more</b>:<br>a &quot;puzzle&quot; game&nbsp;&#8211;&#x2122;",
            "about_the_game":"ignored",
            "release_date":{"coming_soon":false,"date":"18 Apr, 2011"},
            "developers":["Valve"],"publishers":["Valve"," "],
            "genres":[{"id":"1","description":"Action"},{"id":"2","description":"Adventure"}]
        }}}"#;
        let meta = parse_app_details("620", body).unwrap().unwrap();
        assert_eq!(
            meta.description.as_deref(),
            Some("Portal 2 & more:\na \"puzzle\" game \u{2013}\u{2122}")
        );
        assert_eq!(meta.release_date.as_deref(), Some("2011-04-18"));
        assert_eq!(meta.developers, ["Valve"]);
        assert_eq!(meta.publishers, ["Valve"]);
        assert_eq!(meta.genres, ["Action", "Adventure"]);
    }

    #[test]
    fn app_details_fixture_gives_features_platforms_scores_and_links() {
        let body = include_str!("../fixtures/appdetails_620.json");
        let meta = parse_app_details("620", body).unwrap().unwrap();
        assert_eq!(meta.features[0], "Single-player");
        assert_eq!(
            meta.features
                .iter()
                .filter(|f| *f == "Steam Workshop")
                .count(),
            1
        );
        assert_eq!(meta.platforms, ["Windows", "Linux"]);
        assert_eq!(
            meta.critic_score,
            Some(Score {
                value: 95,
                source: "Metacritic".into()
            })
        );
        let names: Vec<_> = meta.links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Steam store page",
                "Community hub",
                "Discussions",
                "Guides",
                "News",
                "PCGamingWiki",
                "Achievements",
                "Workshop",
                "Official website"
            ]
        );
        assert_eq!(meta.links[0].url, "https://store.steampowered.com/app/620");
        assert_eq!(meta.links[8].url, "http://www.thinkwithportals.com/");
    }

    #[test]
    fn app_details_links_skip_missing_extras() {
        let body = r#"{"5":{"success":true,"data":{"short_description":"x",
            "categories":[{"id":2,"description":"Single-player"}],
            "platforms":{"windows":true,"mac":true,"linux":false},
            "metacritic":{"score":250},"achievements":{"total":0},"website":""}}}"#;
        let meta = parse_app_details("5", body).unwrap().unwrap();
        assert_eq!(meta.platforms, ["Windows", "macOS"]);
        assert_eq!(meta.critic_score, None);
        assert_eq!(meta.links.len(), 6);
        assert!(meta.links.iter().all(|l| l.name != "Achievements"));
    }

    #[test]
    fn store_item_gives_tags_and_review_score() {
        let body = include_str!("../fixtures/store_items_620.json");
        let item = parse_store_item("620", body).unwrap();
        assert_eq!(item.tag_ids, [4182, 1625, 1664, 3839, 5923]);
        let score = item.community_score.unwrap();
        assert_eq!((score.value, score.source.as_str()), (98, "Steam reviews"));
        assert_eq!(parse_store_item("621", body), None);
        assert_eq!(parse_store_item("620", "<html>"), None);
    }

    #[test]
    fn store_item_edge_cases() {
        let few = r#"{"response":{"store_items":[{"appid":7,"success":1,
            "tags":[{"tagid":9,"weight":1},{"tagid":19,"weight":0}],
            "reviews":{"summary_filtered":{"review_count":9,"percent_positive":100}}}]}}"#;
        let item = parse_store_item("7", few).unwrap();
        assert_eq!(item.tag_ids, [9, 19]);
        assert_eq!(item.community_score, None);
        let failed = r#"{"response":{"store_items":[{"appid":7,"success":2}]}}"#;
        assert_eq!(parse_store_item("7", failed), None);
    }

    #[test]
    fn tag_list_resolves_names_in_order() {
        let names = parse_tag_list(include_str!("../fixtures/tag_list.json"));
        assert_eq!(names.get(&1664).map(String::as_str), Some("Puzzle"));
        assert_eq!(
            resolve_tags(&[4182, 99999, 1625], &names),
            ["Singleplayer", "Platformer"]
        );
        assert!(parse_tag_list("nope").is_empty());
    }

    #[test]
    fn app_details_falls_back_to_truncated_about_text() {
        let long = "word ".repeat(300);
        let body = format!(
            r#"{{"1":{{"success":true,"data":{{"short_description":"","about_the_game":"<p>{long}</p>"}}}}}}"#
        );
        let meta = parse_app_details("1", &body).unwrap().unwrap();
        let description = meta.description.unwrap();
        assert!(description.ends_with('\u{2026}'));
        assert!(description.chars().count() <= ABOUT_MAX_CHARS + 1);
        assert_eq!(meta.release_date, None);
        assert!(meta.genres.is_empty());
    }

    #[test]
    fn app_details_unsuccessful_or_missing_is_none() {
        assert_eq!(
            parse_app_details("9", r#"{"9":{"success":false}}"#).unwrap(),
            None
        );
        assert_eq!(
            parse_app_details("9", r#"{"8":{"success":true}}"#).unwrap(),
            None
        );
        assert_eq!(
            parse_app_details("9", r#"{"9":{"success":true}}"#).unwrap(),
            None
        );
        assert!(parse_app_details("9", "nope").is_err());
    }

    #[test]
    fn community_icon_becomes_cdn_urls() {
        let body = r#"{"response":{"store_items":[
            {"appid":1,"assets":{"community_icon":"ffff"}},
            {"appid":252950,"success":1,"assets":{"header":"x/header.jpg","community_icon":"9AD6dd3d173523354385955b5fb2af87639c4163"}}
        ]}}"#;
        let hash = parse_community_icon("252950", body).unwrap();
        assert_eq!(hash, "9ad6dd3d173523354385955b5fb2af87639c4163");
        assert_eq!(
            icon_urls("252950", &hash)[0],
            "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/252950/9ad6dd3d173523354385955b5fb2af87639c4163.jpg"
        );
        assert_eq!(parse_community_icon("2", body), None);
        assert_eq!(
            parse_community_icon(
                "3",
                r#"{"response":{"store_items":[{"appid":3,"assets":{"community_icon":"../x"}}]}}"#
            ),
            None
        );
        assert_eq!(
            parse_community_icon(
                "3",
                r#"{"response":{"store_items":[{"appid":3,"success":2}]}}"#
            ),
            None
        );
        assert_eq!(parse_community_icon("3", "<html>"), None);
    }

    #[test]
    fn strips_html() {
        assert_eq!(strip_html("a<br/>b<br />\n\n\n<p>c</p>"), "a\nb\n\nc");
        assert_eq!(
            strip_html("<ul><li>one</li><li>two &lt;3</li></ul>"),
            "- one\n- two <3"
        );
        assert_eq!(
            strip_html("Tom &amp;amp; Jerry &unknown; &#xZZ; a & b"),
            "Tom &amp; Jerry &unknown; &#xZZ; a & b"
        );
        assert_eq!(strip_html("  <img src=\"x\">  "), "");
        assert_eq!(strip_html("1 < 2"), "1 < 2");
    }

    #[test]
    fn parses_release_dates() {
        let cases = [
            ("21 Feb, 2019", Some("2019-02-21")),
            ("Feb 21, 2019", Some("2019-02-21")),
            ("February 21, 2019", Some("2019-02-21")),
            ("Feb 2019", Some("2019-02")),
            ("2019", Some("2019")),
            ("Sept 3, 2020", Some("2020-09-03")),
            ("Coming soon", None),
            ("To be announced", None),
            ("31 Feb, 2019", None),
            ("", None),
            ("Q3 2024", None),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_release_date(input).as_deref(), expected, "{input}");
        }
    }

    #[test]
    fn redacts_and_encodes() {
        assert_eq!(
            redact("failed for key=ABC123 here", Some("ABC123")),
            "failed for key=<redacted> here"
        );
        assert_eq!(redact("x", None), "x");
        assert_eq!(encode("a b&c=d"), "a%20b%26c%3Dd");
    }
}
