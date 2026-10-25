//! Steam account sign-in through the Steam store website, the way Playnite's Steam library does
//! it: Steam has no sign-in for other applications, but every store page of a signed-in user
//! carries a short-lived Web API token (`webapi_token` in the `application_config` element), which
//! `IPlayerService/GetOwnedGames` accepts in place of an API key.
//!
//! The app keeps the store session in its own browser profile ([`WebSession`]): the login window
//! signs in there, and before a scan a hidden window loads a store page with that session (Steam
//! renews the session's cookies itself) and reports the page's token through a capture address
//! (`CAPTURE_PREFIX?steamid=...&token=...&name=...`, or `?signedout=1`). The core only stores the
//! SteamID and the persona name; the token lives in memory until it expires.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::ProviderError;
use super::STEAM_SOURCE;
use crate::auth::{
    AccountAuth, AccountStatus, AuthError, Clock, REFRESH_MARGIN_SECS, TokenStore, WebSession,
    query_param, system_clock,
};

/// The page the login window opens; Steam returns to the store front page after signing in.
pub const LOGIN_URL: &str = "https://store.steampowered.com/login/?redir=explore%2F";
/// The store page the hidden window loads to renew the token.
pub const SESSION_URL: &str = "https://store.steampowered.com/explore/";
/// Navigations to this address carry what the store page reported; the window never loads it.
pub const CAPTURE_PREFIX: &str = "https://questkeep.invalid/steam-session";
/// Assumed lifetime of a token whose expiry cannot be read.
const DEFAULT_LIFETIME_SECS: i64 = 3600;

/// What a store page reported about the signed-in user.
#[derive(Clone, PartialEq, Eq)]
pub struct StoreSession {
    pub steam_id: String,
    pub token: String,
    pub display_name: Option<String>,
}

impl std::fmt::Debug for StoreSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreSession")
            .field("steam_id", &self.steam_id)
            .field("token", &"<redacted>")
            .field("display_name", &self.display_name)
            .finish()
    }
}

fn is_steam_id(id: &str) -> bool {
    id.len() == 17 && id.bytes().all(|b| b.is_ascii_digit())
}

/// Tokens are JWTs: three dot-separated URL-safe base64 parts.
fn is_plausible_token(token: &str) -> bool {
    (16..=4096).contains(&token.len())
        && token.split('.').count() == 3
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Reads a capture address. `Ok(None)` means the page saw no signed-in user.
pub fn parse_capture(url: &str) -> Result<Option<StoreSession>, AuthError> {
    let rejected =
        || AuthError::Rejected("Steam did not report a signed-in account. Sign in again.".into());
    let Some(rest) = url.strip_prefix(CAPTURE_PREFIX) else {
        return Err(rejected());
    };
    if !(rest.is_empty() || rest.starts_with('?')) {
        return Err(rejected());
    }
    if query_param(rest, "signedout").is_some() {
        return Ok(None);
    }
    let steam_id = query_param(rest, "steamid").filter(|id| is_steam_id(id));
    let token = query_param(rest, "token").filter(|t| is_plausible_token(t));
    let (Some(steam_id), Some(token)) = (steam_id, token) else {
        return Err(rejected());
    };
    let display_name = query_param(rest, "name")
        .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().take(64).collect());
    Ok(Some(StoreSession {
        steam_id,
        token,
        display_name,
    }))
}

/// The `exp` claim of a JWT, without checking the signature (Steam checks it).
pub fn token_expiry(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp")?.as_i64()
}

/// The persisted part: who is signed in (not secret, but kept with the other accounts).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    steam_id: String,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Default)]
struct State {
    loaded: bool,
    account: Option<StoredAccount>,
    /// The current Web API token and its expiry (Unix seconds).
    token: Option<(String, i64)>,
}

pub struct SteamAccount {
    tokens: Arc<dyn TokenStore>,
    web: Option<Arc<dyn WebSession>>,
    clock: Clock,
    state: Mutex<State>,
}

impl std::fmt::Debug for SteamAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SteamAccount").finish_non_exhaustive()
    }
}

impl SteamAccount {
    /// `web` is the app's store session; without it (tests, no app) the token cannot be renewed.
    pub fn new(tokens: Arc<dyn TokenStore>, web: Option<Arc<dyn WebSession>>) -> Self {
        Self::with(tokens, web, system_clock())
    }

