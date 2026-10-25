//! Steam web access: owned games (Web API, with the user's key or a web session token) and
//! store metadata. Fetching
//! and parsing are separate so the parsers are tested with fixtures and never touch the network.

use chrono::DateTime;
use serde_json::Value;

use super::super::ProviderError;
use crate::model::{GameMetadata, ImportedGame, PlayAction};

/// Why a request failed, without any URL (which may contain the API key).
enum HttpFailure {
    Status(u16),
    Other(String),
}

fn http_get(url: &str, secret: Option<&str>) -> Result<String, HttpFailure> {
    let http = crate::images::http_agent();
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

/// Fetches store metadata; `Ok(None)` when the store has no data for the app.
pub fn fetch_metadata(app_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
    if app_id.is_empty() || !app_id.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let url = format!("https://store.steampowered.com/api/appdetails?appids={app_id}&l=english");
    match http_get(&url, None) {
        Ok(body) => parse_app_details(app_id, &body),
        Err(HttpFailure::Status(429)) => Err(ProviderError::Network("rate limited".into())),
        Err(HttpFailure::Status(code)) => Err(ProviderError::Network(format!(
            "Steam store returned HTTP {code}"
        ))),
        Err(HttpFailure::Other(message)) => Err(ProviderError::Network(message)),
    }
}

/// Percent-encodes everything except unreserved characters.
fn encode(text: &str) -> String {
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

    Ok(Some(GameMetadata {
        description,
        release_date,
        developers: strings("developers"),
        publishers: strings("publishers"),
        genres,
    }))
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
