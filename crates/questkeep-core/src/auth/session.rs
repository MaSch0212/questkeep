//! One signed-in account of a store: lazy token loading, refresh before expiry and on 401.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::AuthError;
use super::http::{Http, HttpRequest, HttpResponse};
use super::store::{TokenSet, TokenStore};
use crate::providers::ProviderError;

/// Seconds since the Unix epoch; injectable so tests control time.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| chrono::Utc::now().timestamp())
}

/// Tokens are refreshed when they expire within this many seconds.
pub const REFRESH_MARGIN_SECS: i64 = 60;

/// Exchanges the refresh token of `old` for a new token set. Return [`AuthError::Rejected`] when
/// the store refused the refresh token (the session is then signed out) and any other error for
/// transient problems (the tokens are kept).
pub type RefreshFn = fn(&dyn Http, &TokenSet, i64) -> Result<TokenSet, AuthError>;

#[derive(Default)]
struct State {
    loaded: bool,
    tokens: Option<TokenSet>,
}

pub struct AccountSession {
    provider: &'static str,
    store_name: &'static str,
    tokens: Arc<dyn TokenStore>,
    http: Arc<dyn Http>,
    clock: Clock,
    refresh: RefreshFn,
    state: Mutex<State>,
}

impl std::fmt::Debug for AccountSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountSession")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

impl AccountSession {
    pub fn new(
        provider: &'static str,
        store_name: &'static str,
        tokens: Arc<dyn TokenStore>,
        http: Arc<dyn Http>,
        clock: Clock,
        refresh: RefreshFn,
    ) -> Self {
        Self {
            provider,
            store_name,
            tokens,
            http,
            clock,
            refresh,
            state: Mutex::new(State::default()),
        }
    }

