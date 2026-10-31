//! HowLongToBeat client, the fallback source for "time to beat" when IGDB has none.
//!
//! HowLongToBeat has no official API, so this follows what its own website does (checked against
//! the live site, Oct 2026; the flow has changed several times before, hence the defensive
//! handling below):
//!
//! 1. `GET https://howlongtobeat.com/api/search/site/init?t=<ms>` answers `{"token":"..."}`. The
//!    token is tied to the caller's IP and `User-Agent`, and expires after a while.
//! 2. `POST https://howlongtobeat.com/api/search/site` with the JSON body
//!    `{"searchType":"games","searchTerms":[word,...],"searchPage":1,"size":20}` and the header
//!    `x-auth-token: <token>` answers `{"data":[{"game_name","game_alias","release_world",
//!    "comp_main","comp_plus","comp_100",...}]}` with times in seconds (0 = unknown).
//!
//! Both calls need `Referer: https://howlongtobeat.com/` (without it: 404 or 403) and the same
//! `User-Agent`. A 403 means the token expired: a new one is fetched and the search repeated once.
//!
//! Failures of the flow itself (unexpected reply, changed endpoints) are logged and read as "no
//! data", so the game page just hides the section; network errors are returned so the lookup is
//! retried later. Parsing and matching are pure functions, tested against a captured reply.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;

use super::fetcher::http_agent;
use super::igdb::match_key;
use crate::error::{CoreError, Result};
use crate::model::TimeToBeat;

const SITE: &str = "https://howlongtobeat.com/";
const INIT_URL: &str = "https://howlongtobeat.com/api/search/site/init";
const SEARCH_URL: &str = "https://howlongtobeat.com/api/search/site";
/// The site rejects non-browser agents' fingerprints, so a browser-like one is sent (the token is
/// bound to it, which is why every call uses this same value).
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
/// Minimum time between two requests, across all threads.
const MIN_INTERVAL: Duration = Duration::from_millis(1000);

/// Times to beat in seconds; `None` where HowLongToBeat has no data. At least one is `Some`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HltbTimes {
    /// Main story ("Main Story").
    pub main: Option<u32>,
    /// Main plus extras ("Main + Extra").
    pub extra: Option<u32>,
    /// Completionist ("100%").
    pub complete: Option<u32>,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: u64,
    pub name: String,
    pub alias: String,
    pub year: Option<i32>,
    pub times: HltbTimes,
}

/// Shared handle to HowLongToBeat. Cheap to clone.
#[derive(Clone)]
pub struct Hltb {
    inner: Arc<Inner>,
}

struct Inner {
    http: ureq::Agent,
    /// The search token, kept until the site rejects it.
    token: Mutex<Option<String>>,
    /// Earliest instant the next request may start.
    next_slot: Mutex<Instant>,
    interval: Duration,
    init_url: String,
    search_url: String,
}

impl std::fmt::Debug for Hltb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hltb").finish_non_exhaustive()
    }
}

impl Default for Hltb {
    fn default() -> Self {
        Self::new()
    }
}

/// How one attempt at the search ended.
enum Attempt {
    Reply(String),
    /// The site refused the token or the request shape: fetch a new token and try again.
    Refused(u16),
}

impl Hltb {
    pub fn new() -> Self {
        Self::with_endpoints(MIN_INTERVAL, INIT_URL.into(), SEARCH_URL.into())
    }

    pub(crate) fn with_endpoints(interval: Duration, init_url: String, search_url: String) -> Self {
        Self {
            inner: Arc::new(Inner {
                http: http_agent(),
                token: Mutex::new(None),
                next_slot: Mutex::new(Instant::now()),
                interval,
                init_url,
                search_url,
            }),
        }
    }

