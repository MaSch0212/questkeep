//! Data for the game page: current players, news and the user's achievements. Fetching and
//! parsing are separate (like in `web.rs`): the `parse_*` functions are pure and tested with
//! fixtures. Caching is the core's job.

use std::collections::HashMap;

use serde_json::Value;

use questkeep_store_api::ProviderError;
use questkeep_store_api::model::{Achievement, Achievements, GameNews, NewsItem};

use crate::web::{Credential, HttpFailure, encode, http_get};

const API: &str = "https://api.steampowered.com";
/// Where Steam's `{STEAM_CLAN_IMAGE}` placeholder in news points to.
const CLAN_IMAGE_BASE: &str = "https://clan.akamai.steamstatic.com/images/";
const SNIPPET_LEN: usize = 300;

fn valid_app_id(app_id: &str) -> bool {
    !app_id.is_empty() && app_id.bytes().all(|b| b.is_ascii_digit())
}

fn failure(f: HttpFailure) -> ProviderError {
    match f {
        HttpFailure::Status(429) => ProviderError::Network("rate limited".into()),
        HttpFailure::Status(code) => {
            ProviderError::Network(format!("Steam Web API returned HTTP {code}"))
        }
        HttpFailure::Other(message) => ProviderError::Network(message),
    }
}

// ---------------------------------------------------------------------------------------------
// Player count
// ---------------------------------------------------------------------------------------------

/// Current players of an app; `Ok(None)` when Steam has no number for it.
pub fn fetch_player_count(app_id: &str) -> Result<Option<u32>, ProviderError> {
    if !valid_app_id(app_id) {
        return Ok(None);
    }
    let url = format!("{API}/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid={app_id}");
    parse_player_count(&http_get(&url, None).map_err(failure)?)
}

pub fn parse_player_count(body: &str) -> Result<Option<u32>, ProviderError> {
    let json: Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Parse(format!("Steam player count: {e}")))?;
    let response = &json["response"];
    if response["result"].as_i64() != Some(1) {
        return Ok(None);
    }
    Ok(response["player_count"]
        .as_u64()
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX)))
}

// ---------------------------------------------------------------------------------------------
// News
// ---------------------------------------------------------------------------------------------

/// The latest news of an app; `Ok(None)` when there is none.
pub fn fetch_news(app_id: &str) -> Result<Option<GameNews>, ProviderError> {
    if !valid_app_id(app_id) {
        return Ok(None);
    }
    let url = format!("{API}/ISteamNews/GetNewsForApp/v2/?appid={app_id}&count=5&maxlength=300");
    parse_news(app_id, &http_get(&url, None).map_err(failure)?)
}

pub fn parse_news(app_id: &str, body: &str) -> Result<Option<GameNews>, ProviderError> {
    let json: Value =
        serde_json::from_str(body).map_err(|e| ProviderError::Parse(format!("Steam news: {e}")))?;
    let Some(entries) = json["appnews"]["newsitems"].as_array() else {
        return Ok(None);
    };
    let items: Vec<NewsItem> = entries.iter().filter_map(parse_news_item).collect();
    if items.is_empty() {
        return Ok(None);
    }
    Ok(Some(GameNews {
        items,
        all_url: Some(format!("https://store.steampowered.com/news/app/{app_id}")),
    }))
}

