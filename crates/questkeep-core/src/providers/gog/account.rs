//! GOG account sign-in and the owned-games list.
//!
//! The OAuth client id and secret below are the public GOG Galaxy client credentials that
//! open-source launchers embed (Heroic's `gogdl`, `gogdl/auth.py`); they identify the client
//! application, not the user, and are not a secret. Only the facts (URLs, parameter and field
//! names) are used; no code is taken from those GPL projects.
//!
//! Fetching and parsing are separate, so every parser is tested with fixtures.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::super::ProviderError;
use super::GOG_SOURCE;
use crate::auth::{
    AccountAuth, AccountSession, AccountStatus, AuthError, Http, HttpRequest, HttpResponse,
    TokenSet, TokenStore, UreqHttp, plausible_code, query_param, system_clock, url_encode,
};
use crate::model::ImportedGame;

const CLIENT_ID: &str = "46899977096215655";
const CLIENT_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";
const REDIRECT_URI: &str = "https://embed.gog.com/on_login_success?origin=client";
const TOKEN_URL: &str = "https://auth.gog.com/token";
const USER_DATA_URL: &str = "https://embed.gog.com/userData.json";
const PRODUCTS_URL: &str = "https://embed.gog.com/account/getFilteredProducts";
/// Per-game playtime of the account (`/u/<username>/games/stats`, the data behind the profile's
/// games page): paged JSON with the minutes played and the last session of each game.
const STATS_URL: &str = "https://embed.gog.com/u";
/// The login redirect; the shell watches for navigations to this prefix.
pub const LOGIN_SUCCESS_PREFIX: &str = "https://embed.gog.com/on_login_success";
/// Upper bound for pages of the owned list (a guard against a server that never stops).
const MAX_PAGES: u32 = 200;

pub fn login_url() -> String {
    format!(
        "https://auth.gog.com/auth?client_id={CLIENT_ID}&redirect_uri={}&response_type=code&layout=client2",
        url_encode(REDIRECT_URI)
    )
}

/// The code from what the user pasted: the full `on_login_success?...&code=<code>` URL or just
/// the code.
pub fn extract_code(input: &str) -> Option<String> {
    let input = input.trim();
    match query_param(input, "code") {
        Some(code) if input.contains('=') => plausible_code(&code),
        _ => plausible_code(input),
    }
}

fn token_request(grant: &[(&str, &str)]) -> HttpRequest {
    let mut url = format!("{TOKEN_URL}?client_id={CLIENT_ID}&client_secret={CLIENT_SECRET}");
    for (key, value) in grant {
        url.push_str(&format!("&{key}={}", url_encode(value)));
    }
    HttpRequest::get(url)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    user_id: Option<Value>,
}

/// Maps a token (or refresh) response to tokens. `previous` supplies the refresh token and
/// account when the response omits them.
pub fn parse_token_response(
    body: &str,
    now: i64,
    previous: Option<&TokenSet>,
) -> Result<TokenSet, AuthError> {
    let unreadable = || AuthError::Parse("GOG returned an unexpected sign-in response.".into());
    let parsed: TokenResponse = serde_json::from_str(body).map_err(|_| unreadable())?;
    let refresh_token = parsed
        .refresh_token
        .filter(|t| !t.is_empty())
        .or_else(|| previous.map(|p| p.refresh_token.clone()))
        .ok_or_else(unreadable)?;
    if parsed.access_token.is_empty() {
        return Err(unreadable());
    }
    let account_id = match parsed.user_id {
        Some(Value::String(s)) if !s.is_empty() => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => previous.and_then(|p| p.account_id.clone()),
    };
    Ok(TokenSet {
        access_token: parsed.access_token,
        refresh_token,
        expires_at: now + parsed.expires_in.unwrap_or(3600),
        account_id,
        display_name: previous.and_then(|p| p.display_name.clone()),
    })
}

fn token_status_error(response: &HttpResponse, what: &str) -> AuthError {
    match response.status {
        400 | 401 | 403 => AuthError::Rejected(what.to_string()),
        status => AuthError::Network(format!("GOG returned HTTP {status}")),
    }
}

