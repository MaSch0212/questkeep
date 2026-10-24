//! Catalog lookups without a user account.
//!
//! The launcher's public OAuth client can request an application token with the
//! `client_credentials` grant (no user involved). That token is accepted by the catalog service
//! (`bulk/items` for a namespace + catalog item id), which returns the store listing with its key
//! images. The ids come from the installed-game manifests, so covers work for installed games
//! even when the user never signs in. Only URLs and field names are used from the open-source
//! launchers; no code.
//!
//! The app token lives in memory only and is never written anywhere. Catalog answers are cached
//! for the run (`None` = Epic does not know the item) and, when a disk cache is set, across runs
//! (see `cache`). Transport errors are not cached, so a later lookup retries.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde::Deserialize;

use super::super::ProviderError;
use super::account::{CATALOG_URL, token_request};
use super::cache::CatalogDiskCache;
use super::web::{self, CatalogItem};
use crate::auth::{Clock, Http, HttpRequest, UreqHttp, system_clock, url_encode};

/// Seconds before expiry at which the token is renewed.
const TOKEN_MARGIN_SECS: i64 = 60;

#[derive(Deserialize)]
struct AppTokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// The token and the unix time it expires at.
pub fn parse_app_token(body: &str, now: i64) -> Result<(String, i64), ProviderError> {
    let unreadable = || ProviderError::Parse("Epic returned an unexpected token response.".into());
    let parsed: AppTokenResponse = serde_json::from_str(body).map_err(|_| unreadable())?;
    if parsed.access_token.is_empty() {
        return Err(unreadable());
    }
    Ok((parsed.access_token, now + parsed.expires_in.unwrap_or(3600)))
}

/// `…/namespace/<ns>/bulk/items?id=<item>&country=US&locale=en-US&includeMainGameDetails=true`.
pub fn catalog_item_url(namespace: &str, item_id: &str) -> String {
    format!(
        "{CATALOG_URL}/{}/bulk/items?id={}&country=US&locale=en-US&includeMainGameDetails=true",
        url_encode(namespace),
        url_encode(item_id)
    )
}

#[derive(Default)]
struct State {
    token: Option<(String, i64)>,
    items: HashMap<String, Option<Arc<CatalogItem>>>,
}

/// Account-free access to the Epic catalog.
pub struct PublicCatalog {
    http: Arc<dyn Http>,
    clock: Clock,
    state: Mutex<State>,
    /// Optional persistent layer below the in-memory cache.
    disk: OnceLock<Arc<CatalogDiskCache>>,
}

impl PublicCatalog {
    /// Real network and system clock.
    pub fn new() -> Self {
        Self::with(Arc::new(UreqHttp::new()), system_clock())
    }

    /// Injectable network and clock (tests).
    pub fn with(http: Arc<dyn Http>, clock: Clock) -> Self {
        Self {
            http,
            clock,
            state: Mutex::new(State::default()),
            disk: OnceLock::new(),
        }
    }

    /// Keeps answers on disk as well (fresh entries replace network requests). Only the first
    /// call has an effect.
    pub fn set_disk_cache(&self, cache: Arc<CatalogDiskCache>) {
        let _ = self.disk.set(cache);
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn token(&self) -> Result<String, ProviderError> {
        let now = (self.clock)();
        if let Some((token, expires_at)) = &self.state().token
            && *expires_at - TOKEN_MARGIN_SECS > now
        {
            return Ok(token.clone());
        }
        let request = token_request(&[("grant_type", "client_credentials")]);
        let response = self.http.send(&request).map_err(ProviderError::Network)?;
        if !response.ok() {
            return Err(ProviderError::Network(format!(
                "Epic returned HTTP {} for the app token",
                response.status
            )));
        }
        let (token, expires_at) = parse_app_token(&response.body, now)?;
        self.state().token = Some((token.clone(), expires_at));
        Ok(token)
    }

    fn forget_token(&self) {
        self.state().token = None;
    }

    /// The catalog item for a namespace + catalog item id; `Ok(None)` when Epic does not know it.
    pub fn item(
        &self,
        namespace: &str,
        item_id: &str,
    ) -> Result<Option<Arc<CatalogItem>>, ProviderError> {
        if let Some(cached) = self.state().items.get(item_id) {
            return Ok(cached.clone());
        }
        if let Some(stored) = self.disk.get().and_then(|d| d.get(item_id)) {
            self.state()
                .items
                .insert(item_id.to_string(), stored.clone());
            return Ok(stored);
        }
        let url = catalog_item_url(namespace, item_id);
        let mut retried = false;
        let response = loop {
            let token = self.token()?;
            let response = self
                .http
                .send(&HttpRequest::get(url.clone()).bearer(&token))
                .map_err(ProviderError::Network)?;
            if response.status == 401 && !retried {
                // Expired or revoked early: get a fresh app token once.
                retried = true;
                self.forget_token();
                continue;
            }
            break response;
        };
        let found = match response.status {
            404 => None,
            _ if response.ok() => web::parse_catalog(&response.body)?
                .remove(item_id)
                .map(Arc::new),
            status => {
                return Err(ProviderError::Network(format!(
                    "Epic returned HTTP {status} for the catalog"
                )));
            }
        };
        if let Some(disk) = self.disk.get() {
            disk.put(item_id, found.as_deref());
        }
        self.state()
            .items
            .insert(item_id.to_string(), found.clone());
        Ok(found)
    }
}

impl Default for PublicCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PublicCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicCatalog").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    use super::*;
    use crate::auth::testing::{FakeHttp, response};

