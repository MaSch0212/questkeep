//! A store account kept as a website session ([`WebSession`]) for stores without a sign-in for
//! other applications (Battle.net, EA). The login window signs in on the store's website in a
//! browser profile of its own; afterwards a hidden window opens a store page with that session,
//! and a script on it reports what Questkeep needs through a capture address
//! (`<capture prefix>?name=...&...`, or `?signedout=1`), which the window never loads.
//!
//! Only the account's display name is stored (with the other accounts, in the keyring). The
//! captured data (owned games, a short-lived token) is kept in memory for a short while so that a
//! scan right after the sign-in does not open the store page again.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use super::{
    AccountAuth, AccountStatus, AuthError, Clock, TokenStore, WebSession, query_param, system_clock,
};
use crate::providers::ProviderError;

/// What a store page reported: the capture address's query parameters.
#[derive(Clone, PartialEq, Eq)]
pub struct WebCapture {
    params: Vec<(String, String)>,
}

impl std::fmt::Debug for WebCapture {
    // Values may be tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys: Vec<&str> = self.params.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("WebCapture").field("keys", &keys).finish()
    }
}

impl WebCapture {
    /// A parameter's decoded value; empty values count as missing.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    }

    /// The display name, with whitespace collapsed and at most 64 characters.
    fn display_name(&self) -> Option<String> {
        let name = self
            .get("name")?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        (!name.is_empty()).then(|| name.chars().take(64).collect())
    }
}

/// Reads a capture address. `Ok(None)` means the page saw no signed-in user.
pub fn parse_web_capture(prefix: &str, url: &str) -> Result<Option<WebCapture>, AuthError> {
    let rest = url
        .strip_prefix(prefix)
        .filter(|rest| rest.is_empty() || rest.starts_with('?'))
        .ok_or_else(|| AuthError::Rejected("The store page did not report an account.".into()))?;
    let query = rest.trim_start_matches('?');
    let keys = query
        .split('&')
        .filter_map(|pair| pair.split_once('=').map(|(k, _)| k).or(Some(pair)))
        .filter(|k| !k.is_empty());
    let mut params: Vec<(String, String)> = Vec::new();
    for key in keys {
        if !params.iter().any(|(k, _)| k == key) {
            params.push((key.to_string(), query_param(rest, key).unwrap_or_default()));
        }
    }
    let capture = WebCapture { params };
    Ok((capture.get("signedout").is_none()).then_some(capture))
}

/// What is stored: who is signed in (not secret).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Default)]
struct State {
    loaded: bool,
    account: Option<StoredAccount>,
    /// The last capture and when it was taken (Unix seconds).
    last: Option<(WebCapture, i64)>,
}

/// The parts that differ per store.
#[derive(Debug, Clone, Copy)]
pub struct WebAccountSpec {
    /// Provider id, e.g. `battlenet`.
    pub provider: &'static str,
    /// Store name for messages, e.g. `Battle.net`.
    pub name: &'static str,
    /// The page the login window opens.
    pub login_url: &'static str,
    /// The capture address the store's page script navigates to.
    pub capture_prefix: &'static str,
}

pub struct WebAccount {
    spec: WebAccountSpec,
    tokens: Arc<dyn TokenStore>,
    web: Option<Arc<dyn WebSession>>,
    clock: Clock,
    state: Mutex<State>,
}

impl std::fmt::Debug for WebAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebAccount")
            .field("provider", &self.spec.provider)
            .finish_non_exhaustive()
    }
}

impl WebAccount {
    /// `web` is the app's store session; without it (tests, no app) nothing can be captured.
    pub fn new(
        spec: WebAccountSpec,
        tokens: Arc<dyn TokenStore>,
        web: Option<Arc<dyn WebSession>>,
    ) -> Self {
        Self::with(spec, tokens, web, system_clock())
    }

