//! IGDB client (<https://api-docs.igdb.com>), used as a fallback metadata source (description,
//! genres, developer, publisher, release date) for games whose store provides none: manual games
//! and stores without a public API.
//!
//! IGDB is owned by Twitch and needs a Twitch application's Client ID and Client Secret. They are
//! user-provided, shared behind a lock so changing them in settings takes effect for the running
//! worker, and only ever sent to Twitch (token) and IGDB (queries). The app access token is kept
//! in memory only and refreshed on expiry or when IGDB answers 401. All API calls go through one
//! global limiter (IGDB allows 4 requests per second).
//!
//! Parsing and matching are separate from fetching so they can be tested against fixtures.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike};
use serde::Deserialize;

use super::fetcher::http_agent;
use super::steamgriddb::encode;
use crate::error::{CoreError, Result};
use crate::model::GameMetadata;

const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const GAMES_URL: &str = "https://api.igdb.com/v4/games";
/// Minimum time between two API requests, across all threads (IGDB allows 4 per second).
const MIN_INTERVAL: Duration = Duration::from_millis(300);
/// A token is renewed this long before it expires.
const TOKEN_MARGIN: Duration = Duration::from_secs(60);
const FIELDS: &str = "name,summary,first_release_date,genres.name,\
    involved_companies.developer,involved_companies.publisher,involved_companies.company.name";

const REJECTED: &str = "The IGDB (Twitch) Client ID or Client Secret was rejected.";

/// `games.extra` key recording that IGDB was asked for this game and had nothing, so it isn't
/// asked again on every start. Value: RFC 3339 timestamp.
pub(crate) const CHECKED_KEY: &str = "igdbCheckedAt";

/// Identifies a game for lookup.
#[derive(Debug, Clone, Copy)]
pub struct IgdbLookup<'a> {
    pub name: &'a str,
    /// Release year when the game has one (used to tell same-named games apart).
    pub year: Option<i32>,
}

#[derive(Clone, PartialEq, Eq)]
struct Credentials {
    client_id: String,
    client_secret: String,
}

struct Token {
    value: String,
    expires: Instant,
}

/// Shared handle to IGDB. Cheap to clone.
#[derive(Clone)]
pub struct Igdb {
    inner: Arc<Inner>,
}

struct Inner {
    http: ureq::Agent,
    credentials: RwLock<Option<Credentials>>,
    token: Mutex<Option<Token>>,
    /// Set when Twitch or IGDB rejected the credentials, so a bad pair costs one request instead
    /// of one per queued game. Cleared when the credentials change.
    rejected: AtomicBool,
    /// Earliest instant the next request may start.
    next_slot: Mutex<Instant>,
    interval: Duration,
    token_url: String,
    games_url: String,
}

impl std::fmt::Debug for Igdb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the credentials or the token.
        f.debug_struct("Igdb")
            .field("configured", &self.is_configured())
            .finish()
    }
}

impl Igdb {
    pub fn new(client_id: Option<String>, client_secret: Option<String>) -> Self {
        Self::with_endpoints(
            client_id,
            client_secret,
            MIN_INTERVAL,
            TOKEN_URL.into(),
            GAMES_URL.into(),
        )
    }