    pub fn with(
        tokens: Arc<dyn TokenStore>,
        web: Option<Arc<dyn WebSession>>,
        clock: Clock,
    ) -> Self {
        Self {
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
        let stored = self.tokens.load(STEAM_SOURCE)?;
        state.account = stored
            .as_deref()
            .and_then(|json| serde_json::from_str::<StoredAccount>(json).ok())
            .filter(|a| is_steam_id(&a.steam_id));
        state.loaded = true;
        Ok(())
    }

    pub fn is_signed_in(&self) -> bool {
        let mut state = self.lock();
        self.load(&mut state).is_ok() && state.account.is_some()
    }

    fn save(&self, state: &mut State, session: &StoreSession) -> Result<(), AuthError> {
        let account = StoredAccount {
            steam_id: session.steam_id.clone(),
            display_name: session.display_name.clone(),
        };
        if state.account.as_ref() != Some(&account) {
            let json =
                serde_json::to_string(&account).map_err(|e| AuthError::Storage(e.to_string()))?;
            self.tokens.save(STEAM_SOURCE, &json)?;
        }
        let now = (self.clock)();
        let expires_at = token_expiry(&session.token).unwrap_or(now + DEFAULT_LIFETIME_SECS);
        state.loaded = true;
        state.account = Some(account);
        state.token = Some((session.token.clone(), expires_at));
        Ok(())
    }

    fn forget(&self, state: &mut State) {
        if let Err(err) = self.tokens.delete(STEAM_SOURCE) {
            tracing::warn!(%err, "could not delete the Steam account");
        }
        state.loaded = true;
        state.account = None;
        state.token = None;
    }

    /// SteamID and a Web API token of the signed-in account. `renew` asks the store session for
    /// a new token even when the current one has not expired (after Steam rejected it).
    pub fn web_token(&self, renew: bool) -> Result<(String, String), ProviderError> {
        {
            let mut state = self.lock();
            self.load(&mut state)
                .map_err(|e| ProviderError::NotConfigured(e.to_string()))?;
            let Some(account) = &state.account else {
                return Err(ProviderError::NotConfigured(
                    "Sign in to Steam in Settings.".into(),
                ));
            };
            let now = (self.clock)();
            if !renew
                && let Some((token, expires_at)) = &state.token
                && expires_at - now > REFRESH_MARGIN_SECS
            {
                return Ok((account.steam_id.clone(), token.clone()));
            }
        }
        let Some(web) = &self.web else {
            return Err(ProviderError::NotConfigured(
                "Steam sign-in needs the Questkeep app.".into(),
            ));
        };
        // Not under the lock: the store page can take a while, and the status must stay readable.
        let captured = web.refresh(STEAM_SOURCE).map_err(ProviderError::Network)?;
        let mut state = self.lock();
        if state.account.is_none() {
            // Signed out while the page loaded.
            return Err(ProviderError::NotConfigured(
                "Sign in to Steam in Settings.".into(),
            ));
        }
        let session = match captured.as_deref().map(parse_capture) {
            Some(Ok(Some(session))) => session,
            None | Some(Ok(None)) => {
                tracing::info!("the Steam store session is signed out");
                self.forget(&mut state);
                return Err(ProviderError::NotConfigured(
                    "Sign in to Steam again in Settings.".into(),
                ));
            }
            Some(Err(err)) => return Err(ProviderError::Parse(err.to_string())),
        };
        if let Err(err) = self.save(&mut state, &session) {
            tracing::warn!(%err, "could not save the Steam account");
        }
        Ok((session.steam_id, session.token))
    }
}

impl AccountAuth for SteamAccount {
    fn status(&self) -> AccountStatus {
        let mut state = self.lock();
        let account = self.load(&mut state).ok().and(state.account.clone());
        AccountStatus {
            provider: STEAM_SOURCE.into(),
            name: "Steam".into(),
            signed_in: account.is_some(),
            display_name: account.and_then(|a| a.display_name),
            // The store session lives in the login window's browser profile; there is no code.
            supports_code_sign_in: false,
            code_url: None,
        }
    }

    fn login_url(&self) -> String {
        LOGIN_URL.into()
    }

    /// `input` is the capture address the login window caught.
    fn sign_in_with_code(&self, input: &str) -> Result<(), AuthError> {
        let session = parse_capture(input.trim())?.ok_or_else(|| {
            AuthError::Rejected("Steam did not report a signed-in account. Sign in again.".into())
        })?;
        let mut state = self.lock();
        self.save(&mut state, &session)
    }

    fn sign_out(&self) -> Result<(), AuthError> {
        let result = self.tokens.delete(STEAM_SOURCE);
        {
            let mut state = self.lock();
            state.loaded = true;
            state.account = None;
            state.token = None;
        }
        if let Some(web) = &self.web
            && let Err(err) = web.clear(STEAM_SOURCE)
        {
            tracing::warn!(%err, "could not clear the Steam store session");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    use super::*;
    use crate::auth::MemoryTokenStore;

    const STEAM_ID: &str = "76561197960287930";

    /// A JWT-shaped token expiring at `exp`.
    fn token(exp: i64) -> String {
        let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"sub":"{STEAM_ID}","exp":{exp}}}"#));
        format!("eyJhbGciOiJFZERTQSJ9.{payload}.c2lnbmF0dXJl")
    }