/// `RefreshFn` for [`AccountSession`].
pub fn refresh_tokens(http: &dyn Http, old: &TokenSet, now: i64) -> Result<TokenSet, AuthError> {
    let request = token_request(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &old.refresh_token),
    ]);
    let response = http.send(&request).map_err(AuthError::Network)?;
    if !response.ok() {
        return Err(token_status_error(
            &response,
            "GOG no longer accepts the saved sign-in.",
        ));
    }
    parse_token_response(&response.body, now, Some(old))
}

/// The account name from `userData.json`.
pub fn parse_user_name(body: &str) -> Option<String> {
    let json: Value = serde_json::from_str(body).ok()?;
    json.get("username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// One page of `getFilteredProducts`: games and the total page count.
pub fn parse_filtered_products(body: &str) -> Result<(Vec<ImportedGame>, u32), ProviderError> {
    let json: Value = serde_json::from_str(body).map_err(|_| {
        ProviderError::Parse("GOG returned an unexpected response for the game list.".into())
    })?;
    let products = json
        .get("products")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ProviderError::Parse("GOG returned an unexpected response for the game list.".into())
        })?;
    let total_pages = json
        .get("totalPages")
        .and_then(Value::as_u64)
        .and_then(|p| u32::try_from(p).ok())
        .unwrap_or(1);
    let games = products
        .iter()
        .filter_map(|product| {
            let id = match product.get("id")? {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => return None,
            };
            let title = product.get("title")?.as_str()?.trim();
            (super::api::is_product_id(&id) && !title.is_empty()).then(|| ImportedGame {
                installed: false,
                ..ImportedGame::installed(id, title)
            })
        })
        .collect();
    Ok((games, total_pages))
}

/// Playtime of one game from the stats list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playtime {
    pub seconds: u64,
    pub last_played: Option<DateTime<Utc>>,
}

fn parse_session_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%z"))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// One page of the games stats list: playtime per product id (games without playtime are left
/// out) and the page count. GOG answers `"stats": []` for never-played games and an object keyed
/// by account id otherwise; with several entries the largest playtime wins.
pub fn parse_games_stats(body: &str) -> Result<(HashMap<String, Playtime>, u32), ProviderError> {
    let unreadable = || {
        ProviderError::Parse("GOG returned an unexpected response for the playtime list.".into())
    };
    let json: Value = serde_json::from_str(body).map_err(|_| unreadable())?;
    let items = json
        .pointer("/_embedded/items")
        .and_then(Value::as_array)
        .ok_or_else(unreadable)?;
    let pages = json
        .get("pages")
        .and_then(Value::as_u64)
        .and_then(|p| u32::try_from(p).ok())
        .unwrap_or(1);
    let mut out: HashMap<String, Playtime> = HashMap::new();
    for item in items {
        let id = match item.pointer("/game/id") {
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };
        let Some(stats) = item.get("stats").and_then(Value::as_object) else {
            continue;
        };
        for entry in stats.values() {
            let minutes = entry.get("playtime").and_then(Value::as_u64).unwrap_or(0);
            if minutes == 0 {
                continue;
            }
            let candidate = Playtime {
                seconds: minutes * 60,
                last_played: entry
                    .get("lastSession")
                    .and_then(Value::as_str)
                    .and_then(parse_session_time),
            };
            let best = out.entry(id.clone()).or_insert(candidate);
            if candidate.seconds > best.seconds {
                *best = candidate;
            }
        }
    }
    Ok((out, pages))
}

/// A GOG account: sign-in, status and the owned games.
pub struct GogAccount {
    session: AccountSession,
    http: Arc<dyn Http>,
}

impl std::fmt::Debug for GogAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GogAccount").finish_non_exhaustive()
    }
}

impl GogAccount {
    /// Uses the real network and system clock.
    pub fn new(tokens: Arc<dyn TokenStore>) -> Self {
        Self::with(tokens, Arc::new(UreqHttp::new()), system_clock())
    }

    /// Injectable network and clock (tests).
    pub fn with(
        tokens: Arc<dyn TokenStore>,
        http: Arc<dyn Http>,
        clock: crate::auth::Clock,
    ) -> Self {
        Self {
            session: AccountSession::new(
                GOG_SOURCE,
                "GOG",
                tokens,
                http.clone(),
                clock,
                refresh_tokens,
            ),
            http,
        }
    }

