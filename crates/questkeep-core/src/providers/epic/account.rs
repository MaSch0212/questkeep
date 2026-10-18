//! Epic account sign-in, the owned library, catalog lookups and playtime.
//!
//! The OAuth client id and secret are the public Epic Games Launcher client credentials that
//! open-source launchers embed (Legendary's `legendary/api/egs.py`, Playnite); they identify the
//! client application, not the user. Only facts (URLs, parameter and field names) are used; no
//! code is taken from those GPL projects.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Deserialize;
use serde_json::Value;

use super::super::ProviderError;
use super::EPIC_SOURCE;
use super::web::{self, CatalogItem, LibraryRecord};
use crate::auth::{
    AccountAuth, AccountSession, AccountStatus, AuthError, Clock, Http, HttpRequest, HttpResponse,
    TokenSet, TokenStore, UreqHttp, plausible_code, system_clock, url_encode,
};
use crate::model::ImportedGame;

/// `base64("<client id>:<client secret>")` for the launcher's public client.
const BASIC_AUTH: &str = "basic MzRhMDJjZjhmNDQxNGUyOWIxNTkyMTg3NmRhMzZmOWE6ZGFhZmJjY2M3Mzc3NDUwMzlkZmZlNTNkOTRmYzc2Y2Y=";
const TOKEN_URL: &str =
    "https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/token";
const LIBRARY_URL: &str = "https://library-service.live.use1a.on.epicgames.com/library/api/public/items?includeMetadata=true&platform=Windows";
const PLAYTIME_URL: &str =
    "https://library-service.live.use1a.on.epicgames.com/library/api/public/playtime/account";
const CATALOG_URL: &str =
    "https://catalog-public-service-prod06.ol.epicgames.com/catalog/api/shared/namespace";
/// Where a logged-in browser gets the one-time code (JSON with `authorizationCode`).
pub const REDIRECT_URL: &str = "https://www.epicgames.com/id/api/redirect?clientId=34a02cf8f4414e29b15921876da36f9a&responseType=code";
/// The login window shows this before redirecting to [`REDIRECT_URL`].
const LOGIN_PAGE: &str = "https://www.epicgames.com/id/login";
/// Catalog ids per request; keeps URLs short.
const CATALOG_BATCH: usize = 40;
/// Guards against a cursor loop on the server side.
const MAX_LIBRARY_PAGES: usize = 500;

/// The page the login window opens: Epic's login, then straight on to the code redirect.
pub fn login_url() -> String {
    format!("{LOGIN_PAGE}?redirectUrl={}", url_encode(REDIRECT_URL))
}

/// The code from what the user pasted: the page's JSON (with `authorizationCode`) or the bare
/// code.
pub fn extract_code(input: &str) -> Option<String> {
    let input = input.trim();
    if input.starts_with('{') {
        let json: Value = serde_json::from_str(input).ok()?;
        return plausible_code(json.get("authorizationCode")?.as_str()?);
    }
    plausible_code(input)
}

fn token_request(grant: &[(&str, &str)]) -> HttpRequest {
    let mut form: Vec<(String, String)> = grant
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    form.push(("token_type".into(), "eg1".into()));
    HttpRequest::post_form(TOKEN_URL, form).header("Authorization", BASIC_AUTH)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
}

/// Maps a token (or refresh) response to tokens; `previous` fills what the response omits.
pub fn parse_token_response(
    body: &str,
    now: i64,
    previous: Option<&TokenSet>,
) -> Result<TokenSet, AuthError> {
    let unreadable = || AuthError::Parse("Epic returned an unexpected sign-in response.".into());
    let parsed: TokenResponse = serde_json::from_str(body).map_err(|_| unreadable())?;
    let refresh_token = parsed
        .refresh_token
        .filter(|t| !t.is_empty())
        .or_else(|| previous.map(|p| p.refresh_token.clone()))
        .ok_or_else(unreadable)?;
    if parsed.access_token.is_empty() {
        return Err(unreadable());
    }
    Ok(TokenSet {
        access_token: parsed.access_token,
        refresh_token,
        expires_at: now + parsed.expires_in.unwrap_or(3600),
        account_id: parsed
            .account_id
            .filter(|s| !s.is_empty())
            .or_else(|| previous.and_then(|p| p.account_id.clone())),
        display_name: parsed
            .display_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| previous.and_then(|p| p.display_name.clone())),
    })
}