fn parse_news_item(entry: &Value) -> Option<NewsItem> {
    let title = clean_text(entry["title"].as_str()?, usize::MAX);
    let url = http_url(entry["url"].as_str()?)?;
    if title.is_empty() {
        return None;
    }
    let contents = entry["contents"].as_str().unwrap_or_default();
    let text_field = |key: &str| {
        entry[key]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Some(NewsItem {
        title,
        url,
        date: entry["date"].as_i64(),
        source: text_field("feedlabel")
            .or_else(|| text_field("author"))
            .unwrap_or_else(|| "Steam".into()),
        snippet: clean_text(contents, SNIPPET_LEN),
        image: first_image(contents),
    })
}

/// `url` when it is an http(s) URL (spaces are escaped; Steam's external post links have them).
fn http_url(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")).then(|| url.replace(' ', "%20"))
}

/// The first image in news contents: `[img]...[/img]` or `<img src="...">`.
fn first_image(contents: &str) -> Option<String> {
    let lower = contents.to_ascii_lowercase();
    let bb = lower.find("[img]").map(|start| {
        let from = start + "[img]".len();
        let end = lower[from..]
            .find("[/img]")
            .map_or(contents.len(), |e| from + e);
        (start, contents[from..end].trim().to_string())
    });
    let html = lower.find("<img").and_then(|start| {
        let tag_end = lower[start..]
            .find('>')
            .map_or(contents.len(), |e| start + e);
        let tag = &contents[start..tag_end];
        let tag_lower = &lower[start..tag_end];
        let attr = tag_lower.find("src=")? + "src=".len();
        let rest = &tag[attr..];
        let value = match rest.chars().next()? {
            q @ ('"' | '\'') => rest[1..].split(q).next()?,
            _ => rest.split_whitespace().next()?,
        };
        Some((start, value.trim().to_string()))
    });
    let found = match (bb, html) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    };
    let (_, url) = found?;
    let url = match url.strip_prefix("{STEAM_CLAN_IMAGE}") {
        Some(rest) => format!("{CLAN_IMAGE_BASE}{}", rest.trim_start_matches('/')),
        None => url,
    };
    http_url(&url)
}

/// BBCode tags that are removed (see [`strip_markup`]); an unknown `[word]` stays as text.
const BBCODE_TAGS: &[&str] = &[
    "b",
    "i",
    "u",
    "s",
    "strike",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "url",
    "img",
    "list",
    "olist",
    "ul",
    "ol",
    "li",
    "*",
    "quote",
    "code",
    "spoiler",
    "noparse",
    "p",
    "br",
    "hr",
    "table",
    "tr",
    "td",
    "th",
    "video",
    "previewyoutube",
    "previewimage",
    "color",
    "size",
    "center",
    "left",
    "right",
    "dynamiclink",
    "expand",
    "sub",
];

/// Tags (BBCode and HTML) that separate words even though nothing sits between them.
const BLOCK_TAGS: &[&str] = &[
    "br",
    "p",
    "div",
    "li",
    "ul",
    "ol",
    "list",
    "olist",
    "*",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "tr",
    "td",
    "th",
    "table",
    "hr",
    "quote",
    "code",
    "blockquote",
    "center",
];

/// Tags whose content is dropped with them (it is a URL, not text).
const DROP_CONTENT: &[&str] = &["img", "video", "previewyoutube", "previewimage"];

/// Plain text of Steam news contents: markup removed, entities decoded, whitespace collapsed,
/// cut to at most `max` characters at a word boundary (with an ellipsis).
pub fn clean_text(raw: &str, max: usize) -> String {
    let stripped = strip_markup(raw);
    let decoded = decode_entities(&stripped);
    let text = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate(&text, max)
}

fn strip_markup(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = bytes[i];
        if b == b'<'
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/' || *c == b'!')
        {
            // An unterminated tag (the text was cut off) swallows the rest.
            let end = raw[i..].find('>').map_or(raw.len(), |e| i + e + 1);
            let name: String = lower[i + 1..end]
                .trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if BLOCK_TAGS.contains(&name.as_str()) {
                out.push(' ');
            }
            i = end;
            continue;
        }
        if b == b'['
            && let Some((name, close, closing)) = bbcode_tag(&lower, i)
        {
            if BLOCK_TAGS.contains(&name) {
                out.push(' ');
            }
            i = close.map_or(raw.len(), |c| c + 1);
            if !closing && DROP_CONTENT.contains(&name) {
                let end_tag = format!("[/{name}]");
                i = lower[i..]
                    .find(&end_tag)
                    .map_or(raw.len(), |e| i + e + end_tag.len());
            }
            continue;
        }
        // Copy one whole character.
        let len = raw[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&raw[i..i + len]);
        i += len;
    }
    out
}