    pub fn with(
        spec: WebAccountSpec,
        tokens: Arc<dyn TokenStore>,
        web: Option<Arc<dyn WebSession>>,
        clock: Clock,
    ) -> Self {
        Self {
            spec,
            tokens,
            web,
            clock,
            state: Mutex::new(State::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn load(&self, state: &mut State) -> Result<(), AuthError> {
        if state.loaded {
            return Ok(());
        }
        state.account = self
            .tokens
            .load(self.spec.provider)?
            .as_deref()
            .and_then(|json| serde_json::from_str::<StoredAccount>(json).ok());
        state.loaded = true;
        Ok(())
    }

    pub fn is_signed_in(&self) -> bool {
        let mut state = self.lock();
        self.load(&mut state).is_ok() && state.account.is_some()
    }

    fn sign_in_again(&self) -> ProviderError {
        ProviderError::NotConfigured(format!("Sign in to {} again in Settings.", self.spec.name))
    }

    fn save(&self, state: &mut State, account: StoredAccount) -> Result<(), AuthError> {
        if state.account.as_ref() != Some(&account) {
            let json =
                serde_json::to_string(&account).map_err(|e| AuthError::Storage(e.to_string()))?;
            self.tokens.save(self.spec.provider, &json)?;
        }
        state.loaded = true;
        state.account = Some(account);
        Ok(())
    }

    fn forget(&self, state: &mut State) {
        if let Err(err) = self.tokens.delete(self.spec.provider) {
            tracing::warn!(provider = self.spec.provider, %err, "could not delete the account");
        }
        state.loaded = true;
        state.account = None;
        state.last = None;
    }

    /// Keeps a capture: remembers it and updates the stored display name.
    fn accept(&self, state: &mut State, capture: WebCapture) -> Result<WebCapture, AuthError> {
        let account = StoredAccount {
            display_name: capture
                .display_name()
                .or_else(|| state.account.as_ref().and_then(|a| a.display_name.clone())),
        };
        self.save(state, account)?;
        state.last = Some((capture.clone(), (self.clock)()));
        Ok(capture)
    }

    /// Changes the shown name (for stores whose page does not report one).
    pub fn set_display_name(&self, name: &str) {
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut state = self.lock();
        if state.account.is_none() || name.is_empty() {
            return;
        }
        let account = StoredAccount {
            display_name: Some(name.chars().take(64).collect()),
        };
        if let Err(err) = self.save(&mut state, account) {
            tracing::warn!(provider = self.spec.provider, %err, "could not save the account");
        }
    }

    /// Opens the store page with the saved session and returns what it reported. A capture taken
    /// at most `max_age_secs` ago is reused. A signed-out session signs the account out.
    pub fn capture(&self, max_age_secs: i64) -> Result<WebCapture, ProviderError> {
        {
            let mut state = self.lock();
            self.load(&mut state)
                .map_err(|e| ProviderError::NotConfigured(e.to_string()))?;
            if state.account.is_none() {
                return Err(ProviderError::NotConfigured(format!(
                    "Sign in to {} in Settings.",
                    self.spec.name
                )));
            }
            if let Some((capture, at)) = &state.last
                && (self.clock)() - at <= max_age_secs
            {
                return Ok(capture.clone());
            }
        }
        // Not under the lock: the store page can take a while, and the status must stay readable.
        let captured = self.refresh()?;
        let mut state = self.lock();
        if state.account.is_none() {
            // Signed out while the page loaded.
            return Err(self.sign_in_again());
        }
        match captured {
            Some(capture) => self
                .accept(&mut state, capture)
                .map_err(|e| ProviderError::NotConfigured(e.to_string())),
            None => {
                tracing::info!(
                    provider = self.spec.provider,
                    "the store session is signed out"
                );
                self.forget(&mut state);
                Err(self.sign_in_again())
            }
        }
    }

    /// One capture through the web session.
    fn refresh(&self) -> Result<Option<WebCapture>, ProviderError> {
        let Some(web) = &self.web else {
            return Err(ProviderError::NotConfigured(format!(
                "{} sign-in needs the Questkeep app.",
                self.spec.name
            )));
        };
        match web
            .refresh(self.spec.provider)
            .map_err(ProviderError::Network)?
        {
            Some(url) => parse_web_capture(self.spec.capture_prefix, &url)
                .map_err(|e| ProviderError::Parse(e.to_string())),
            None => Ok(None),
        }
    }
}

impl AccountAuth for WebAccount {
    fn status(&self) -> AccountStatus {
        let mut state = self.lock();
        let account = self.load(&mut state).ok().and(state.account.clone());
        AccountStatus {
            provider: self.spec.provider.into(),
            name: self.spec.name.into(),
            signed_in: account.is_some(),
            display_name: account.and_then(|a| a.display_name),
            // The session lives in the login window's browser profile; there is no code.
            supports_code_sign_in: false,
            code_url: None,
        }
    }

    fn login_url(&self) -> String {
        self.spec.login_url.into()
    }

    /// `input` is what the login window caught: a capture address, or (when the login ends on a
    /// page without one, as EA's does) anything else, after which the store page is opened with
    /// the new session.
    fn sign_in_with_code(&self, input: &str) -> Result<(), AuthError> {
        let input = input.trim();
        let capture = if input.starts_with(self.spec.capture_prefix) {
            parse_web_capture(self.spec.capture_prefix, input)?
        } else {
            self.refresh().map_err(|e| match e {
                ProviderError::Network(m) => AuthError::Network(m),
                other => AuthError::Rejected(other.to_string()),
            })?
        };
        let capture = capture.ok_or_else(|| {
            AuthError::Rejected(format!(
                "{} did not report a signed-in account. Sign in again.",
                self.spec.name
            ))
        })?;
        let mut state = self.lock();
        self.load(&mut state)?;
        self.accept(&mut state, capture).map(|_| ())
    }

    fn sign_out(&self) -> Result<(), AuthError> {
        let result = self.tokens.delete(self.spec.provider);
        {
            let mut state = self.lock();
            state.loaded = true;
            state.account = None;
            state.last = None;
        }
        if let Some(web) = &self.web
            && let Err(err) = web.clear(self.spec.provider)
        {
            tracing::warn!(provider = self.spec.provider, %err, "could not clear the store session");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    use super::*;
    use crate::auth::MemoryTokenStore;

    const PREFIX: &str = "https://questkeep.invalid/test-session";
    const SPEC: WebAccountSpec = WebAccountSpec {
        provider: "test",
        name: "Test Store",
        login_url: "https://store.test/login",
        capture_prefix: PREFIX,
    };

    struct FakeWeb {
        next: Mutex<Vec<Result<Option<String>, String>>>,
        refreshes: AtomicUsize,
        clears: AtomicUsize,
    }

    impl FakeWeb {
        fn new(answers: Vec<Result<Option<String>, String>>) -> Arc<Self> {
            Arc::new(Self {
                next: Mutex::new(answers),
                refreshes: AtomicUsize::new(0),
                clears: AtomicUsize::new(0),
            })
        }
    }

    impl WebSession for FakeWeb {
        fn refresh(&self, provider: &str) -> Result<Option<String>, String> {
            assert_eq!(provider, "test");
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            self.next.lock().unwrap().remove(0)
        }
        fn clear(&self, _provider: &str) -> Result<(), String> {
            self.clears.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn account(web: Arc<FakeWeb>) -> (WebAccount, Arc<AtomicI64>, Arc<MemoryTokenStore>) {
        let now = Arc::new(AtomicI64::new(1_000));
        let clock_now = now.clone();
        let store = Arc::new(MemoryTokenStore::new());
        let account = WebAccount::with(
            SPEC,
            store.clone(),
            Some(web),
            Arc::new(move || clock_now.load(Ordering::SeqCst)),
        );
        (account, now, store)
    }

    #[test]
    fn parses_captures() {
        let capture =
            parse_web_capture(PREFIX, &format!("{PREFIX}?name=Ann%20%231&games=%5B1%5D&x"))
                .unwrap()
                .unwrap();
        assert_eq!(capture.get("name"), Some("Ann #1"));
        assert_eq!(capture.get("games"), Some("[1]"));
        assert_eq!(capture.get("x"), None);
        assert!(!format!("{capture:?}").contains("Ann"));
        assert_eq!(
            parse_web_capture(PREFIX, &format!("{PREFIX}?signedout=1")).unwrap(),
            None
        );
        for bad in [
            format!("{PREFIX}x?name=a"),
            "https://evil.test/?name=a".to_string(),
        ] {
            assert!(parse_web_capture(PREFIX, &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn signs_in_from_a_capture_and_reuses_it_briefly() {
        let web = FakeWeb::new(vec![Ok(Some(format!("{PREFIX}?games=2")))]);
        let (account, now, store) = account(web.clone());
        assert!(!account.status().signed_in);
        assert!(matches!(
            account.capture(60),
            Err(ProviderError::NotConfigured(_))
        ));
        account
            .sign_in_with_code(&format!("{PREFIX}?name=Ann&games=1"))
            .unwrap();
        let status = account.status();
        assert!(status.signed_in && !status.supports_code_sign_in);
        assert_eq!(status.display_name.as_deref(), Some("Ann"));
        assert_eq!(
            store.load("test").unwrap().unwrap(),
            r#"{"displayName":"Ann"}"#
        );

        assert_eq!(account.capture(60).unwrap().get("games"), Some("1"));
        assert_eq!(web.refreshes.load(Ordering::SeqCst), 0);
        now.store(1_100, Ordering::SeqCst);
        // A capture without a name keeps the stored one.
        assert_eq!(account.capture(60).unwrap().get("games"), Some("2"));
        assert_eq!(account.status().display_name.as_deref(), Some("Ann"));
        assert_eq!(web.refreshes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn signs_in_through_the_session_when_the_login_ends_elsewhere() {
        let web = FakeWeb::new(vec![
            Ok(Some(format!("{PREFIX}?token=t"))),
            Ok(Some(format!("{PREFIX}?signedout=1"))),
        ]);
        let (account, _, _) = account(web.clone());
        account
            .sign_in_with_code("https://store.test/done?code=1")
            .unwrap();
        account.set_display_name("  Bob  ");
        assert_eq!(account.status().display_name.as_deref(), Some("Bob"));
        account.sign_out().unwrap();
        assert!(matches!(
            account.sign_in_with_code("https://store.test/done"),
            Err(AuthError::Rejected(_))
        ));
        assert!(!account.is_signed_in());
    }

    #[test]
    fn transient_failures_keep_the_account_and_signed_out_sessions_forget_it() {
        let web = FakeWeb::new(vec![
            Err("The store did not answer.".into()),
            Ok(Some(format!("{PREFIX}?signedout=1"))),
        ]);
        let (account, now, store) = account(web.clone());
        account
            .sign_in_with_code(&format!("{PREFIX}?name=Ann"))
            .unwrap();
        now.store(5_000, Ordering::SeqCst);
        assert!(matches!(
            account.capture(60),
            Err(ProviderError::Network(_))
        ));
        assert!(account.is_signed_in());
        assert!(matches!(
            account.capture(60),
            Err(ProviderError::NotConfigured(_))
        ));
        assert!(!account.is_signed_in());
        assert!(store.load("test").unwrap().is_none());
    }

    #[test]
    fn sign_out_clears_the_browser_session() {
        let web = FakeWeb::new(vec![]);
        let (account, _, _) = account(web.clone());
        account
            .sign_in_with_code(&format!("{PREFIX}?name=Ann"))
            .unwrap();
        account.sign_out().unwrap();
        assert!(!account.status().signed_in);
        assert_eq!(web.clears.load(Ordering::SeqCst), 1);
    }
}