fn token_status_error(response: &HttpResponse, what: &str) -> AuthError {
    match response.status {
        400 | 401 | 403 => AuthError::Rejected(what.to_string()),
        status => AuthError::Network(format!("Epic returned HTTP {status}")),
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
            "Epic no longer accepts the saved sign-in.",
        ));
    }
    parse_token_response(&response.body, now, Some(old))
}

#[derive(Default)]
struct Cache {
    /// Candidate library records by lowercase appName; `None` until the library was fetched.
    library: Option<HashMap<String, LibraryRecord>>,
    /// Catalog items by id; `None` = the catalog does not know the item. Lives for one run.
    catalog: HashMap<String, Option<Arc<CatalogItem>>>,
}

/// An Epic account: sign-in, status, owned games and catalog data.
pub struct EpicAccount {
    session: AccountSession,
    http: Arc<dyn Http>,
    cache: Mutex<Cache>,
}

impl std::fmt::Debug for EpicAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpicAccount").finish_non_exhaustive()
    }
}

impl EpicAccount {
    /// Uses the real network and system clock.
    pub fn new(tokens: Arc<dyn TokenStore>) -> Self {
        Self::with(tokens, Arc::new(UreqHttp::new()), system_clock())
    }

    /// Injectable network and clock (tests).
    pub fn with(tokens: Arc<dyn TokenStore>, http: Arc<dyn Http>, clock: Clock) -> Self {
        Self {
            session: AccountSession::new(
                EPIC_SOURCE,
                "Epic Games",
                tokens,
                http.clone(),
                clock,
                refresh_tokens,
            ),
            http,
            cache: Mutex::new(Cache::default()),
        }
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_signed_in(&self) -> bool {
        self.session.is_signed_in()
    }

    /// All library pages; candidate records are remembered for later catalog lookups.
    fn fetch_library(&self) -> Result<Vec<LibraryRecord>, ProviderError> {
        let mut records: Vec<LibraryRecord> = Vec::new();
        let mut seen_cursors: HashSet<String> = HashSet::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_LIBRARY_PAGES {
            let url = match &cursor {
                Some(c) => format!("{LIBRARY_URL}&cursor={}", url_encode(c)),
                None => LIBRARY_URL.to_string(),
            };
            let response = self
                .session
                .send(&|token| HttpRequest::get(url.clone()).bearer(token))?;
            let (page, next) = web::parse_library_page(&response.body)?;
            records.extend(page.into_iter().filter(LibraryRecord::is_candidate));
            match next {
                Some(next) if seen_cursors.insert(next.clone()) => cursor = Some(next),
                _ => break,
            }
        }
        let mut unique: HashMap<String, LibraryRecord> = HashMap::new();
        for record in records {
            unique
                .entry(record.app_name.to_lowercase())
                .or_insert(record);
        }
        let mut list: Vec<LibraryRecord> = unique.values().cloned().collect();
        list.sort_by(|a, b| a.app_name.cmp(&b.app_name));
        self.cache().library = Some(unique);
        Ok(list)
    }

    /// Fetches the catalog items that are not cached yet, one request per namespace batch.
    fn fetch_catalog(&self, records: &[LibraryRecord]) -> Result<(), ProviderError> {
        let missing: BTreeMap<String, Vec<String>> = {
            let cache = self.cache();
            let mut by_namespace: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for record in records {
                if !cache.catalog.contains_key(&record.catalog_item_id) {
                    let ids = by_namespace.entry(record.namespace.clone()).or_default();
                    if !ids.contains(&record.catalog_item_id) {
                        ids.push(record.catalog_item_id.clone());
                    }
                }
            }
            by_namespace
        };
        for (namespace, ids) in missing {
            for batch in ids.chunks(CATALOG_BATCH) {
                let mut url = format!(
                    "{CATALOG_URL}/{}/bulk/items?country=US&locale=en-US&includeMainGameDetails=true",
                    url_encode(&namespace)
                );
                for id in batch {
                    url.push_str(&format!("&id={}", url_encode(id)));
                }
                let response = self
                    .session
                    .send(&|token| HttpRequest::get(url.clone()).bearer(token))?;
                let mut items = web::parse_catalog(&response.body)?;
                let mut cache = self.cache();
                for id in batch {
                    cache
                        .catalog
                        .insert(id.clone(), items.remove(id).map(Arc::new));
                }
            }
        }
        Ok(())
    }

    fn fetch_playtime(&self) -> Result<HashMap<String, u64>, ProviderError> {
        let Some(account_id) = self.session.account_id() else {
            return Ok(HashMap::new());
        };
        let url = format!("{PLAYTIME_URL}/{}/all", url_encode(&account_id));
        let response = self
            .session
            .send(&|token| HttpRequest::get(url.clone()).bearer(token))?;
        web::parse_playtime(&response.body)
    }

    /// The owned games (not installed): library entries that the catalog says are games.
    pub fn scan_owned(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        let records = self.fetch_library()?;
        self.fetch_catalog(&records)?;
        let playtime = self.fetch_playtime().unwrap_or_else(|err| {
            tracing::warn!(%err, "could not fetch Epic playtime");
            HashMap::new()
        });
        let cache = self.cache();
        let mut games: Vec<ImportedGame> = records
            .iter()
            .filter_map(|record| {
                let item = cache.catalog.get(&record.catalog_item_id)?.as_ref()?;
                item.is_game().then(|| ImportedGame {
                    installed: false,
                    playtime_seconds: playtime.get(&record.app_name).copied(),
                    ..ImportedGame::installed(record.app_name.clone(), item.title.trim())
                })
            })
            .collect();
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    /// The catalog item of an owned game. `NotConfigured` when signed out, `Ok(None)` when the
    /// account does not own `app_name` or the catalog does not know it.
    pub fn catalog_item(&self, app_name: &str) -> Result<Option<Arc<CatalogItem>>, ProviderError> {
        if !self.session.is_signed_in() {
            return Err(ProviderError::NotConfigured(
                "Sign in to Epic Games in Settings.".into(),
            ));
        }
        let key = app_name.to_lowercase();
        let lookup = |account: &Self| {
            account
                .cache()
                .library
                .as_ref()
                .map(|l| l.get(&key).cloned())
        };
        let record = match lookup(self) {
            Some(found) => found,
            None => {
                self.fetch_library()?;
                lookup(self).flatten()
            }
        };
        let Some(record) = record else {
            return Ok(None);
        };
        self.fetch_catalog(std::slice::from_ref(&record))?;
        Ok(self
            .cache()
            .catalog
            .get(&record.catalog_item_id)
            .cloned()
            .flatten())
    }

    /// The library record of an owned game (namespace + catalog item id), from the cache or a
    /// library fetch. `None` when signed out, not owned, or the library can't be fetched.
    pub fn library_record(&self, app_name: &str) -> Option<LibraryRecord> {
        if !self.session.is_signed_in() {
            return None;
        }
        let key = app_name.to_lowercase();
        let cached = self.cache().library.as_ref().map(|l| l.get(&key).cloned());
        match cached {
            Some(found) => found,
            None => {
                if let Err(err) = self.fetch_library() {
                    tracing::warn!(%err, "could not fetch the Epic library");
                    return None;
                }
                self.cache()
                    .library
                    .as_ref()
                    .and_then(|l| l.get(&key).cloned())
            }
        }
    }

    fn clear_cache(&self) {
        *self.cache() = Cache::default();
    }
}

impl AccountAuth for EpicAccount {
    fn status(&self) -> AccountStatus {
        AccountStatus {
            provider: EPIC_SOURCE.into(),
            name: "Epic Games".into(),
            signed_in: self.session.is_signed_in(),
            display_name: self.session.display_name(),
            supports_code_sign_in: true,
            code_url: Some(REDIRECT_URL.into()),
        }
    }

    fn login_url(&self) -> String {
        login_url()
    }

    fn sign_in_with_code(&self, input: &str) -> Result<(), AuthError> {
        let code = extract_code(input).ok_or_else(|| {
            AuthError::Rejected(
                "No Epic sign-in code found. Paste the authorizationCode value (or the whole page text) from the page you were sent to."
                    .into(),
            )
        })?;
        let request = token_request(&[("grant_type", "authorization_code"), ("code", &code)]);
        let response = self.http.send(&request).map_err(AuthError::Network)?;
        if !response.ok() {
            return Err(token_status_error(
                &response,
                "Epic did not accept that code. Codes work once and expire quickly; get a new one and try again.",
            ));
        }
        let tokens = parse_token_response(&response.body, self.session.now(), None)?;
        self.clear_cache();
        self.session.sign_in(tokens)
    }

    fn sign_out(&self) -> Result<(), AuthError> {
        self.clear_cache();
        self.session.sign_out()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::auth::MemoryTokenStore;
    use crate::auth::testing::{FakeHttp, response};

    const TOKEN_FIXTURE: &str = include_str!("../../../tests/fixtures/epic/token.json");
    const LIBRARY_PAGE_1: &str = include_str!("../../../tests/fixtures/epic/library_page1.json");
    const LIBRARY_PAGE_2: &str = include_str!("../../../tests/fixtures/epic/library_page2.json");
    const CATALOG: &str = include_str!("../../../tests/fixtures/epic/catalog_bulk.json");
    const PLAYTIME: &str = include_str!("../../../tests/fixtures/epic/playtime.json");

    #[test]
    fn basic_auth_header_is_the_launcher_client() {
        // base64("34a02cf8f4414e29b15921876da36f9a:daafbccc737745039dffe53d94fc76cf")
        assert!(BASIC_AUTH.starts_with("basic MzRhMDJjZjhmNDQxNGUyOWIx"));
        assert!(BASIC_AUTH.ends_with("Y2Y="));
    }

    #[test]
    fn login_url_redirects_to_the_code_page() {
        assert_eq!(
            login_url(),
            "https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode"
        );
    }

    #[test]
    fn extracts_codes_from_json_and_bare_pastes() {
        let code = "0123456789abcdef0123456789abcdef";
        let json = format!(
            r#"{{"redirectUrl":"https://localhost/launcher/authorized?code={code}","authorizationCode":"{code}","exchangeCode":null,"sid":null}}"#
        );
        assert_eq!(extract_code(&json).as_deref(), Some(code));
        assert_eq!(
            extract_code(&format!(" \"{code}\" ")).as_deref(),
            Some(code)
        );
        assert_eq!(extract_code(r#"{"authorizationCode":null}"#), None);
        assert_eq!(extract_code("{broken"), None);
        assert_eq!(extract_code("short"), None);
    }

    #[test]
    fn parses_token_response() {
        let tokens = parse_token_response(TOKEN_FIXTURE, 1_000, None).unwrap();
        assert_eq!(tokens.access_token, "eg1~epic-access-token-value");
        assert_eq!(tokens.refresh_token, "epic-refresh-token-value");
        assert_eq!(tokens.expires_at, 1_000 + 7200);
        assert_eq!(
            tokens.account_id.as_deref(),
            Some("8a1b2c3d4e5f60718293a4b5c6d7e8f9")
        );
        assert_eq!(tokens.display_name.as_deref(), Some("SugarPlayer"));
        assert!(parse_token_response("{}", 0, None).is_err());
        assert!(parse_token_response("nope", 0, None).is_err());
    }

    fn make_account(
        handler: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
    ) -> (EpicAccount, Arc<MemoryTokenStore>, Arc<FakeHttp>) {
        let store = Arc::new(MemoryTokenStore::new());
        let http = Arc::new(FakeHttp::new(handler));
        let account = EpicAccount::with(store.clone(), http.clone(), Arc::new(|| 1_000));
        (account, store, http)
    }

    fn form_value<'a>(req: &'a HttpRequest, key: &str) -> Option<&'a str> {
        req.form
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn code_sign_in_posts_the_eg1_exchange_and_stores_tokens() {
        let (account, store, http) = make_account(|_| response(200, TOKEN_FIXTURE));
        let code = "0123456789abcdef0123456789abcdef";
        account
            .sign_in_with_code(&format!(r#"{{"authorizationCode":"{code}"}}"#))
            .unwrap();
        let sent = http.log.lock().unwrap()[0].clone();
        assert_eq!(sent.url, TOKEN_URL);
        assert_eq!(form_value(&sent, "grant_type"), Some("authorization_code"));
        assert_eq!(form_value(&sent, "code"), Some(code));
        assert_eq!(form_value(&sent, "token_type"), Some("eg1"));
        assert_eq!(sent.headers[0].1, BASIC_AUTH);
        let status = account.status();
        assert!(status.signed_in);
        assert_eq!(status.display_name.as_deref(), Some("SugarPlayer"));
        assert_eq!(status.code_url.as_deref(), Some(REDIRECT_URL));
        assert!(status.supports_code_sign_in);
        let stored = store.load("epic").unwrap().unwrap();
        assert!(stored.contains("epic-refresh-token-value"));
        assert!(!stored.contains("eg1~epic-access-token-value"));

        account.sign_out().unwrap();
        assert!(!account.status().signed_in);
        assert_eq!(store.load("epic").unwrap(), None);
    }

    #[test]
    fn sign_in_errors_are_readable_and_secret_free() {
        let code = "0123456789abcdef0123456789abcdef";
        let (account, _, _) = make_account(|_| {
            response(
                400,
                r#"{"errorCode":"errors.com.epicgames.account.oauth.authorization_code_not_found"}"#,
            )
        });
        let err = account.sign_in_with_code(code).unwrap_err();
        assert!(err.to_string().contains("did not accept"));
        assert!(!err.to_string().contains(code));
        assert!(
            account
                .sign_in_with_code("hello world")
                .unwrap_err()
                .to_string()
                .contains("No Epic sign-in code")
        );
        let (account, _, _) = make_account(|_| Err("connection reset".into()));
        assert!(matches!(
            account.sign_in_with_code(code),
            Err(AuthError::Network(_))
        ));
    }

    #[test]
    fn refresh_uses_the_refresh_grant_with_eg1() {
        let http = FakeHttp::new(|req| {
            assert_eq!(form_value(req, "grant_type"), Some("refresh_token"));
            assert_eq!(form_value(req, "refresh_token"), Some("R1"));
            assert_eq!(form_value(req, "token_type"), Some("eg1"));
            response(200, TOKEN_FIXTURE)
        });
        let old = TokenSet {
            access_token: String::new(),
            refresh_token: "R1".into(),
            expires_at: 0,
            account_id: None,
            display_name: None,
        };
        assert_eq!(
            refresh_tokens(&http, &old, 5).unwrap().refresh_token,
            "epic-refresh-token-value"
        );
        let rejected = FakeHttp::new(|_| response(400, ""));
        assert!(matches!(
            refresh_tokens(&rejected, &old, 5),
            Err(AuthError::Rejected(_))
        ));
    }

    /// A signed-in account over a scripted Epic: counts catalog requests.
    fn signed_in_account() -> (EpicAccount, Arc<FakeHttp>, Arc<AtomicUsize>) {
        let catalog_calls = Arc::new(AtomicUsize::new(0));
        let counter = catalog_calls.clone();
        let (account, store, http) = make_account(move |req| {
            if req.url == TOKEN_URL {
                response(200, TOKEN_FIXTURE)
            } else if req.url.starts_with(LIBRARY_URL) {
                if req.url.ends_with("&cursor=cursor-2") {
                    response(200, LIBRARY_PAGE_2)
                } else {
                    response(200, LIBRARY_PAGE_1)
                }
            } else if req.url.starts_with(CATALOG_URL) {
                counter.fetch_add(1, Ordering::SeqCst);
                response(200, CATALOG)
            } else if req.url.contains("/playtime/account/") {
                response(200, PLAYTIME)
            } else {
                response(404, "")
            }
        });
        store
            .save(
                "epic",
                r#"{"refreshToken":"epic-refresh-token-value","accountId":"8a1b2c3d4e5f60718293a4b5c6d7e8f9"}"#,
            )
            .unwrap();
        (account, http, catalog_calls)
    }

    #[test]
    fn owned_scan_filters_pages_and_batches_catalog_requests() {
        let (account, http, catalog_calls) = signed_in_account();
        let games = account.scan_owned().unwrap();
        let summary: Vec<_> = games
            .iter()
            .map(|g| {
                (
                    g.name.as_str(),
                    g.source_game_id.as_str(),
                    g.playtime_seconds,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("Launchable Addon", "Launch", None),
                ("Sugar Game", "Sugar", Some(6_000)),
            ]
        );
        assert!(games.iter().all(|g| !g.installed));
        // Records: Sugar, Dlc, Launch share one namespace -> a single batched catalog request.
        assert_eq!(catalog_calls.load(Ordering::SeqCst), 1);
        let urls = http.urls();
        let catalog_url = urls.iter().find(|u| u.starts_with(CATALOG_URL)).unwrap();
        assert!(catalog_url.contains("/ns-sugar/bulk/items?"));
        for id in ["sugar-item", "dlc-item", "launch-item"] {
            assert!(catalog_url.contains(&format!("&id={id}")), "{catalog_url}");
        }
        assert!(catalog_url.contains("includeMainGameDetails=true"));
        assert!(
            urls.iter()
                .any(|u| u.contains("/playtime/account/8a1b2c3d4e5f60718293a4b5c6d7e8f9/all"))
        );
        assert!(
            urls.iter().any(|u| u.ends_with("&cursor=cursor-2")),
            "cursor paging"
        );

        // A second scan reuses the cached catalog.
        account.scan_owned().unwrap();
        assert_eq!(catalog_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn playtime_failure_does_not_fail_the_scan() {
        let (account, _, _) = make_account(|req| {
            if req.url == TOKEN_URL {
                response(200, TOKEN_FIXTURE)
            } else if req.url.starts_with(LIBRARY_URL) {
                response(200, LIBRARY_PAGE_2)
            } else if req.url.starts_with(CATALOG_URL) {
                response(200, CATALOG)
            } else {
                response(500, "")
            }
        });
        account
            .sign_in_with_code("0123456789abcdef0123456789abcdef")
            .unwrap();
        let games = account.scan_owned().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].playtime_seconds, None);
    }

    #[test]
    fn catalog_lookup_gives_metadata_and_images_per_app() {
        let (account, _, catalog_calls) = signed_in_account();
        let item = account.catalog_item("sugar").unwrap().unwrap();
        assert_eq!(item.title, "Sugar Game");
        assert_eq!(item.cover_urls()[0], "https://cdn.example/sugar-tall.jpg");
        // The library was loaded for the lookup and the catalog item cached.
        account.catalog_item("Sugar").unwrap().unwrap();
        assert_eq!(catalog_calls.load(Ordering::SeqCst), 1);
        assert!(account.catalog_item("NotOwned").unwrap().is_none());
    }

    #[test]
    fn catalog_lookup_signed_out_is_not_configured() {
        let (account, _, http) = make_account(|_| response(200, ""));
        assert!(matches!(
            account.catalog_item("Sugar"),
            Err(ProviderError::NotConfigured(m)) if m == "Sign in to Epic Games in Settings."
        ));
        assert!(matches!(
            account.scan_owned(),
            Err(ProviderError::NotConfigured(_))
        ));
        assert!(http.urls().is_empty(), "no request without a session");
    }

    #[test]
    fn library_cursor_loops_terminate() {
        let (account, _, _) = make_account(|req| {
            if req.url == TOKEN_URL {
                response(200, TOKEN_FIXTURE)
            } else {
                // Every page points to the same next cursor.
                response(200, LIBRARY_PAGE_1)
            }
        });
        account
            .sign_in_with_code("0123456789abcdef0123456789abcdef")
            .unwrap();
        let records = account.fetch_library().unwrap();
        assert_eq!(records.len(), 2, "de-duplicated candidates");
    }
}