/// A known BBCode tag starting at `lower[start]` (which is `[`): its name, the index of the
/// closing `]` (`None` when the text ends first) and whether it is a closing tag.
fn bbcode_tag(lower: &str, start: usize) -> Option<(&'static str, Option<usize>, bool)> {
    let rest = &lower[start + 1..];
    let (closing, rest) = match rest.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let name_len = if rest.starts_with('*') {
        1
    } else {
        rest.bytes().take_while(u8::is_ascii_alphanumeric).count()
    };
    let name = &rest[..name_len];
    let known = BBCODE_TAGS.iter().find(|t| **t == name)?;
    // After the name: `]`, or `=`/space and attributes up to `]`.
    match rest[name_len..].chars().next() {
        Some(']') | Some('=') | Some(' ') | None => {}
        Some(_) => return None,
    }
    let close = lower[start..].find(']').map(|c| start + c);
    if close.is_none() && closing {
        return None;
    }
    Some((known, close, closing))
}

/// Decodes the entities that show up in news (named and numeric).
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let decoded = rest[1..].find(';').filter(|e| *e <= 8).and_then(|end| {
            let name = &rest[1..1 + end];
            let ch = match name {
                "amp" => '&',
                "quot" => '"',
                "apos" => '\'',
                "lt" => '<',
                "gt" => '>',
                "nbsp" => ' ',
                _ => {
                    let num = name.strip_prefix('#')?;
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((ch, end + 2))
        });
        match decoded {
            Some((ch, used)) => {
                out.push(ch);
                rest = &rest[used..];
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

/// Cuts `text` to `max` characters at a word boundary and appends an ellipsis.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let next_is_space = text[cut.len()..].starts_with(' ');
    let kept = match cut.rfind(' ') {
        Some(space) if !next_is_space && space >= cut.len() / 2 => &cut[..space],
        _ => cut.as_str(),
    };
    let kept = kept.trim_end_matches(|c: char| c.is_whitespace() || ",.;:-".contains(c));
    format!("{kept}…")
}

// ---------------------------------------------------------------------------------------------
// Achievements
// ---------------------------------------------------------------------------------------------

/// Why an achievements request failed.
pub enum AchievementsError {
    /// HTTP 401/403: the key or token is not accepted.
    Rejected,
    Other(ProviderError),
}

impl From<ProviderError> for AchievementsError {
    fn from(e: ProviderError) -> Self {
        Self::Other(e)
    }
}

fn authed_get(
    path: &str,
    credential: &Credential<'_>,
    query: &str,
) -> Result<Option<String>, AchievementsError> {
    let (param, secret) = match credential {
        Credential::Key(key) => ("key", *key),
        Credential::Token(token) => ("access_token", *token),
    };
    let url = format!("{API}/{path}/?{param}={}&{query}", encode(secret));
    match http_get(&url, Some(secret)) {
        Ok(body) => Ok(Some(body)),
        Err(HttpFailure::Status(401 | 403)) => Err(AchievementsError::Rejected),
        // Steam answers 400 for apps without stats.
        Err(HttpFailure::Status(400)) => Ok(None),
        Err(other) => Err(AchievementsError::Other(failure(other))),
    }
}

/// Where `GetGameAchievements` icon file names live (one folder per app).
const ACHIEVEMENT_ICONS: &str =
    "https://shared.fastly.steamstatic.com/community_assets/images/apps";

/// A game's achievements and the share of players (0-100) that have each, by api name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Definitions {
    pub achievements: Vec<SchemaAchievement>,
    pub percents: HashMap<String, f32>,
}

/// What the game defines, with rarity. Public (`IPlayerService/GetGameAchievements`), so it
/// needs no key or sign-in. `Ok(None)` when the game has no achievements.
pub fn fetch_definitions(app_id: &str) -> Result<Option<Definitions>, ProviderError> {
    if !valid_app_id(app_id) {
        return Ok(None);
    }
    let url =
        format!("{API}/IPlayerService/GetGameAchievements/v1/?appid={app_id}&language=english");
    let body = match http_get(&url, None) {
        Ok(body) => body,
        Err(HttpFailure::Status(400 | 404)) => return Ok(None),
        Err(other) => return Err(failure(other)),
    };
    let definitions = parse_game_achievements(&body, app_id)?;
    Ok((!definitions.achievements.is_empty()).then_some(definitions))
}

/// The player's unlocks through the Web API (key or sign-in token). `Ok(None)` when Steam has
/// none to give (no stats for the app, private profile).
pub fn fetch_unlocks(
    credential: Credential<'_>,
    steam_id: &str,
    app_id: &str,
) -> Result<Option<Vec<Unlock>>, AchievementsError> {
    if !valid_app_id(app_id) {
        return Ok(None);
    }
    let Some(body) = authed_get(
        "ISteamUserStats/GetPlayerAchievements/v1",
        &credential,
        &format!("appid={app_id}&steamid={}&l=english", encode(steam_id)),
    )?
    else {
        return Ok(None);
    };
    Ok(parse_player_achievements(&body)?)
}

/// The player's unlocks from their community profile, which needs no key or sign-in when the
/// profile's game details are public. `Ok(None)` for a private profile.
pub fn fetch_community_unlocks(
    steam_id: &str,
    app_id: &str,
) -> Result<Option<Vec<Unlock>>, ProviderError> {
    if !valid_app_id(app_id) || !valid_app_id(steam_id) {
        return Ok(None);
    }
    let url = format!(
        "https://steamcommunity.com/profiles/{steam_id}/stats/{app_id}/achievements/?xml=1&l=english"
    );
    match http_get(&url, None) {
        Ok(body) => Ok(parse_community_achievements(&body)),
        Err(HttpFailure::Status(400 | 404)) => Ok(None),
        Err(other) => Err(failure(other)),
    }
}

/// An achievement as the game defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaAchievement {
    pub api_name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    /// The colored icon.
    pub icon: Option<String>,
}

/// An achievement the player has unlocked.
#[derive(Debug, Clone, PartialEq)]
pub struct Unlock {
    pub api_name: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub unlocked_at: Option<i64>,
}

fn non_empty(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The achievements of a `GetGameAchievements` response (empty when the game has none), with
/// icons as full URLs and the share of players that have each.
pub fn parse_game_achievements(body: &str, app_id: &str) -> Result<Definitions, ProviderError> {
    let json: Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Parse(format!("Steam achievement list: {e}")))?;
    let Some(list) = json["response"]["achievements"].as_array() else {
        return Ok(Definitions::default());
    };
    let mut definitions = Definitions::default();
    for a in list {
        let Some(api_name) = non_empty(&a["internal_name"]) else {
            continue;
        };
        if let Some(percent) = percent(&a["player_percent_unlocked"]) {
            definitions.percents.insert(api_name.clone(), percent);
        }
        // A bare file name; anything else is not trusted as a path.
        let icon = non_empty(&a["icon"])
            .filter(|f| {
                f.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
            })
            .map(|f| format!("{ACHIEVEMENT_ICONS}/{app_id}/{f}"));
        definitions.achievements.push(SchemaAchievement {
            api_name,
            display_name: non_empty(&a["localized_name"]),
            description: non_empty(&a["localized_desc"]),
            icon,
        });
    }
    Ok(definitions)
}

/// A percentage Steam sends as a string or a number.
fn percent(value: &Value) -> Option<f32> {
    let percent = match value {
        Value::String(s) => s.trim().parse::<f32>().ok()?,
        Value::Number(n) => n.as_f64()? as f32,
        _ => return None,
    };
    percent.is_finite().then_some(percent)
}

/// The unlocked achievements of a community profile's achievements XML (`closed="1"`).
/// `None` when the page has no achievement list (private profile, unknown game).
pub fn parse_community_achievements(body: &str) -> Option<Vec<Unlock>> {
    let list = xml_text(body, "achievements")?;
    let mut unlocks = Vec::new();
    for block in list.split("<achievement ").skip(1) {
        let block = block.split("</achievement>").next().unwrap_or_default();
        let (attributes, _) = block.split_once('>').unwrap_or_default();
        if !attributes.contains(r#"closed="1""#) {
            continue;
        }
        let Some(api_name) = xml_text(block, "apiname").filter(|s| !s.is_empty()) else {
            continue;
        };
        unlocks.push(Unlock {
            api_name,
            name: xml_text(block, "name").filter(|s| !s.is_empty()),
            description: xml_text(block, "description").filter(|s| !s.is_empty()),
            unlocked_at: xml_text(block, "unlockTimestamp")
                .and_then(|t| t.parse::<i64>().ok())
                .filter(|t| *t > 0),
        });
    }
    Some(unlocks)
}

/// The text of the first `<tag>` element in `xml` (CDATA unwrapped, entities decoded).
fn xml_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{tag}>"))?;
    let raw = xml[start..end].trim();
    Some(
        match raw
            .strip_prefix("<![CDATA[")
            .and_then(|r| r.strip_suffix("]]>"))
        {
            Some(data) => data.trim().to_string(),
            None => raw
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&"),
        },
    )
}