    fn capture(token: &str, name: &str) -> String {
        format!(
            "{CAPTURE_PREFIX}?steamid={STEAM_ID}&token={token}&name={}",
            crate::auth::url_encode(name)
        )
    }

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
            assert_eq!(provider, "steam");
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            self.next.lock().unwrap().remove(0)
        }
        fn clear(&self, _provider: &str) -> Result<(), String> {
            self.clears.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn account(web: Arc<FakeWeb>) -> (SteamAccount, Arc<AtomicI64>, Arc<MemoryTokenStore>) {
        let now = Arc::new(AtomicI64::new(1_000));
        let clock_now = now.clone();
        let store = Arc::new(MemoryTokenStore::new());
        let account = SteamAccount::with(
            store.clone(),
            Some(web),
            Arc::new(move || clock_now.load(Ordering::SeqCst)),
        );
        (account, now, store)
    }

    #[test]
    fn parses_captures() {
        let session = parse_capture(&capture(&token(5_000), "  Gabe   N "))
            .unwrap()
            .unwrap();
        assert_eq!(session.steam_id, STEAM_ID);
        assert_eq!(session.token, token(5_000));
        assert_eq!(session.display_name.as_deref(), Some("Gabe N"));
        assert!(!format!("{session:?}").contains(&token(5_000)));
        assert_eq!(
            parse_capture(&format!("{CAPTURE_PREFIX}?signedout=1")).unwrap(),
            None
        );
        for bad in [
            format!("{CAPTURE_PREFIX}?steamid=123&token={}", token(1)),
            format!("{CAPTURE_PREFIX}?steamid={STEAM_ID}&token=not-a-jwt"),
            format!("{CAPTURE_PREFIX}x?steamid={STEAM_ID}&token={}", token(1)),
            format!("https://evil.test/?steamid={STEAM_ID}&token={}", token(1)),
        ] {
            assert!(parse_capture(&bad).is_err(), "{bad}");
        }
        assert_eq!(token_expiry(&token(5_000)), Some(5_000));
        assert_eq!(token_expiry("a.b.c"), None);
    }

    #[test]
    fn signs_in_from_the_login_window_and_reuses_the_token() {
        let web = FakeWeb::new(vec![]);
        let (account, _, store) = account(web.clone());
        assert!(!account.status().signed_in);
        account
            .sign_in_with_code(&capture(&token(5_000), "Gabe"))
            .unwrap();
        let status = account.status();
        assert!(status.signed_in && !status.supports_code_sign_in);
        assert_eq!(status.display_name.as_deref(), Some("Gabe"));
        // Only who is signed in is stored, never the token.
        let stored = store.load("steam").unwrap().unwrap();
        assert!(stored.contains(STEAM_ID) && !stored.contains(&token(5_000)));

        assert_eq!(
            account.web_token(false).unwrap(),
            (STEAM_ID.into(), token(5_000))
        );
        assert_eq!(web.refreshes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn renews_expired_tokens_and_forgets_signed_out_sessions() {
        let web = FakeWeb::new(vec![
            Ok(Some(capture(&token(9_000), "Gabe"))),
            Err("Steam did not answer.".into()),
            Ok(Some(format!("{CAPTURE_PREFIX}?signedout=1"))),
        ]);
        let (account, now, store) = account(web.clone());
        account
            .sign_in_with_code(&capture(&token(2_000), "Gabe"))
            .unwrap();
        now.store(1_990, Ordering::SeqCst);
        assert_eq!(account.web_token(false).unwrap().1, token(9_000));
        // Transient failures keep the account.
        assert!(matches!(
            account.web_token(true),
            Err(ProviderError::Network(_))
        ));
        assert!(account.is_signed_in());
        // A signed-out session signs the account out.
        assert!(matches!(
            account.web_token(true),
            Err(ProviderError::NotConfigured(_))
        ));
        assert!(!account.is_signed_in());
        assert!(store.load("steam").unwrap().is_none());
        assert_eq!(web.refreshes.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn sign_out_clears_the_browser_session() {
        let web = FakeWeb::new(vec![]);
        let (account, _, _) = account(web.clone());
        account
            .sign_in_with_code(&capture(&token(5_000), ""))
            .unwrap();
        account.sign_out().unwrap();
        assert!(!account.status().signed_in);
        assert_eq!(web.clears.load(Ordering::SeqCst), 1);
        assert!(matches!(
            account.web_token(false),
            Err(ProviderError::NotConfigured(_))
        ));
    }
}