    /// The owned games (all pages), not installed.
    pub fn fetch_owned(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        let mut games: Vec<ImportedGame> = Vec::new();
        let mut page = 1;
        loop {
            let url = format!("{PRODUCTS_URL}?mediaType=1&sortBy=title&page={page}");
            let response = self
                .session
                .send(&|token| HttpRequest::get(url.clone()).bearer(token))?;
            let (mut batch, total_pages) = parse_filtered_products(&response.body)?;
            games.append(&mut batch);
            if page >= total_pages.min(MAX_PAGES) {
                break;
            }
            page += 1;
        }
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        games.dedup_by(|a, b| a.source_game_id == b.source_game_id);
        let stats = self.fetch_playtime().unwrap_or_else(|err| {
            tracing::warn!(%err, "could not fetch GOG playtime");
            HashMap::new()
        });
        for game in &mut games {
            if let Some(time) = stats.get(&game.source_game_id) {
                game.playtime_seconds = Some(time.seconds);
                game.last_played = time.last_played;
            }
        }
        Ok(games)
    }

    /// Playtime per product id. Best effort: the caller imports the library without it on any
    /// failure. Sent without `AccountSession::send` on purpose: a 401 there signs the user out,
    /// and this is a side endpoint that must never cost the sign-in.
    fn fetch_playtime(&self) -> Result<HashMap<String, Playtime>, ProviderError> {
        let token = self.session.access_token()?;
        let username = match self.session.display_name() {
            Some(name) => name,
            None => self
                .fetch_user_name(&token)
                .ok_or_else(|| ProviderError::NotFound("GOG account name unknown".into()))?,
        };
        let mut all = HashMap::new();
        let mut page = 1;
        loop {
            let url = format!(
                "{STATS_URL}/{}/games/stats?sort=recent_playtime&order=desc&page={page}",
                url_encode(&username)
            );
            let response = self
                .http
                .send(&HttpRequest::get(url).bearer(&token))
                .map_err(ProviderError::Network)?;
            if !response.ok() {
                return Err(ProviderError::Network(format!(
                    "GOG returned HTTP {} for the playtime list",
                    response.status
                )));
            }
            let (batch, pages) = parse_games_stats(&response.body)?;
            all.extend(batch);
            if page >= pages.min(MAX_PAGES) {
                break;
            }
            page += 1;
        }
        Ok(all)
    }

    pub fn is_signed_in(&self) -> bool {
        self.session.is_signed_in()
    }

    /// The account name; best effort, so a failure here never blocks the sign-in.
    fn fetch_user_name(&self, access_token: &str) -> Option<String> {
        let response = self
            .http
            .send(&HttpRequest::get(USER_DATA_URL).bearer(access_token))
            .ok()?;
        response.ok().then(|| parse_user_name(&response.body))?
    }
}

impl AccountAuth for GogAccount {
    fn status(&self) -> AccountStatus {
        AccountStatus {
            provider: GOG_SOURCE.into(),
            name: "GOG".into(),
            signed_in: self.session.is_signed_in(),
            display_name: self.session.display_name(),
            supports_code_sign_in: true,
            code_url: Some(login_url()),
        }
    }

    fn login_url(&self) -> String {
        login_url()
    }