    const APP_TOKEN: &str = include_str!("../../../tests/fixtures/epic/app_token.json");
    const CATALOG: &str = include_str!("../../../tests/fixtures/epic/catalog_bulk.json");

    fn form_value<'a>(req: &'a HttpRequest, key: &str) -> Option<&'a str> {
        req.form
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn scripted() -> (PublicCatalog, Arc<FakeHttp>, Arc<AtomicI64>) {
        let now = Arc::new(AtomicI64::new(1_000));
        let clock_now = now.clone();
        let http = Arc::new(FakeHttp::new(|req| {
            if req.url.contains("/oauth/token") {
                response(200, APP_TOKEN)
            } else if req.url.contains("/bulk/items") && req.url.contains("id=sugar-item") {
                response(200, CATALOG)
            } else {
                response(200, "{}")
            }
        }));
        let catalog = PublicCatalog::with(
            http.clone(),
            Arc::new(move || clock_now.load(Ordering::SeqCst)),
        );
        (catalog, http, now)
    }

    #[test]
    fn parses_app_token() {
        assert_eq!(
            parse_app_token(APP_TOKEN, 100).unwrap(),
            ("eg1~epic-app-token-value".to_string(), 3_700)
        );
        assert!(parse_app_token("{}", 0).is_err());
        assert!(parse_app_token(r#"{"access_token":""}"#, 0).is_err());
        assert!(parse_app_token("nope", 0).is_err());
    }

    #[test]
    fn builds_the_catalog_url() {
        assert_eq!(
            catalog_item_url("ns sugar", "item/1"),
            format!(
                "{CATALOG_URL}/ns%20sugar/bulk/items?id=item%2F1&country=US&locale=en-US&includeMainGameDetails=true"
            )
        );
    }

    #[test]
    fn looks_up_without_an_account_and_caches() {
        let (catalog, http, _) = scripted();
        let item = catalog.item("ns-sugar", "sugar-item").unwrap().unwrap();
        assert_eq!(item.cover_urls()[0], "https://cdn.example/sugar-tall.jpg");
        let log = http.log.lock().unwrap().clone();
        assert_eq!(log.len(), 2);
        assert_eq!(
            form_value(&log[0], "grant_type"),
            Some("client_credentials")
        );
        assert_eq!(form_value(&log[0], "token_type"), Some("eg1"));
        assert!(
            log[1]
                .headers
                .iter()
                .any(|(k, v)| k == "Authorization" && v == "Bearer eg1~epic-app-token-value")
        );
        // Cached: no further requests, also for unknown items.
        catalog.item("ns-sugar", "sugar-item").unwrap().unwrap();
        assert!(catalog.item("ns-sugar", "other").unwrap().is_none());
        assert!(catalog.item("ns-sugar", "other").unwrap().is_none());
        assert_eq!(http.urls().len(), 3);
    }

    #[test]
    fn token_is_reused_until_near_expiry() {
        let (catalog, http, now) = scripted();
        catalog.item("n", "a").unwrap();
        catalog.item("n", "b").unwrap();
        let tokens = |h: &FakeHttp| {
            h.urls()
                .iter()
                .filter(|u| u.contains("/oauth/token"))
                .count()
        };
        assert_eq!(tokens(&http), 1);
        now.store(1_000 + 3_600 - 30, Ordering::SeqCst);
        catalog.item("n", "c").unwrap();
        assert_eq!(tokens(&http), 2);
    }

    #[test]
    fn unauthorized_refreshes_the_token_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let http = Arc::new(FakeHttp::new(move |req| {
            if req.url.contains("/oauth/token") {
                response(200, APP_TOKEN)
            } else if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                response(401, "")
            } else {
                response(200, CATALOG)
            }
        }));
        let catalog = PublicCatalog::with(http.clone(), Arc::new(|| 5));
        assert!(catalog.item("ns-sugar", "sugar-item").unwrap().is_some());
        // token, catalog (401), token, catalog
        assert_eq!(http.urls().len(), 4);
    }

    #[test]
    fn failures_are_errors_and_not_cached() {
        let http = Arc::new(FakeHttp::new(|req| {
            if req.url.contains("/oauth/token") {
                response(200, APP_TOKEN)
            } else {
                response(500, "")
            }
        }));
        let catalog = PublicCatalog::with(http.clone(), Arc::new(|| 5));
        assert!(matches!(
            catalog.item("n", "x"),
            Err(ProviderError::Network(_))
        ));
        assert!(catalog.item("n", "x").is_err());
        assert_eq!(http.urls().len(), 3, "token once, catalog twice");

        let down = PublicCatalog::with(
            Arc::new(FakeHttp::new(|_| Err("offline".into()))),
            Arc::new(|| 5),
        );
        assert!(matches!(
            down.item("n", "x"),
            Err(ProviderError::Network(_))
        ));
        let rejected = PublicCatalog::with(
            Arc::new(FakeHttp::new(|_| response(403, ""))),
            Arc::new(|| 5),
        );
        assert!(rejected.item("n", "x").is_err());
    }

    #[test]
    fn disk_cache_survives_a_restart_and_covers_unknown_items() {
        let tmp = tempfile::tempdir().unwrap();
        let clock: Clock = Arc::new(|| 5);
        let disk = || Arc::new(CatalogDiskCache::new(tmp.path(), Arc::new(|| 5)));
        let http = Arc::new(FakeHttp::new(|req| {
            if req.url.contains("/oauth/token") {
                response(200, APP_TOKEN)
            } else if req.url.contains("id=sugar-item") {
                response(200, CATALOG)
            } else {
                response(404, "")
            }
        }));
        let first = PublicCatalog::with(http.clone(), clock.clone());
        first.set_disk_cache(disk());
        assert!(first.item("ns-sugar", "sugar-item").unwrap().is_some());
        assert!(first.item("ns-sugar", "missing").unwrap().is_none());
        assert_eq!(http.urls().len(), 3, "token + two catalog requests");

        // New run, same folder: no network at all.
        let offline = Arc::new(FakeHttp::new(|_| Err("offline".into())));
        let second = PublicCatalog::with(offline.clone(), clock);
        second.set_disk_cache(disk());
        let item = second.item("ns-sugar", "sugar-item").unwrap().unwrap();
        assert_eq!(item.cover_urls()[0], "https://cdn.example/sugar-tall.jpg");
        assert!(second.item("ns-sugar", "missing").unwrap().is_none());
        assert!(offline.urls().is_empty());
    }

    #[test]
    fn expired_disk_entries_are_fetched_again() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Arc::new(AtomicI64::new(1_000));
        let clock_now = now.clone();
        let clock: Clock = Arc::new(move || clock_now.load(Ordering::SeqCst));
        let http = Arc::new(FakeHttp::new(|req| {
            if req.url.contains("/oauth/token") {
                response(200, APP_TOKEN)
            } else {
                response(200, CATALOG)
            }
        }));
        let make = || {
            let catalog = PublicCatalog::with(http.clone(), clock.clone());
            catalog.set_disk_cache(Arc::new(CatalogDiskCache::new(tmp.path(), clock.clone())));
            catalog
        };
        make().item("ns-sugar", "sugar-item").unwrap();
        let requests = || {
            http.urls()
                .iter()
                .filter(|u| u.contains("/bulk/items"))
                .count()
        };
        assert_eq!(requests(), 1);
        make().item("ns-sugar", "sugar-item").unwrap();
        assert_eq!(requests(), 1, "fresh entry");
        now.store(
            1_000 + super::super::cache::ITEM_TTL_SECS + 1,
            Ordering::SeqCst,
        );
        make().item("ns-sugar", "sugar-item").unwrap();
        assert_eq!(requests(), 2, "expired entry");
    }

    #[test]
    fn not_found_is_cached_as_unknown() {
        let http = Arc::new(FakeHttp::new(|req| {
            if req.url.contains("/oauth/token") {
                response(200, APP_TOKEN)
            } else {
                response(404, "")
            }
        }));
        let catalog = PublicCatalog::with(http.clone(), Arc::new(|| 5));
        assert!(catalog.item("n", "x").unwrap().is_none());
        assert!(catalog.item("n", "x").unwrap().is_none());
        assert_eq!(http.urls().len(), 2);
    }
}