/// The unlocked achievements of a `GetPlayerAchievements` response. `Ok(None)` when Steam
/// reports an error instead of data (no stats for the app, private profile).
pub fn parse_player_achievements(body: &str) -> Result<Option<Vec<Unlock>>, ProviderError> {
    let json: Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Parse(format!("Steam achievements: {e}")))?;
    let stats = &json["playerstats"];
    if stats["success"].as_bool() == Some(false) {
        return Ok(None);
    }
    let Some(list) = stats["achievements"].as_array() else {
        return Ok(Some(Vec::new()));
    };
    Ok(Some(
        list.iter()
            .filter(|a| a["achieved"].as_i64() == Some(1))
            .filter_map(|a| {
                Some(Unlock {
                    api_name: non_empty(&a["apiname"])?,
                    name: non_empty(&a["name"]),
                    description: non_empty(&a["description"]),
                    unlocked_at: a["unlocktime"].as_i64().filter(|t| *t > 0),
                })
            })
            .collect(),
    ))
}

/// Joins definitions, rarity and unlocks; unlocked sorted by unlock time, newest first
/// (unknown times last). Api names match without case (the community XML writes them lowercase).
pub fn build_achievements(definitions: &Definitions, unlocks: &[Unlock]) -> Achievements {
    let key = |name: &str| name.to_ascii_lowercase();
    let by_name: HashMap<String, &SchemaAchievement> = definitions
        .achievements
        .iter()
        .map(|s| (key(&s.api_name), s))
        .collect();
    let percents: HashMap<String, f32> = definitions
        .percents
        .iter()
        .map(|(name, percent)| (key(name), *percent))
        .collect();
    let mut unlocked: Vec<Achievement> = unlocks
        .iter()
        .map(|u| {
            let def = by_name.get(&key(&u.api_name));
            Achievement {
                name: def
                    .and_then(|d| d.display_name.clone())
                    .or_else(|| u.name.clone())
                    .unwrap_or_else(|| u.api_name.clone()),
                description: def
                    .and_then(|d| d.description.clone())
                    .or_else(|| u.description.clone()),
                icon: def.and_then(|d| d.icon.clone()),
                unlocked_at: u.unlocked_at,
                percent: percents.get(&key(&u.api_name)).copied(),
            }
        })
        .collect();
    unlocked.sort_by_key(|a| std::cmp::Reverse(a.unlocked_at));
    Achievements {
        total: u32::try_from(definitions.achievements.len()).unwrap_or(u32::MAX),
        unlocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn player_count_parses_and_handles_missing_data() {
        assert_eq!(
            parse_player_count(&fixture("players_620.json")).unwrap(),
            Some(2215)
        );
        assert_eq!(
            parse_player_count(&fixture("players_none.json")).unwrap(),
            None
        );
        assert!(parse_player_count("nope").is_err());
    }

    #[test]
    fn news_fixture_maps_items() {
        let news = parse_news("620", &fixture("news_620.json"))
            .unwrap()
            .unwrap();
        assert_eq!(
            news.all_url.as_deref(),
            Some("https://store.steampowered.com/news/app/620")
        );
        assert_eq!(news.items.len(), 5);
        let update = &news.items[1];
        assert_eq!(update.title, "Portal 2 - Update");
        assert_eq!(update.source, "Community Announcements");
        assert_eq!(update.date, Some(1790707799));
        assert!(update.url.starts_with("https://"));
        assert!(update.snippet.starts_with("An update has been released"));
        let pc_gamer = &news.items[2];
        assert!(!pc_gamer.snippet.contains('<'));
        assert!(pc_gamer.snippet.contains("Half-Life 2's cancelled"));
        // Spaces in external post links are escaped.
        assert!(news.items.iter().all(|i| !i.url.contains(' ')));
        let news730 = parse_news("730", &fixture("news_730.json"))
            .unwrap()
            .unwrap();
        assert!(!news730.items.is_empty());
        assert!(
            news730
                .items
                .iter()
                .all(|i| i.snippet.chars().count() <= 300)
        );
    }

    #[test]
    fn news_item_rules() {
        let body = r#"{"appnews":{"appid":1,"newsitems":[
          {"title":"No url","contents":"x","date":1},
          {"title":"Bad scheme","url":"ftp://x/y","contents":"x"},
          {"title":"","url":"https://x/empty"},
          {"title":"Author only","url":"https://x/a","author":" Kerry ","contents":"[b]Hi[/b]"},
          {"title":"Nothing","url":"http://x/n","contents":""}
        ]}}"#;
        let news = parse_news("1", body).unwrap().unwrap();
        assert_eq!(news.items.len(), 2);
        assert_eq!(news.items[0].source, "Kerry");
        assert_eq!(news.items[0].snippet, "Hi");
        assert_eq!(news.items[0].date, None);
        assert_eq!(news.items[1].source, "Steam");
        assert_eq!(news.items[1].image, None);
    }

    #[test]
    fn empty_news_is_none() {
        assert_eq!(parse_news("1", "{}").unwrap(), None);
        assert_eq!(
            parse_news("1", r#"{"appnews":{"appid":1,"newsitems":[]}}"#).unwrap(),
            None
        );
        // Every item unusable.
        assert_eq!(
            parse_news(
                "1",
                r#"{"appnews":{"newsitems":[{"title":"t","url":"javascript:x"}]}}"#
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn bbcode_is_stripped() {
        assert_eq!(
            clean_text(
                "[h1]Patch[/h1][list][*]Fixed [b]crash[/b][*]Added [url=https://x.y/z]map[/url][/list]",
                300
            ),
            "Patch Fixed crash Added map"
        );
        assert_eq!(clean_text("[IMG]https://x/a.png[/IMG]Hello", 300), "Hello");
        assert_eq!(
            clean_text("[previewyoutube=abc;full][/previewyoutube]Text", 300),
            "Text"
        );
        // Not BBCode: stays.
        assert_eq!(
            clean_text("Version [1.2] [PATCH] ok", 300),
            "Version [1.2] [PATCH] ok"
        );
    }

    #[test]
    fn html_is_stripped_and_entities_decoded() {
        assert_eq!(
            clean_text("<p>One</p><p>Two<br/>Three</p><a href=\"x\">link</a>.", 300),
            "One Two Three link."
        );
        assert_eq!(
            clean_text(
                "Tom &amp; Jerry &quot;q&quot; it&#39;s &lt;b&gt; a&nbsp;b &#x41; &bogus; &",
                300
            ),
            "Tom & Jerry \"q\" it's <b> a b A &bogus; &"
        );
        // Decoded markup is text, not a tag.
        assert_eq!(clean_text("&lt;b&gt;x&lt;/b&gt;", 300), "<b>x</b>");
    }

    #[test]
    fn cut_off_markup_at_the_end_is_dropped() {
        assert_eq!(clean_text("Text <a href=\"https://x/y?z", 300), "Text");
        assert_eq!(clean_text("Text [url=https://x/y", 300), "Text");
        assert_eq!(clean_text("Text [img]https://x/y.png", 300), "Text");
    }

    #[test]
    fn whitespace_is_collapsed() {
        assert_eq!(clean_text("  a \n\n\t b   c  ", 300), "a b c");
        assert_eq!(clean_text("", 300), "");
    }

    #[test]
    fn long_text_is_cut_at_a_word() {
        let text = "alpha beta gamma delta epsilon";
        assert_eq!(clean_text(text, 30), text);
        assert_eq!(clean_text(text, 14), "alpha beta…");
        assert_eq!(clean_text(text, 16), "alpha beta gamma…");
        // Exactly at a word end.
        assert_eq!(clean_text(text, 10), "alpha beta…");
        // No usable space: hard cut.
        assert_eq!(clean_text("abcdefghijkl", 5), "abcde…");
        // Multi-byte characters are counted as characters.
        assert_eq!(clean_text("ääää ääää ääää", 8), "ääää…");
        // Trailing punctuation does not precede the ellipsis.
        assert_eq!(clean_text("one, two, three", 9), "one, two…");
    }

    #[test]
    fn first_image_from_bbcode_html_and_placeholder() {
        assert_eq!(
            first_image("Intro [img]https://x/a.png[/img] [img]https://x/b.png[/img]").as_deref(),
            Some("https://x/a.png")
        );
        assert_eq!(
            first_image(r#"<p>Hi <img class="c" src="https://x/h.jpg" width="1"> x"#).as_deref(),
            Some("https://x/h.jpg")
        );
        assert_eq!(
            first_image("<img src='http://x/s.jpg'>").as_deref(),
            Some("http://x/s.jpg")
        );
        assert_eq!(
            first_image("[img]{STEAM_CLAN_IMAGE}/3904/abc.png[/img]").as_deref(),
            Some("https://clan.akamai.steamstatic.com/images/3904/abc.png")
        );
        // The earlier of the two wins.
        assert_eq!(
            first_image(r#"<img src="https://x/html.png"> [img]https://x/bb.png[/img]"#).as_deref(),
            Some("https://x/html.png")
        );
        assert_eq!(first_image("[img]javascript:x[/img]"), None);
        assert_eq!(first_image("[img]data:image/png;base64,AA[/img]"), None);
        assert_eq!(first_image("no image here"), None);
    }

    #[test]
    fn news_images_come_from_contents() {
        let body = r#"{"appnews":{"newsitems":[{"title":"T","url":"https://x/t","feedlabel":"L",
          "contents":"[img]{STEAM_CLAN_IMAGE}/1/2.png[/img] Text","date":5}]}}"#;
        let news = parse_news("9", body).unwrap().unwrap();
        assert_eq!(
            news.items[0].image.as_deref(),
            Some("https://clan.akamai.steamstatic.com/images/1/2.png")
        );
        assert_eq!(news.items[0].snippet, "Text");
    }

    #[test]
    fn game_achievements_parse_with_icons_and_rarity() {
        let defs =
            parse_game_achievements(&fixture("game_achievements_1473350.json"), "1473350").unwrap();
        assert_eq!(defs.achievements.len(), 3);
        assert_eq!(
            defs.achievements[0].display_name.as_deref(),
            Some("A gnorp is born")
        );
        assert_eq!(
            defs.achievements[0].icon.as_deref(),
            Some(
                "https://shared.fastly.steamstatic.com/community_assets/images/apps/1473350/a4b8d6b0f3ab301b832801026de9a7d59b1941d1.jpg"
            )
        );
        // Empty descriptions and unreadable percentages are left out.
        assert_eq!(defs.achievements[2].description, None);
        assert_eq!(defs.percents.len(), 2);
        assert_eq!(defs.percents["GNORP_SLAM"], 19.9);
    }

    #[test]
    fn game_without_achievements_is_empty() {
        assert!(
            parse_game_achievements("{}", "1")
                .unwrap()
                .achievements
                .is_empty()
        );
        assert!(
            parse_game_achievements(r#"{"response":{}}"#, "1")
                .unwrap()
                .achievements
                .is_empty()
        );
        assert!(parse_game_achievements("<html>", "1").is_err());
        // Icon names that are not plain file names are dropped.
        let defs = parse_game_achievements(
            r#"{"response":{"achievements":[{"internal_name":"A","icon":"../x.jpg"}]}}"#,
            "1",
        )
        .unwrap();
        assert_eq!(defs.achievements[0].icon, None);
    }

    #[test]
    fn community_xml_lists_unlocked_achievements() {
        let unlocks =
            parse_community_achievements(&fixture("community_achievements_1473350.xml")).unwrap();
        assert_eq!(unlocks.len(), 2);
        assert_eq!(unlocks[0].api_name, "first_gnorp");
        assert_eq!(unlocks[0].unlocked_at, Some(1770142733));
        assert_eq!(unlocks[1].name.as_deref(), Some("Gnorp Slam & more"));
        assert_eq!(
            parse_community_achievements(&fixture("community_achievements_private.xml")),
            None
        );
        assert_eq!(parse_community_achievements("<html></html>"), None);
    }

    #[test]
    fn player_achievements_keep_unlocked_only() {
        let unlocks = parse_player_achievements(&fixture("player_achievements_620.json"))
            .unwrap()
            .unwrap();
        assert_eq!(unlocks.len(), 2);
        assert_eq!(unlocks[0].api_name, "ACH.SURVIVE_CONTAINER_RIDE");
        assert_eq!(unlocks[0].unlocked_at, Some(1500000000));
        assert_eq!(unlocks[1].description, None);
    }

    #[test]
    fn player_achievements_errors_are_none() {
        for name in [
            "player_achievements_nostats.json",
            "player_achievements_private.json",
        ] {
            assert_eq!(parse_player_achievements(&fixture(name)).unwrap(), None);
        }
        // Success without a list: nothing unlocked.
        assert_eq!(
            parse_player_achievements(r#"{"playerstats":{"success":true}}"#).unwrap(),
            Some(Vec::new())
        );
    }

    #[test]
    fn achievements_are_joined_and_sorted_newest_first() {
        let defs =
            parse_game_achievements(&fixture("game_achievements_1473350.json"), "1473350").unwrap();
        let unlocks =
            parse_community_achievements(&fixture("community_achievements_1473350.xml")).unwrap();
        let result = build_achievements(&defs, &unlocks);
        assert_eq!(result.total, 3);
        assert_eq!(result.unlocked.len(), 2);
        // Lowercase api names from the XML still find their definition and rarity.
        let newest = &result.unlocked[0];
        assert_eq!(newest.name, "Gnorp Slam!");
        assert_eq!(newest.percent, Some(19.9));
        assert!(
            newest
                .icon
                .as_deref()
                .unwrap()
                .ends_with("48f904a33d5823b594262799d86de649af72fa28.jpg")
        );
        let oldest = &result.unlocked[1];
        assert_eq!(oldest.name, "A gnorp is born");
        assert_eq!(oldest.description.as_deref(), Some("Get your first gnorp"));
    }

    #[test]
    fn unknown_unlocks_fall_back_and_sort_last() {
        let unlocks = vec![
            Unlock {
                api_name: "NO_TIME".into(),
                name: None,
                description: None,
                unlocked_at: None,
            },
            Unlock {
                api_name: "OLD".into(),
                name: Some("Old one".into()),
                description: Some("d".into()),
                unlocked_at: Some(10),
            },
        ];
        let result = build_achievements(&Definitions::default(), &unlocks);
        assert_eq!(result.unlocked[0].name, "Old one");
        assert_eq!(result.unlocked[0].description.as_deref(), Some("d"));
        assert_eq!(result.unlocked[1].name, "NO_TIME");
        assert_eq!(result.unlocked[1].percent, None);
    }

    #[test]
    fn invalid_app_ids_never_reach_the_network() {
        assert_eq!(fetch_player_count("").unwrap(), None);
        assert_eq!(fetch_player_count("12&x=1").unwrap(), None);
        assert_eq!(fetch_news("../1").unwrap(), None);
        assert_eq!(fetch_definitions("1 2").unwrap(), None);
        assert!(matches!(
            fetch_unlocks(Credential::Key("k"), "1", "1 2"),
            Ok(None)
        ));
        assert_eq!(fetch_community_unlocks("1/x", "620").unwrap(), None);
    }
}