    fn sign_in_with_code(&self, input: &str) -> Result<(), AuthError> {
        let code = extract_code(input).ok_or_else(|| {
            AuthError::Rejected(
                "No GOG sign-in code found. Paste the code or the whole address you were sent to after signing in."
                    .into(),
            )
        })?;
        let request = token_request(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
        ]);
        let response = self.http.send(&request).map_err(AuthError::Network)?;
        if !response.ok() {
            return Err(token_status_error(
                &response,
                "GOG did not accept that code. Codes work once and expire quickly; sign in again to get a new one.",
            ));
        }
        let mut tokens = parse_token_response(&response.body, self.session.now(), None)?;
        tokens.display_name = self.fetch_user_name(&tokens.access_token);
        self.session.sign_in(tokens)
    }

    fn sign_out(&self) -> Result<(), AuthError> {
        self.session.sign_out()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::auth::MemoryTokenStore;
    use crate::auth::testing::{FakeHttp, response};

    const TOKEN_FIXTURE: &str = include_str!("../../../tests/fixtures/gog/token.json");
    const PRODUCTS_FIXTURE: &str =
        include_str!("../../../tests/fixtures/gog/filtered_products.json");

    #[test]
    fn login_url_matches_the_galaxy_client_flow() {
        assert_eq!(
            login_url(),
            "https://auth.gog.com/auth?client_id=46899977096215655&redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient&response_type=code&layout=client2"
        );
    }

    #[test]
    fn extracts_codes_from_urls_and_pastes() {
        let url = "https://embed.gog.com/on_login_success?origin=client&code=AbC123_xyz-9";
        assert_eq!(extract_code(url).as_deref(), Some("AbC123_xyz-9"));
        assert_eq!(
            extract_code("  AbC123_xyz-9\n").as_deref(),
            Some("AbC123_xyz-9")
        );
        assert_eq!(
            extract_code("code=AbC123_xyz-9").as_deref(),
            Some("AbC123_xyz-9")
        );
        assert_eq!(
            extract_code("https://embed.gog.com/on_login_success?origin=client"),
            None
        );
        assert_eq!(extract_code(""), None);
        assert_eq!(extract_code("two words here"), None);
    }

    #[test]
    fn parses_token_response() {
        let tokens = parse_token_response(TOKEN_FIXTURE, 1_000, None).unwrap();
        assert_eq!(tokens.access_token, "gog-access-token-value");
        assert_eq!(tokens.refresh_token, "gog-refresh-token-value");
        assert_eq!(tokens.expires_at, 1_000 + 3600);
        assert_eq!(tokens.account_id.as_deref(), Some("48628349107"));
        assert!(parse_token_response("{}", 0, None).is_err());
        assert!(parse_token_response("<html>", 0, None).is_err());
        let prev = TokenSet {
            access_token: String::new(),
            refresh_token: "old-refresh".into(),
            expires_at: 0,
            account_id: Some("old".into()),
            display_name: Some("Ann".into()),
        };
        let kept = parse_token_response(r#"{"access_token":"a","expires_in":60}"#, 0, Some(&prev))
            .unwrap();
        assert_eq!(kept.refresh_token, "old-refresh");
        assert_eq!(kept.account_id.as_deref(), Some("old"));
        assert_eq!(kept.display_name.as_deref(), Some("Ann"));
    }

    #[test]
    fn parses_user_name() {
        assert_eq!(
            parse_user_name(r#"{"username":" Ann ","email":"x"}"#).as_deref(),
            Some("Ann")
        );
        assert_eq!(parse_user_name(r#"{"username":""}"#), None);
        assert_eq!(parse_user_name("nope"), None);
    }

    #[test]
    fn parses_filtered_products() {
        let (games, pages) = parse_filtered_products(PRODUCTS_FIXTURE).unwrap();
        assert_eq!(pages, 2);
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Cyberpunk 2077", "The Witcher: Enhanced Edition"]);
        assert_eq!(games[0].source_game_id, "1423049311");
        assert!(
            games
                .iter()
                .all(|g| !g.installed && g.play_action.is_none())
        );
        assert!(parse_filtered_products("{}").is_err());
        assert!(parse_filtered_products("nope").is_err());
        let (none, pages) = parse_filtered_products(r#"{"products":[]}"#).unwrap();
        assert!(none.is_empty() && pages == 1);
    }

    const STATS_FIXTURE: &str = include_str!("../../../tests/fixtures/gog/games_stats.json");

    #[test]
    fn parses_games_stats() {
        let (stats, pages) = parse_games_stats(STATS_FIXTURE).unwrap();
        assert_eq!(pages, 1);
        let cp = stats["1423049311"];
        assert_eq!(cp.seconds, 125 * 60);
        assert_eq!(
            cp.last_played.map(|d| d.to_rfc3339()).as_deref(),
            Some("2024-03-05T18:30:12+00:00")
        );
        assert!(!stats.contains_key("1207658924"), "zero minutes");
        assert!(
            !stats.contains_key("1207658930"),
            "empty array means never played"
        );
        assert_eq!(stats["1456460669"].seconds, 30 * 60, "largest entry wins");
        assert!(stats["1456460669"].last_played.is_some());
        assert!(parse_games_stats("{}").is_err());
        assert!(parse_games_stats("nope").is_err());
    }

    type StatsAnswer = fn(&HttpRequest) -> Result<HttpResponse, String>;

    fn signed_in_account(stats: StatsAnswer) -> (GogAccount, Arc<FakeHttp>) {
        let (account, store, http) = make_account(move |req| {
            if req.url.starts_with(TOKEN_URL) {
                response(200, TOKEN_FIXTURE)
            } else if req.url.starts_with(PRODUCTS_URL) {
                response(
                    200,
                    r#"{"totalPages":1,"products":[{"id":1423049311,"title":"Cyberpunk 2077"},{"id":5,"title":"Other"}]}"#,
                )
            } else if req.url.starts_with(STATS_URL) {
                stats(req)
            } else {
                response(404, "")
            }
        });
        store
            .save(
                "gog",
                r#"{"refreshToken":"gog-refresh-token-value","displayName":"Ann B"}"#,
            )
            .unwrap();
        (account, http)
    }

    #[test]
    fn owned_scan_adds_playtime() {
        let (account, http) = signed_in_account(|_| response(200, STATS_FIXTURE));
        let games = account.fetch_owned().unwrap();
        assert_eq!(games[0].source_game_id, "1423049311");
        assert_eq!(games[0].playtime_seconds, Some(7_500));
        assert!(games[0].last_played.is_some());
        assert_eq!(games[1].playtime_seconds, None);
        assert!(http.urls().iter().any(|u| u
            == "https://embed.gog.com/u/Ann%20B/games/stats?sort=recent_playtime&order=desc&page=1"));
    }

    #[test]
    fn owned_scan_survives_a_playtime_failure() {
        let failures: [StatsAnswer; 4] = [
            |_| response(403, ""),
            |_| response(401, ""),
            |_| response(200, "<html>"),
            |_| Err("offline".into()),
        ];
        for failing in failures {
            let (account, _) = signed_in_account(failing);
            let games = account.fetch_owned().unwrap();
            assert_eq!(games.len(), 2);
            assert!(games.iter().all(|g| g.playtime_seconds.is_none()));
            assert!(
                account.is_signed_in(),
                "a refused stats call keeps the sign-in"
            );
        }
    }

    fn make_account(
        handler: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
    ) -> (GogAccount, Arc<MemoryTokenStore>, Arc<FakeHttp>) {
        let store = Arc::new(MemoryTokenStore::new());
        let http = Arc::new(FakeHttp::new(handler));
        let account = GogAccount::with(store.clone(), http.clone(), Arc::new(|| 1_000));
        (account, store, http)
    }

    #[test]
    fn code_sign_in_exchanges_and_stores_tokens() {
        let (account, store, http) = make_account(|req| {
            if req.url.starts_with(TOKEN_URL) {
                response(200, TOKEN_FIXTURE)
            } else if req.url == USER_DATA_URL {
                response(200, r#"{"username":"Ann"}"#)
            } else {
                response(404, "")
            }
        });
        assert!(!account.status().signed_in);
        account
            .sign_in_with_code(
                "https://embed.gog.com/on_login_success?origin=client&code=AbC123_xyz-9",
            )
            .unwrap();
        let status = account.status();
        assert!(status.signed_in && status.supports_code_sign_in);
        assert_eq!(status.display_name.as_deref(), Some("Ann"));
        assert_eq!(status.code_url.as_deref(), Some(login_url().as_str()));
        let stored = store.load("gog").unwrap().unwrap();
        assert!(stored.contains("gog-refresh-token-value"));
        assert!(!stored.contains("gog-access-token-value"));
        let token_url = &http.urls()[0];
        assert!(token_url.contains("grant_type=authorization_code"));
        assert!(token_url.contains("code=AbC123_xyz-9"));
        assert!(token_url.contains(
            "redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient"
        ));

        account.sign_out().unwrap();
        assert!(!account.status().signed_in);
        assert_eq!(store.load("gog").unwrap(), None);
    }

    #[test]
    fn sign_in_errors_are_readable_and_secret_free() {
        let (account, _, _) = make_account(|_| response(400, r#"{"error":"invalid_grant"}"#));
        let err = account.sign_in_with_code("AbC123_xyz-9").unwrap_err();
        assert!(matches!(&err, AuthError::Rejected(m) if m.contains("did not accept")));
        assert!(!err.to_string().contains("AbC123"));
        let err = account.sign_in_with_code("nonsense words").unwrap_err();
        assert!(err.to_string().contains("No GOG sign-in code"));
        let (account, _, _) = make_account(|_| response(503, ""));
        assert!(matches!(
            account.sign_in_with_code("AbC123_xyz-9"),
            Err(AuthError::Network(_))
        ));
    }

    #[test]
    fn user_name_failure_does_not_block_sign_in() {
        let (account, _, _) = make_account(|req| {
            if req.url.starts_with(TOKEN_URL) {
                response(200, TOKEN_FIXTURE)
            } else {
                Err("offline".into())
            }
        });
        account.sign_in_with_code("AbC123_xyz-9").unwrap();
        let status = account.status();
        assert!(status.signed_in);
        assert_eq!(status.display_name, None);
    }

    #[test]
    fn refresh_uses_the_refresh_grant() {
        let http = FakeHttp::new(|req| {
            assert!(
                req.url
                    .contains("grant_type=refresh_token&refresh_token=R%2F1")
            );
            response(200, TOKEN_FIXTURE)
        });
        let old = TokenSet {
            access_token: String::new(),
            refresh_token: "R/1".into(),
            expires_at: 0,
            account_id: None,
            display_name: Some("Ann".into()),
        };
        let fresh = refresh_tokens(&http, &old, 10).unwrap();
        assert_eq!(fresh.refresh_token, "gog-refresh-token-value");
        assert_eq!(fresh.display_name.as_deref(), Some("Ann"));
        let rejected = FakeHttp::new(|_| response(400, ""));
        assert!(matches!(
            refresh_tokens(&rejected, &old, 10),
            Err(AuthError::Rejected(_))
        ));
        let down = FakeHttp::new(|_| response(502, ""));
        assert!(matches!(
            refresh_tokens(&down, &old, 10),
            Err(AuthError::Network(_))
        ));
    }

    #[test]
    fn owned_scan_pages_through_the_list() {
        let pages = Arc::new(AtomicUsize::new(0));
        let seen = pages.clone();
        let (account, store, http) = make_account(move |req| {
            if req.url.starts_with(TOKEN_URL) {
                return response(200, TOKEN_FIXTURE);
            }
            assert_eq!(req.headers[0].1, "Bearer gog-access-token-value");
            if req.url.starts_with(STATS_URL) {
                return response(404, "");
            }
            seen.fetch_add(1, Ordering::SeqCst);
            if req.url.ends_with("page=1") {
                response(200, PRODUCTS_FIXTURE)
            } else {
                response(
                    200,
                    r#"{"totalPages":2,"products":[{"id":1207658924,"title":"Alpha"},{"id":1423049311,"title":"Cyberpunk 2077"}]}"#,
                )
            }
        });
        // A restored session (no access token yet) refreshes first.
        store
            .save("gog", r#"{"refreshToken":"gog-refresh-token-value"}"#)
            .unwrap();
        let games = account.fetch_owned().unwrap();
        assert_eq!(pages.load(Ordering::SeqCst), 2);
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["Alpha", "Cyberpunk 2077", "The Witcher: Enhanced Edition"],
            "sorted and de-duplicated"
        );
        assert!(
            http.urls()
                .iter()
                .any(|u| u.contains("mediaType=1&sortBy=title&page=2"))
        );
    }

    #[test]
    fn owned_scan_when_signed_out_is_not_configured() {
        let (account, _, _) = make_account(|_| response(200, ""));
        assert!(matches!(
            account.fetch_owned(),
            Err(ProviderError::NotConfigured(m)) if m == "Sign in to GOG in Settings."
        ));
    }
}