    pub(crate) fn with_endpoints(
        client_id: Option<String>,
        client_secret: Option<String>,
        interval: Duration,
        token_url: String,
        games_url: String,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                http: http_agent(),
                credentials: RwLock::new(normalize(client_id, client_secret)),
                token: Mutex::new(None),
                rejected: AtomicBool::new(false),
                next_slot: Mutex::new(Instant::now()),
                interval,
                token_url,
                games_url,
            }),
        }
    }

    /// Replaces the credentials (called when settings are saved). Both must be non-blank,
    /// otherwise IGDB counts as not configured. Unchanged credentials keep the token and a
    /// "rejected" verdict.
    pub fn set_credentials(&self, client_id: Option<String>, client_secret: Option<String>) {
        let next = normalize(client_id, client_secret);
        let mut current = self
            .inner
            .credentials
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if *current == next {
            return;
        }
        *current = next;
        *self
            .inner
            .token
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        self.inner.rejected.store(false, Ordering::Relaxed);
    }

    /// Whether credentials are set and have not been rejected since they were entered.
    pub fn is_configured(&self) -> bool {
        self.credentials().is_some() && !self.inner.rejected.load(Ordering::Relaxed)
    }

    fn credentials(&self) -> Option<Credentials> {
        self.inner
            .credentials
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn reject(&self) -> CoreError {
        self.inner.rejected.store(true, Ordering::Relaxed);
        CoreError::Invalid(REJECTED.into())
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

    /// The cached app access token, or a new one from Twitch.
    fn token(&self, credentials: &Credentials) -> Result<String> {
        let mut cached = self
            .inner
            .token
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(token) = cached.as_ref()
            && token.expires > Instant::now()
        {
            return Ok(token.value.clone());
        }
        // The secret goes in the form body, not the URL, so it can't end up in logs.
        let body = token_request_body(credentials);
        let result = self
            .inner
            .http
            .post(&self.inner.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body);
        let text = match result {
            Ok(response) => response
                .into_body()
                .read_to_string()
                .map_err(|err| CoreError::Network(format!("Twitch: {err}")))?,
            // Twitch: 400 "invalid client" (unknown id), 403 "invalid client secret".
            Err(ureq::Error::StatusCode(400 | 401 | 403)) => return Err(self.reject()),
            Err(ureq::Error::StatusCode(429)) => {
                return Err(CoreError::Network(
                    "Twitch rate limit reached, try again later.".into(),
                ));
            }
            Err(err) => return Err(CoreError::Network(format!("Twitch: {err}"))),
        };
        let (value, lifetime) = parse_token(&text)?;
        *cached = Some(Token {
            value: value.clone(),
            expires: Instant::now() + lifetime.saturating_sub(TOKEN_MARGIN),
        });
        Ok(value)
    }

    /// POSTs an Apicalypse `body` to the games endpoint. Renews the token once on 401.
    fn query(&self, body: &str) -> Result<String> {
        let credentials = self.credentials().ok_or_else(|| {
            CoreError::Invalid("Add IGDB (Twitch) credentials in the settings first.".into())
        })?;
        for attempt in 0..2 {
            if self.inner.rejected.load(Ordering::Relaxed) {
                return Err(CoreError::Invalid(REJECTED.into()));
            }
            let token = self.token(&credentials)?;
            self.throttle();
            let result = self
                .inner
                .http
                .post(&self.inner.games_url)
                .header("Client-ID", &credentials.client_id)
                .header("Authorization", format!("Bearer {token}"))
                .header("Accept", "application/json")
                .send(body);
            match result {
                Ok(response) => {
                    return response
                        .into_body()
                        .read_to_string()
                        .map_err(|err| CoreError::Network(format!("IGDB: {err}")));
                }
                Err(ureq::Error::StatusCode(401)) => {
                    // Expired or revoked token: get a new one and try once more.
                    *self
                        .inner
                        .token
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = None;
                    if attempt == 1 {
                        return Err(self.reject());
                    }
                }
                Err(ureq::Error::StatusCode(403)) => return Err(self.reject()),
                Err(ureq::Error::StatusCode(429)) => {
                    return Err(CoreError::Network(
                        "IGDB rate limit reached, try again later.".into(),
                    ));
                }
                Err(err) => return Err(CoreError::Network(format!("IGDB: {err}"))),
            }
        }
        Err(self.reject())
    }

    /// Searches IGDB for the game and returns the best match's metadata. `Ok(None)` means IGDB
    /// has no matching game (or none with any data); errors are transient or credential problems
    /// and should not be remembered as "checked".
    pub fn fetch_metadata(&self, lookup: &IgdbLookup<'_>) -> Result<Option<GameMetadata>> {
        if search_text(lookup.name).is_empty() {
            return Ok(None);
        }
        let body = search_body(lookup.name);
        let hits = parse_games(&self.query(&body)?)?;
        Ok(best_match(&hits, lookup).and_then(to_metadata))
    }
}

fn normalize(client_id: Option<String>, client_secret: Option<String>) -> Option<Credentials> {
    let trimmed = |v: Option<String>| v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    Some(Credentials {
        client_id: trimmed(client_id)?,
        client_secret: trimmed(client_secret)?,
    })
}

fn token_request_body(credentials: &Credentials) -> String {
    format!(
        "client_id={}&client_secret={}&grant_type=client_credentials",
        encode(&credentials.client_id),
        encode(&credentials.client_secret)
    )
}

/// The name as sent to IGDB's search: quotes, backslashes and control characters (which would
/// break the Apicalypse string) removed.
fn search_text(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '"' | '\\') && !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

pub(crate) fn search_body(name: &str) -> String {
    format!(
        "search \"{}\"; fields {FIELDS}; limit 10;",
        search_text(name)
    )
}

#[derive(Debug, Deserialize)]
struct TokenReply {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

/// The token and its lifetime.
fn parse_token(body: &str) -> Result<(String, Duration)> {
    let reply: TokenReply = serde_json::from_str(body)
        .map_err(|err| CoreError::Network(format!("Twitch sent an unexpected reply: {err}")))?;
    if reply.access_token.is_empty() {
        return Err(CoreError::Network("Twitch sent an empty token.".into()));
    }
    Ok((reply.access_token, Duration::from_secs(reply.expires_in)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct Game {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    /// Unix seconds.
    #[serde(default)]
    pub first_release_date: Option<i64>,
    #[serde(default)]
    pub genres: Vec<Named>,
    #[serde(default)]
    pub involved_companies: Vec<InvolvedCompany>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct Named {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct InvolvedCompany {
    #[serde(default)]
    pub developer: bool,
    #[serde(default)]
    pub publisher: bool,
    #[serde(default)]
    pub company: Option<Named>,
}

pub(crate) fn parse_games(body: &str) -> Result<Vec<Game>> {
    serde_json::from_str(body)
        .map_err(|err| CoreError::Network(format!("IGDB sent an unexpected reply: {err}")))
}

/// Lowercase letters and digits only, `&` read as "and", a leading "the" dropped: "The Witcher 3:
/// Wild Hunt" and "witcher 3 - wild hunt" are the same game.
pub(crate) fn match_key(name: &str) -> String {
    let lowered = name.to_lowercase().replace('&', " and ");
    let words: Vec<String> = lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let skip = usize::from(words.first().is_some_and(|w| w == "the") && words.len() > 1);
    words[skip..].concat()
}

fn year_of(game: &Game) -> Option<i32> {
    DateTime::from_timestamp(game.first_release_date?, 0).map(|d| d.year())
}

/// The search hit with the same normalized name; among several, the one released in the wanted
/// year, else the first (IGDB lists by relevance). No equal name means no match: a loosely
/// similar game would put wrong data on the page.
pub(crate) fn best_match<'a>(hits: &'a [Game], lookup: &IgdbLookup<'_>) -> Option<&'a Game> {
    let wanted = match_key(lookup.name);
    if wanted.is_empty() {
        return None;
    }
    let mut exact = hits.iter().filter(|g| match_key(&g.name) == wanted);
    let first = exact.next()?;
    let Some(year) = lookup.year else {
        return Some(first);
    };
    std::iter::once(first)
        .chain(exact)
        .find(|g| year_of(g) == Some(year))
        .or(Some(first))
}

/// `None` when IGDB knows the game but has nothing to show for it.
pub(crate) fn to_metadata(game: &Game) -> Option<GameMetadata> {
    let clean = |s: &str| s.trim().to_string();
    let companies = |pick: fn(&InvolvedCompany) -> bool| {
        let mut names: Vec<String> = Vec::new();
        for company in game.involved_companies.iter().filter(|c| pick(c)) {
            let name = company.company.as_ref().map(|c| clean(&c.name));
            if let Some(name) = name.filter(|n| !n.is_empty())
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        names
    };
    let mut genres: Vec<String> = Vec::new();
    for genre in &game.genres {
        let name = clean(&genre.name);
        if !name.is_empty() && !genres.contains(&name) {
            genres.push(name);
        }
    }
    let meta = GameMetadata {
        description: game.summary.as_deref().map(clean).filter(|s| !s.is_empty()),
        release_date: game
            .first_release_date
            .and_then(|t| DateTime::from_timestamp(t, 0))
            .map(|d| d.format("%Y-%m-%d").to_string()),
        developers: companies(|c| c.developer),
        publishers: companies(|c| c.publisher),
        genres,
    };
    (meta != GameMetadata::default()).then_some(meta)
}

/// Year of a stored release date (`YYYY`, `YYYY-MM` or `YYYY-MM-DD`).
pub(crate) fn release_year(release_date: Option<&str>) -> Option<i32> {
    release_date?.get(..4)?.parse().ok()
}

/// A tiny HTTP server for tests: answers requests in order with canned replies and records them.
#[cfg(test)]
pub(crate) mod test_server {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// One request as received.
    #[derive(Debug, Clone)]
    pub struct Received {
        pub path: String,
        /// Lowercased header names with their values.
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Received {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        }
    }

    /// `(status, body)` per request, in order; extra requests get 500.
    pub struct Server {
        pub base: String,
        pub requests: Arc<Mutex<Vec<Received>>>,
    }

    pub fn serve(replies: Vec<(u16, &'static str)>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        std::thread::spawn(move || {
            let mut replies = replies.into_iter();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                let (head_end, length) = loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break (data.len(), 0);
                    }
                    data.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&data).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        break (end + 4, length);
                    }
                };
                while data.len() < head_end + length {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                }
                let text = String::from_utf8_lossy(&data).to_string();
                let (head, body) = text.split_at(head_end.min(text.len()));
                let mut lines = head.lines();
                let path = lines
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or_default()
                    .to_string();
                let headers = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
                    .collect();
                log.lock().unwrap().push(Received {
                    path,
                    headers,
                    body: body.to_string(),
                });
                let (status, reply) = replies.next().unwrap_or((500, "{}"));
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Server { base, requests }
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::serve;
    use super::*;

    // Shape of a real `POST /v4/games` reply for the query above (trimmed).
    const WITCHER: &str = r#"[
      {"id":1942,"name":"The Witcher 3: Wild Hunt",
       "summary":"  You are Geralt of Rivia.  ","first_release_date":1431993600,
       "genres":[{"id":12,"name":"Role-playing (RPG)"},{"id":31,"name":"Adventure"},{"id":12,"name":"Role-playing (RPG)"}],
       "involved_companies":[
         {"id":1,"developer":true,"publisher":false,"company":{"id":10,"name":"CD Projekt RED"}},
         {"id":2,"developer":false,"publisher":true,"company":{"id":11,"name":"CD Projekt"}},
         {"id":3,"developer":true,"publisher":true,"company":{"id":10,"name":"CD Projekt RED"}},
         {"id":4,"developer":false,"publisher":false}]},
      {"id":2000,"name":"The Witcher 3: Wild Hunt - Blood and Wine","first_release_date":1464134400}
    ]"#;

    fn game(name: &str, year_ts: Option<i64>) -> Game {
        Game {
            name: name.into(),
            first_release_date: year_ts,
            ..Game::default()
        }
    }

    fn lookup(name: &str) -> IgdbLookup<'_> {
        IgdbLookup { name, year: None }
    }

    #[test]
    fn builds_the_search_query() {
        let body = search_body("Say \"Hi\" \\ there\n");
        assert!(body.starts_with("search \"Say Hi  there\"; fields name,summary,"));
        assert!(body.contains("involved_companies.company.name"));
        assert!(body.ends_with("limit 10;"));
    }

    #[test]
    fn token_body_is_form_encoded_and_secret_never_in_the_url() {
        let body = token_request_body(&Credentials {
            client_id: "id1".into(),
            client_secret: "s&cret/=".into(),
        });
        assert_eq!(
            body,
            "client_id=id1&client_secret=s%26cret%2F%3D&grant_type=client_credentials"
        );
        assert!(!TOKEN_URL.contains("secret"));
    }

    #[test]
    fn parses_the_token() {
        let (token, life) =
            parse_token(r#"{"access_token":"abc","expires_in":5587808,"token_type":"bearer"}"#)
                .unwrap();
        assert_eq!(token, "abc");
        assert_eq!(life, Duration::from_secs(5_587_808));
        assert!(parse_token(r#"{"status":400,"message":"invalid client"}"#).is_err());
        assert!(parse_token(r#"{"access_token":""}"#).is_err());
    }

    #[test]
    fn parses_games_into_metadata() {
        let games = parse_games(WITCHER).unwrap();
        assert_eq!(games.len(), 2);
        let meta = to_metadata(&games[0]).unwrap();
        assert_eq!(
            meta.description.as_deref(),
            Some("You are Geralt of Rivia.")
        );
        assert_eq!(meta.release_date.as_deref(), Some("2015-05-19"));
        assert_eq!(meta.developers, vec!["CD Projekt RED"]);
        assert_eq!(meta.publishers, vec!["CD Projekt", "CD Projekt RED"]);
        assert_eq!(meta.genres, vec!["Role-playing (RPG)", "Adventure"]);
        assert!(parse_games("[]").unwrap().is_empty());
        assert!(parse_games("<html>").is_err());
        // Only a name: nothing to show.
        assert!(to_metadata(&Game::default()).is_none());
    }

    #[test]
    fn normalizes_names_for_matching() {
        assert_eq!(
            match_key("The Witcher 3: Wild Hunt"),
            match_key("witcher 3 - wild hunt")
        );
        assert_eq!(match_key("Tom & Jerry"), match_key("Tom and Jerry"));
        assert_eq!(match_key("Half-Life 2™"), "halflife2");
        assert_eq!(match_key("Half-Life 2"), "halflife2");
        assert_eq!(match_key("The"), "the", "a lone article is a name");
        assert_eq!(match_key("  "), "");
    }

    #[test]
    fn picks_exact_names_only_and_breaks_ties_by_year() {
        let hits = vec![
            game("Doom 3: Resurrection of Evil", None),
            game("DOOM", Some(1_463_000_000)), // 2016
            game("Doom", Some(756_000_000)),   // 1993
            game("Doom", Some(1_100_000_000)), // 2004
        ];
        // No year: the first exact match in IGDB's order.
        assert_eq!(
            best_match(&hits, &lookup("doom"))
                .unwrap()
                .first_release_date,
            Some(1_463_000_000)
        );
        // With a year: the one released then.
        let in_1993 = IgdbLookup {
            name: "Doom",
            year: Some(1993),
        };
        assert_eq!(
            best_match(&hits, &in_1993).unwrap().first_release_date,
            Some(756_000_000)
        );
        // A year nobody has: fall back to the first exact name.
        let other = IgdbLookup {
            name: "Doom",
            year: Some(1850),
        };
        assert_eq!(
            best_match(&hits, &other).unwrap().first_release_date,
            Some(1_463_000_000)
        );
        // Loosely similar is not a match.
        assert!(best_match(&hits, &lookup("Doom 3")).is_none());
        assert!(best_match(&[], &lookup("x")).is_none());
        assert!(best_match(&hits, &lookup("!!")).is_none());
    }

    #[test]
    fn reads_the_year_of_a_release_date() {
        assert_eq!(release_year(Some("2015-05-19")), Some(2015));
        assert_eq!(release_year(Some("2015")), Some(2015));
        assert_eq!(release_year(Some("x")), None);
        assert_eq!(release_year(None), None);
    }

    #[test]
    fn credential_handling_and_debug_do_not_leak() {
        let igdb = Igdb::new(Some("id".into()), Some("  ".into()));
        assert!(!igdb.is_configured(), "both are needed");
        igdb.set_credentials(Some(" id ".into()), Some(" top-secret ".into()));
        assert!(igdb.is_configured());
        assert_eq!(igdb.credentials().unwrap().client_secret, "top-secret");
        assert!(!format!("{igdb:?}").contains("secret"));
        igdb.set_credentials(None, Some("x".into()));
        assert!(!igdb.is_configured());
        assert!(matches!(
            igdb.fetch_metadata(&lookup("x")),
            Err(CoreError::Invalid(_))
        ));
    }

    #[test]
    fn throttle_spaces_requests() {
        let igdb = Igdb::with_endpoints(
            None,
            None,
            Duration::from_millis(40),
            String::new(),
            String::new(),
        );
        let start = Instant::now();
        igdb.throttle();
        igdb.throttle();
        igdb.throttle();
        assert!(start.elapsed() >= Duration::from_millis(80));
    }

    fn client(server: &str) -> Igdb {
        Igdb::with_endpoints(
            Some("my-id".into()),
            Some("my-secret".into()),
            Duration::ZERO,
            format!("{server}/token"),
            format!("{server}/games"),
        )
    }

    const TOKEN: &str = r#"{"access_token":"tok1","expires_in":3600,"token_type":"bearer"}"#;
    const TOKEN2: &str = r#"{"access_token":"tok2","expires_in":3600,"token_type":"bearer"}"#;

    #[test]
    fn fetches_with_the_right_requests_and_reuses_the_token() {
        let server = serve(vec![(200, TOKEN), (200, WITCHER), (200, "[]")]);
        let igdb = client(&server.base);
        let meta = igdb
            .fetch_metadata(&lookup("The Witcher 3: Wild Hunt"))
            .unwrap()
            .unwrap();
        assert_eq!(meta.developers, vec!["CD Projekt RED"]);
        // An empty result list is "no data", not an error; the token is reused.
        assert_eq!(igdb.fetch_metadata(&lookup("Nothing")).unwrap(), None);

        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 3, "one token request, two searches");
        assert_eq!(requests[0].path, "/token");
        assert_eq!(
            requests[0].body,
            "client_id=my-id&client_secret=my-secret&grant_type=client_credentials"
        );
        assert_eq!(requests[1].path, "/games");
        assert_eq!(requests[1].header("client-id"), Some("my-id"));
        assert_eq!(requests[1].header("authorization"), Some("Bearer tok1"));
        assert!(
            requests[1]
                .body
                .starts_with("search \"The Witcher 3: Wild Hunt\";")
        );
    }

    #[test]
    fn a_401_renews_the_token_once() {
        let server = serve(vec![
            (200, TOKEN),
            (401, "{}"),
            (200, TOKEN2),
            (200, WITCHER),
        ]);
        let igdb = client(&server.base);
        let found = igdb
            .fetch_metadata(&lookup("The Witcher 3: Wild Hunt"))
            .unwrap();
        assert!(found.is_some());
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[3].header("authorization"), Some("Bearer tok2"));
        assert!(igdb.is_configured());
    }

    #[test]
    fn rejected_credentials_cost_one_request_until_they_change() {
        let server = serve(vec![(
            403,
            r#"{"status":403,"message":"invalid client secret"}"#,
        )]);
        let igdb = client(&server.base);
        assert!(matches!(
            igdb.fetch_metadata(&lookup("Doom")),
            Err(CoreError::Invalid(_))
        ));
        assert!(!igdb.is_configured());
        assert!(igdb.fetch_metadata(&lookup("Doom")).is_err());
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        // The same credentials stay rejected; new ones are tried again.
        igdb.set_credentials(Some("my-id".into()), Some("my-secret".into()));
        assert!(!igdb.is_configured());
        igdb.set_credentials(Some("my-id".into()), Some("other".into()));
        assert!(igdb.is_configured());
    }

    #[test]
    fn network_and_rate_limit_errors_are_not_rejections() {
        let server = serve(vec![(200, TOKEN), (429, "{}")]);
        let igdb = client(&server.base);
        assert!(matches!(
            igdb.fetch_metadata(&lookup("Doom")),
            Err(CoreError::Network(_))
        ));
        assert!(igdb.is_configured());
    }
}