    pub fn now(&self) -> i64 {
        (self.clock)()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn not_signed_in(&self) -> ProviderError {
        ProviderError::NotConfigured(format!("Sign in to {} in Settings.", self.store_name))
    }

    fn signed_out_by_server(&self) -> ProviderError {
        ProviderError::NotConfigured(format!("Sign in to {} again in Settings.", self.store_name))
    }

    /// Loads the stored tokens once. Failures are not cached, so a locked keyring can be retried.
    fn load(&self, state: &mut State) -> Result<(), AuthError> {
        if state.loaded {
            return Ok(());
        }
        let stored = self.tokens.load(self.provider)?;
        state.tokens = stored.as_deref().and_then(TokenSet::from_stored);
        state.loaded = true;
        Ok(())
    }

    fn provider_error(&self, err: AuthError) -> ProviderError {
        match err {
            AuthError::NoKeyring(_) | AuthError::Storage(_) => {
                ProviderError::NotConfigured(err.to_string())
            }
            other => other.into(),
        }
    }

    pub fn is_signed_in(&self) -> bool {
        let mut state = self.lock();
        self.load(&mut state).is_ok() && state.tokens.is_some()
    }

    pub fn display_name(&self) -> Option<String> {
        let mut state = self.lock();
        self.load(&mut state).ok()?;
        state.tokens.as_ref()?.display_name.clone()
    }

    pub fn account_id(&self) -> Option<String> {
        let mut state = self.lock();
        self.load(&mut state).ok()?;
        state.tokens.as_ref()?.account_id.clone()
    }

    /// Persists new tokens and makes them current. Nothing is kept if saving fails, so the UI
    /// never shows "signed in" for an account that would be forgotten on restart.
    pub fn sign_in(&self, tokens: TokenSet) -> Result<(), AuthError> {
        let stored = tokens.to_stored()?;
        self.tokens.save(self.provider, &stored)?;
        let mut state = self.lock();
        state.loaded = true;
        state.tokens = Some(tokens);
        Ok(())
    }

    /// Deletes the stored tokens.
    pub fn sign_out(&self) -> Result<(), AuthError> {
        let result = self.tokens.delete(self.provider);
        let mut state = self.lock();
        state.loaded = true;
        state.tokens = None;
        result
    }

    /// A valid access token, refreshing first when it is (almost) expired.
    pub fn access_token(&self) -> Result<String, ProviderError> {
        let mut state = self.lock();
        self.load(&mut state).map_err(|e| self.provider_error(e))?;
        let Some(tokens) = &state.tokens else {
            return Err(self.not_signed_in());
        };
        let fresh =
            !tokens.access_token.is_empty() && tokens.expires_at - self.now() > REFRESH_MARGIN_SECS;
        if fresh {
            return Ok(tokens.access_token.clone());
        }
        self.refresh_locked(&mut state)
    }

    /// After a 401 for `rejected`: refreshes unless another caller already did.
    fn refresh_after_rejection(&self, rejected: &str) -> Result<String, ProviderError> {
        let mut state = self.lock();
        match &state.tokens {
            None => Err(self.not_signed_in()),
            Some(t) if !t.access_token.is_empty() && t.access_token != rejected => {
                Ok(t.access_token.clone())
            }
            Some(_) => self.refresh_locked(&mut state),
        }
    }

    fn refresh_locked(&self, state: &mut State) -> Result<String, ProviderError> {
        let Some(old) = state.tokens.clone() else {
            return Err(self.not_signed_in());
        };
        match (self.refresh)(self.http.as_ref(), &old, self.now()) {
            Ok(mut new) => {
                new.display_name = new.display_name.or(old.display_name);
                new.account_id = new.account_id.or(old.account_id);
                // The new refresh token replaces the old one, so a save failure must not lose
                // the working session; it only costs a new sign-in after a restart.
                match new
                    .to_stored()
                    .and_then(|s| self.tokens.save(self.provider, &s))
                {
                    Ok(()) => {}
                    Err(err) => {
                        tracing::warn!(provider = self.provider, %err, "could not save refreshed tokens")
                    }
                }
                let access = new.access_token.clone();
                state.tokens = Some(new);
                Ok(access)
            }
            Err(AuthError::Rejected(reason)) => {
                tracing::info!(provider = self.provider, %reason, "refresh token rejected, signing out");
                if let Err(err) = self.tokens.delete(self.provider) {
                    tracing::warn!(provider = self.provider, %err, "could not delete rejected tokens");
                }
                state.tokens = None;
                Err(self.signed_out_by_server())
            }
            Err(other) => Err(other.into()),
        }
    }

    /// Sends an authenticated request built by `build` from the access token. A 401 triggers one
    /// refresh and one retry. Any non-2xx answer becomes a readable [`ProviderError`].
    pub fn send(&self, build: &dyn Fn(&str) -> HttpRequest) -> Result<HttpResponse, ProviderError> {
        let token = self.access_token()?;
        let mut response = self.transport(&build(&token))?;
        if response.status == 401 {
            let token = self.refresh_after_rejection(&token)?;
            response = self.transport(&build(&token))?;
            if response.status == 401 {
                let mut state = self.lock();
                if let Err(err) = self.tokens.delete(self.provider) {
                    tracing::warn!(provider = self.provider, %err, "could not delete rejected tokens");
                }
                state.tokens = None;
                return Err(self.signed_out_by_server());
            }
        }
        if response.ok() {
            Ok(response)
        } else {
            Err(ProviderError::Network(format!(
                "{} returned HTTP {}",
                self.store_name, response.status
            )))
        }
    }

    fn transport(&self, request: &HttpRequest) -> Result<HttpResponse, ProviderError> {
        self.http.send(request).map_err(ProviderError::Network)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    use super::super::http::testing::{FakeHttp, response};
    use super::super::store::MemoryTokenStore;
    use super::*;

    /// Refresh that asks the (fake) server and parses `{"a":"..","r":"..","e":secs}`.
    fn refresh(http: &dyn Http, old: &TokenSet, now: i64) -> Result<TokenSet, AuthError> {
        let request = HttpRequest::post_form(
            "https://store.test/refresh",
            vec![("refresh_token".into(), old.refresh_token.clone())],
        );
        let resp = http.send(&request).map_err(AuthError::Network)?;
        match resp.status {
            200 => {
                let v: serde_json::Value = serde_json::from_str(&resp.body)
                    .map_err(|e| AuthError::Parse(e.to_string()))?;
                Ok(TokenSet {
                    access_token: v["a"].as_str().unwrap_or_default().into(),
                    refresh_token: v["r"].as_str().unwrap_or_default().into(),
                    expires_at: now + v["e"].as_i64().unwrap_or(0),
                    account_id: None,
                    display_name: None,
                })
            }
            400 | 401 => Err(AuthError::Rejected("refused".into())),
            s => Err(AuthError::Network(format!("HTTP {s}"))),
        }
    }

    struct Fixture {
        session: AccountSession,
        store: Arc<MemoryTokenStore>,
        http: Arc<FakeHttp>,
        now: Arc<AtomicI64>,
        refreshes: Arc<AtomicUsize>,
    }

    /// `api_status` answers `/api` calls; `/refresh` hands out `A<n>` tokens that live 3600 s.
    fn fixture(refresh_status: u16, api_statuses: Vec<u16>) -> Fixture {
        let refreshes = Arc::new(AtomicUsize::new(0));
        let counter = refreshes.clone();
        let api_calls = Arc::new(AtomicUsize::new(0));
        let http = Arc::new(FakeHttp::new(move |req| {
            if req.url.ends_with("/refresh") {
                let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                if refresh_status == 200 {
                    response(200, &format!(r#"{{"a":"A{n}","r":"R{n}","e":3600}}"#))
                } else {
                    response(refresh_status, "")
                }
            } else {
                let i = api_calls.fetch_add(1, Ordering::SeqCst);
                let status = api_statuses.get(i).copied().unwrap_or(200);
                response(status, req.headers[0].1.as_str())
            }
        }));
        let store = Arc::new(MemoryTokenStore::new());
        let now = Arc::new(AtomicI64::new(1_000));
        let clock_now = now.clone();
        let session = AccountSession::new(
            "teststore",
            "Test Store",
            store.clone(),
            http.clone(),
            Arc::new(move || clock_now.load(Ordering::SeqCst)),
            refresh,
        );
        Fixture {
            session,
            store,
            http,
            now,
            refreshes,
        }
    }

    fn signed_in_tokens(expires_at: i64) -> TokenSet {
        TokenSet {
            access_token: "A0".into(),
            refresh_token: "R0".into(),
            expires_at,
            account_id: Some("acc".into()),
            display_name: Some("Ann".into()),
        }
    }

    fn api(f: &Fixture) -> Result<HttpResponse, ProviderError> {
        f.session
            .send(&|token| HttpRequest::get("https://store.test/api").bearer(token))
    }

    #[test]
    fn signed_out_session_asks_for_sign_in() {
        let f = fixture(200, vec![]);
        assert!(!f.session.is_signed_in());
        assert!(matches!(
            f.session.access_token(),
            Err(ProviderError::NotConfigured(m)) if m == "Sign in to Test Store in Settings."
        ));
    }

    #[test]
    fn valid_token_is_used_without_refresh() {
        let f = fixture(200, vec![]);
        f.session.sign_in(signed_in_tokens(1_000 + 600)).unwrap();
        assert_eq!(api(&f).unwrap().body, "Bearer A0");
        assert_eq!(f.refreshes.load(Ordering::SeqCst), 0);
        assert_eq!(f.session.display_name().as_deref(), Some("Ann"));
    }

    #[test]
    fn token_close_to_expiry_is_refreshed_and_saved() {
        let f = fixture(200, vec![]);
        f.session.sign_in(signed_in_tokens(1_000 + 59)).unwrap();
        assert_eq!(api(&f).unwrap().body, "Bearer A1");
        assert_eq!(f.refreshes.load(Ordering::SeqCst), 1);
        let stored = f.store.load("teststore").unwrap().unwrap();
        assert!(stored.contains("R1") && !stored.contains("R0") && !stored.contains("A1"));
        // Name and account survive the refresh although the server did not repeat them.
        assert_eq!(f.session.display_name().as_deref(), Some("Ann"));
        assert_eq!(f.session.account_id().as_deref(), Some("acc"));

        // Valid for an hour now; after that it is refreshed again (fake clock).
        api(&f).unwrap();
        assert_eq!(f.refreshes.load(Ordering::SeqCst), 1);
        f.now.store(1_000 + 3_600, Ordering::SeqCst);
        assert_eq!(api(&f).unwrap().body, "Bearer A2");
    }

    #[test]
    fn restored_session_refreshes_on_first_use() {
        let f = fixture(200, vec![]);
        f.store
            .save("teststore", &signed_in_tokens(0).to_stored().unwrap())
            .unwrap();
        assert!(f.session.is_signed_in());
        assert_eq!(api(&f).unwrap().body, "Bearer A1");
    }

    #[test]
    fn a_401_refreshes_and_retries_once() {
        let f = fixture(200, vec![401, 200]);
        f.session.sign_in(signed_in_tokens(1_000 + 600)).unwrap();
        assert_eq!(api(&f).unwrap().body, "Bearer A1");
        assert_eq!(f.refreshes.load(Ordering::SeqCst), 1);
        let urls = f.http.urls();
        assert_eq!(urls.iter().filter(|u| u.ends_with("/api")).count(), 2);
    }

    #[test]
    fn a_second_401_signs_out() {
        let f = fixture(200, vec![401, 401]);
        f.session.sign_in(signed_in_tokens(1_000 + 600)).unwrap();
        assert!(matches!(api(&f), Err(ProviderError::NotConfigured(_))));
        assert!(!f.session.is_signed_in());
        assert_eq!(f.store.load("teststore").unwrap(), None);
    }

    #[test]
    fn rejected_refresh_clears_tokens_with_a_readable_message() {
        let f = fixture(400, vec![]);
        f.session.sign_in(signed_in_tokens(0)).unwrap();
        let err = api(&f).unwrap_err();
        assert!(matches!(
            &err,
            ProviderError::NotConfigured(m) if m == "Sign in to Test Store again in Settings."
        ));
        assert_eq!(f.store.load("teststore").unwrap(), None);
        assert!(!f.session.is_signed_in());
    }

    #[test]
    fn transient_refresh_failure_keeps_the_tokens() {
        let f = fixture(503, vec![]);
        f.session.sign_in(signed_in_tokens(0)).unwrap();
        assert!(matches!(api(&f), Err(ProviderError::Network(_))));
        assert!(f.session.is_signed_in());
        assert!(f.store.load("teststore").unwrap().is_some());
    }

    #[test]
    fn other_statuses_are_readable_errors() {
        let f = fixture(200, vec![500]);
        f.session.sign_in(signed_in_tokens(1_000 + 600)).unwrap();
        let err = api(&f).unwrap_err();
        assert_eq!(
            err.to_string(),
            "network error: Test Store returned HTTP 500"
        );
    }

    #[test]
    fn sign_out_deletes_the_tokens() {
        let f = fixture(200, vec![]);
        f.session.sign_in(signed_in_tokens(5_000)).unwrap();
        assert!(f.store.load("teststore").unwrap().is_some());
        f.session.sign_out().unwrap();
        assert!(!f.session.is_signed_in());
        assert_eq!(f.store.load("teststore").unwrap(), None);
    }

    struct BrokenStore;
    impl TokenStore for BrokenStore {
        fn load(&self, _: &str) -> Result<Option<String>, AuthError> {
            Err(AuthError::NoKeyring("no secret service".into()))
        }
        fn save(&self, _: &str, _: &str) -> Result<(), AuthError> {
            Err(AuthError::NoKeyring("no secret service".into()))
        }
        fn delete(&self, _: &str) -> Result<(), AuthError> {
            Ok(())
        }
    }

    #[test]
    fn missing_keyring_is_a_readable_error_and_nothing_is_kept() {
        let session = AccountSession::new(
            "teststore",
            "Test Store",
            Arc::new(BrokenStore),
            Arc::new(FakeHttp::new(|_| response(200, ""))),
            system_clock(),
            refresh,
        );
        let err = session.sign_in(signed_in_tokens(5_000)).unwrap_err();
        assert!(err.to_string().starts_with("No system keyring available"));
        assert!(!session.is_signed_in());
        assert!(matches!(
            session.access_token(),
            Err(ProviderError::NotConfigured(m)) if m.starts_with("No system keyring")
        ));
    }
}