    /// Looks the game up and returns the times of the best match. `Ok(None)`: no matching game,
    /// no times for it, or the site's flow changed (logged). `Err`: network trouble, retry later.
    pub fn time_to_beat(&self, name: &str, release_year: Option<i32>) -> Result<Option<HltbTimes>> {
        let terms = search_terms(name);
        if terms.is_empty() || match_key(name).is_empty() {
            return Ok(None);
        }
        let body = search_body(&terms);
        let mut last_refusal = 0;
        for _ in 0..2 {
            let Some(token) = self.token()? else {
                return Ok(None);
            };
            match self.search(&token, &body)? {
                Attempt::Reply(text) => {
                    return Ok(match parse_search(&text) {
                        Ok(hits) => best_match(&hits, name, release_year).map(|hit| hit.times),
                        Err(reason) => {
                            tracing::warn!("HowLongToBeat: {reason}; ignoring it");
                            None
                        }
                    });
                }
                Attempt::Refused(status) => {
                    last_refusal = status;
                    self.forget_token();
                }
            }
        }
        tracing::warn!(
            "HowLongToBeat refused the search twice (HTTP {last_refusal}); its API probably changed"
        );
        Ok(None)
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

    fn forget_token(&self) {
        *self
            .inner
            .token
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// The cached token, or a new one. `Ok(None)`: the site's init endpoint is gone or odd.
    fn token(&self) -> Result<Option<String>> {
        let mut cached = self
            .inner
            .token
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(token) = cached.as_ref() {
            return Ok(Some(token.clone()));
        }
        self.throttle();
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let result = self
            .inner
            .http
            .get(format!("{}?t={millis}", self.inner.init_url))
            .header("User-Agent", USER_AGENT)
            .header("Referer", SITE)
            .call();
        let text = match result {
            Ok(response) => response
                .into_body()
                .read_to_string()
                .map_err(|err| CoreError::Network(format!("HowLongToBeat: {err}")))?,
            Err(ureq::Error::StatusCode(429)) => return Err(rate_limited()),
            Err(ureq::Error::StatusCode(status)) if (400..500).contains(&status) => {
                tracing::warn!(
                    "HowLongToBeat token request answered HTTP {status}; its API probably changed"
                );
                return Ok(None);
            }
            Err(err) => return Err(CoreError::Network(format!("HowLongToBeat: {err}"))),
        };
        match parse_token(&text) {
            Some(token) => {
                *cached = Some(token.clone());
                Ok(Some(token))
            }
            None => {
                tracing::warn!("HowLongToBeat sent no search token; its API probably changed");
                Ok(None)
            }
        }
    }

    fn search(&self, token: &str, body: &str) -> Result<Attempt> {
        self.throttle();
        let result = self
            .inner
            .http
            .post(&self.inner.search_url)
            .header("User-Agent", USER_AGENT)
            .header("Referer", SITE)
            .header("Content-Type", "application/json")
            .header("x-auth-token", token)
            .send(body);
        match result {
            Ok(response) => response
                .into_body()
                .read_to_string()
                .map(Attempt::Reply)
                .map_err(|err| CoreError::Network(format!("HowLongToBeat: {err}"))),
            Err(ureq::Error::StatusCode(429)) => Err(rate_limited()),
            Err(ureq::Error::StatusCode(status @ (400 | 401 | 403 | 404))) => {
                Ok(Attempt::Refused(status))
            }
            Err(err) => Err(CoreError::Network(format!("HowLongToBeat: {err}"))),
        }
    }
}

fn rate_limited() -> CoreError {
    CoreError::Network("HowLongToBeat rate limit reached, try again later.".into())
}

/// The words sent as search terms: trademark signs removed, split at whitespace.
fn search_terms(name: &str) -> Vec<String> {
    name.chars()
        .filter(|c| !matches!(c, '\u{2122}' | '\u{00AE}' | '\u{00A9}') && !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn search_body(terms: &[String]) -> String {
    json!({
        "searchType": "games",
        "searchTerms": terms,
        "searchPage": 1,
        "size": 20,
    })
    .to_string()
}

/// The token of an init reply (`{"token":"..."}`), if it is there and non-empty.
pub(crate) fn parse_token(body: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Reply {
        token: String,
    }
    serde_json::from_str::<Reply>(body)
        .ok()
        .map(|r| r.token)
        .filter(|t| !t.trim().is_empty())
}

#[derive(Debug, Deserialize)]
struct SearchReply {
    data: Vec<RawHit>,
}

#[derive(Debug, Deserialize)]
struct RawHit {
    #[serde(default)]
    game_id: u64,
    game_name: String,
    #[serde(default)]
    game_alias: String,
    #[serde(default)]
    release_world: i32,
    #[serde(default)]
    comp_main: i64,
    #[serde(default)]
    comp_plus: i64,
    #[serde(default)]
    comp_100: i64,
}

/// Seconds of a time field; zero and negative values mean "unknown".
fn seconds(value: i64) -> Option<u32> {
    u32::try_from(value).ok().filter(|v| *v > 0)
}

/// The hits of a search reply, in the site's order (by relevance). An error says why the reply
/// could not be read.
pub(crate) fn parse_search(body: &str) -> std::result::Result<Vec<Hit>, String> {
    let reply: SearchReply =
        serde_json::from_str(body).map_err(|err| format!("unexpected search reply ({err})"))?;
    Ok(reply
        .data
        .into_iter()
        .map(|raw| Hit {
            id: raw.game_id,
            name: raw.game_name,
            alias: raw.game_alias,
            year: Some(raw.release_world).filter(|y| *y > 0),
            times: HltbTimes {
                main: seconds(raw.comp_main),
                extra: seconds(raw.comp_plus),
                complete: seconds(raw.comp_100),
            },
        })
        .collect())
}

impl From<HltbTimes> for TimeToBeat {
    fn from(times: HltbTimes) -> Self {
        Self {
            main: times.main.map(u64::from),
            extra: times.extra.map(u64::from),
            complete: times.complete.map(u64::from),
        }
    }
}

impl HltbTimes {
    fn any(&self) -> bool {
        self.main.is_some() || self.extra.is_some() || self.complete.is_some()
    }
}

/// The hit with the same normalized name (or alias) as `name` that has times. With a known
/// `year`, only a hit released that year qualifies, else one released a year off; no equal name
/// or no fitting year means no match, because another game's times would be wrong.
pub(crate) fn best_match<'a>(hits: &'a [Hit], name: &str, year: Option<i32>) -> Option<&'a Hit> {
    let wanted = match_key(name);
    if wanted.is_empty() {
        return None;
    }
    let named: Vec<&Hit> = hits
        .iter()
        .filter(|h| h.times.any())
        .filter(|h| match_key(&h.name) == wanted || match_key(&h.alias) == wanted)
        .collect();
    let Some(year) = year else {
        return named.first().copied();
    };
    named
        .iter()
        .find(|h| h.year == Some(year))
        .or_else(|| {
            named
                .iter()
                .find(|h| h.year.is_some_and(|y| (y - year).abs() == 1))
        })
        .copied()
}

#[cfg(test)]
mod tests {
    use super::super::igdb::test_server::serve;
    use super::*;

    /// A real `POST /api/search/site` reply for "Hades" (first two hits).
    const HADES: &str = include_str!("fixtures/hltb_search.json");
    // Shape of a real `GET /api/search/site/init` reply (token shortened).
    const INIT: &str = r#"{"token":"MTc5MzQ2MDc5MDc5OTo6dG9rZW4"}"#;

    fn hit(name: &str, year: Option<i32>, main: u32) -> Hit {
        Hit {
            id: 1,
            name: name.into(),
            alias: String::new(),
            year,
            times: HltbTimes {
                main: Some(main),
                ..HltbTimes::default()
            },
        }
    }

    fn client(base: &str) -> Hltb {
        Hltb::with_endpoints(
            Duration::ZERO,
            format!("{base}/api/search/site/init"),
            format!("{base}/api/search/site"),
        )
    }

    #[test]
    fn parses_a_real_search_reply() {
        let hits = parse_search(HADES).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "Hades");
        assert_eq!(hits[0].id, 62941);
        assert_eq!(hits[0].year, Some(2020));
        assert_eq!(
            hits[0].times,
            HltbTimes {
                main: Some(84907),
                extra: Some(175076),
                complete: Some(343228)
            }
        );
        assert_eq!(hits[1].name, "Hades II");
        assert_eq!(hits[1].alias, "Hades 2");
    }

    #[test]
    fn zero_times_are_unknown_and_garbage_is_an_error() {
        let hits = parse_search(
            r#"{"data":[{"game_name":"X","comp_main":0,"comp_plus":-1,"comp_100":900,"release_world":0}]}"#,
        )
        .unwrap();
        assert_eq!(hits[0].year, None);
        assert_eq!(hits[0].times.main, None);
        assert_eq!(hits[0].times.extra, None);
        assert_eq!(hits[0].times.complete, Some(900));
        assert!(parse_search("<html>nope</html>").is_err());
        assert!(parse_search(r#"{"error":"Session expired"}"#).is_err());
        assert_eq!(parse_search(r#"{"data":[]}"#).unwrap(), vec![]);
    }

    #[test]
    fn parses_the_token() {
        assert_eq!(
            parse_token(INIT).as_deref(),
            Some("MTc5MzQ2MDc5MDc5OTo6dG9rZW4")
        );
        assert_eq!(parse_token(r#"{"token":" "}"#), None);
        assert_eq!(parse_token("{}"), None);
        assert_eq!(parse_token("not json"), None);
    }

    #[test]
    fn matches_by_name_and_year() {
        let hits = parse_search(HADES).unwrap();
        assert_eq!(best_match(&hits, "HADES", Some(2020)).unwrap().id, 62941);
        assert_eq!(best_match(&hits, "Hades", None).unwrap().id, 62941);
        // The alias counts as a name.
        assert_eq!(
            best_match(&hits, "Hades 2", Some(2025)).unwrap().name,
            "Hades II"
        );
        // Similar names are not matches.
        assert!(best_match(&hits, "Hade", None).is_none());
        assert!(best_match(&hits, "", None).is_none());
        // A year far off is no match.
        assert!(best_match(&hits, "Hades", Some(2010)).is_none());
    }

    #[test]
    fn year_rules() {
        let hits = vec![
            hit("Doom", Some(2016), 10),
            hit("Doom", Some(1993), 20),
            hit("Doom", Some(2017), 30),
        ];
        assert_eq!(
            best_match(&hits, "Doom", None).unwrap().times.main,
            Some(10)
        );
        assert_eq!(
            best_match(&hits, "Doom", Some(1993)).unwrap().times.main,
            Some(20)
        );
        // An exact year beats a neighbouring one that is listed first.
        assert_eq!(
            best_match(&hits, "Doom", Some(2017)).unwrap().times.main,
            Some(30)
        );
        // Off by one only without an exact year.
        assert_eq!(
            best_match(&hits[..1], "Doom", Some(2017))
                .unwrap()
                .times
                .main,
            Some(10)
        );
        assert!(best_match(&hits, "Doom", Some(2005)).is_none());
        // Unknown hit year does not satisfy a known one.
        assert!(best_match(&[hit("Doom", None, 5)], "Doom", Some(2016)).is_none());
    }

    #[test]
    fn normalizes_names_and_skips_hits_without_times() {
        let hits = vec![
            Hit {
                times: HltbTimes::default(),
                ..hit("The Witcher 3: Wild Hunt", Some(2015), 0)
            },
            hit("Witcher 3 - Wild Hunt", Some(2015), 99),
        ];
        let found = best_match(&hits, "The Witcher\u{AE} 3: Wild Hunt\u{2122}", Some(2015));
        assert_eq!(found.unwrap().times.main, Some(99));
    }

    #[test]
    fn builds_the_search_body() {
        let terms = search_terms("The Witcher\u{2122} 3:  Wild Hunt");
        assert_eq!(terms, ["The", "Witcher", "3:", "Wild", "Hunt"]);
        let body: serde_json::Value = serde_json::from_str(&search_body(&terms)).unwrap();
        assert_eq!(body["searchType"], "games");
        assert_eq!(body["searchTerms"][1], "Witcher");
        assert_eq!(body["searchPage"], 1);
    }

    #[test]
    fn flow_scrapes_the_token_then_searches() {
        let server = serve(vec![(200, INIT), (200, HADES)]);
        let hltb = client(&server.base);
        let times = hltb.time_to_beat("Hades", Some(2020)).unwrap().unwrap();
        assert_eq!(times.main, Some(84907));
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].path.starts_with("/api/search/site/init?t="));
        assert_eq!(requests[0].header("referer"), Some(SITE));
        assert_eq!(requests[1].path, "/api/search/site");
        assert_eq!(
            requests[1].header("x-auth-token"),
            Some("MTc5MzQ2MDc5MDc5OTo6dG9rZW4")
        );
        assert_eq!(requests[1].header("referer"), Some(SITE));
        assert!(requests[1].body.contains(r#""searchTerms":["Hades"]"#));
    }

    #[test]
    fn caches_the_token_between_lookups() {
        let server = serve(vec![(200, INIT), (200, HADES), (200, HADES)]);
        let hltb = client(&server.base);
        hltb.time_to_beat("Hades", None).unwrap();
        hltb.time_to_beat("Hades", None).unwrap();
        assert_eq!(server.requests.lock().unwrap().len(), 3);
    }

    #[test]
    fn an_expired_token_is_renewed_once() {
        let server = serve(vec![
            (200, INIT),
            (403, r#"{"error":"Session expired or invalid fingerprint"}"#),
            (200, r#"{"token":"second"}"#),
            (200, HADES),
        ]);
        let hltb = client(&server.base);
        let times = hltb.time_to_beat("Hades", Some(2020)).unwrap();
        assert_eq!(times.unwrap().main, Some(84907));
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[3].header("x-auth-token"), Some("second"));
    }

    #[test]
    fn gives_up_after_one_renewal() {
        let server = serve(vec![(200, INIT), (404, "{}"), (200, INIT), (404, "{}")]);
        let hltb = client(&server.base);
        assert_eq!(hltb.time_to_beat("Hades", None).unwrap(), None);
        assert_eq!(server.requests.lock().unwrap().len(), 4);
    }

    #[test]
    fn flow_failures_read_as_no_data_and_network_errors_are_errors() {
        // Unreadable search reply.
        let server = serve(vec![(200, INIT), (200, "<html></html>")]);
        assert_eq!(
            client(&server.base).time_to_beat("Hades", None).unwrap(),
            None
        );
        // Init endpoint gone.
        let server = serve(vec![(404, "{}")]);
        assert_eq!(
            client(&server.base).time_to_beat("Hades", None).unwrap(),
            None
        );
        // Init without a token.
        let server = serve(vec![(200, "{}")]);
        assert_eq!(
            client(&server.base).time_to_beat("Hades", None).unwrap(),
            None
        );
        // Server trouble and rate limits are retried later.
        let server = serve(vec![(500, "{}")]);
        assert!(client(&server.base).time_to_beat("Hades", None).is_err());
        let server = serve(vec![(200, INIT), (429, "{}")]);
        assert!(client(&server.base).time_to_beat("Hades", None).is_err());
        // Nothing listening.
        let hltb = client("http://127.0.0.1:1");
        assert!(hltb.time_to_beat("Hades", None).is_err());
        // A blank name costs no request.
        assert_eq!(hltb.time_to_beat("  \u{2122} ", None).unwrap(), None);
    }

    #[test]
    fn unmatched_games_have_no_times() {
        let server = serve(vec![(200, INIT), (200, HADES)]);
        let found = client(&server.base)
            .time_to_beat("Something Else", None)
            .unwrap();
        assert_eq!(found, None);
    }
}
