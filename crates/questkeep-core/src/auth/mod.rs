//! Store account sign-in (GOG, Epic): token storage in the system keyring, a refreshing
//! [`AccountSession`] per provider, and the small HTTP and URL helpers they share.
//!
//! Secrets (authorization codes, access and refresh tokens, client secrets) are never logged,
//! never put in error messages and never written to `settings.json`; see `docs/DECISIONS.md`.

mod http;
mod session;
mod store;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use http::{Http, HttpRequest, HttpResponse, Method, UreqHttp, redact_secrets};
pub use session::{AccountSession, Clock, REFRESH_MARGIN_SECS, RefreshFn, system_clock};
pub use store::{KEYRING_SERVICE, KeyringTokenStore, MemoryTokenStore, TokenSet, TokenStore};

#[cfg(test)]
pub(crate) use http::testing;

use crate::error::CoreError;
use crate::providers::ProviderError;

/// Why a sign-in or token operation failed. The text is shown to users.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(
        "No system keyring available ({0}). Questkeep keeps sign-in tokens only in the system keyring, so signing in is not possible without one."
    )]
    NoKeyring(String),
    #[error("Could not use the system keyring: {0}")]
    Storage(String),
    #[error("network error: {0}")]
    Network(String),
    /// The store refused a code or token.
    #[error("{0}")]
    Rejected(String),
    #[error("{0}")]
    Parse(String),
}

impl From<AuthError> for ProviderError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::Network(m) => ProviderError::Network(m),
            AuthError::Parse(m) => ProviderError::Parse(m),
            other => ProviderError::NotConfigured(other.to_string()),
        }
    }
}

impl From<AuthError> for CoreError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::Network(m) => CoreError::Network(m),
            other => CoreError::Invalid(other.to_string()),
        }
    }
}

/// What the UI shows about one store account. This exact shape is the UI contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountStatus {
    /// Provider id, e.g. `gog`.
    pub provider: String,
    /// Store name, e.g. `GOG`.
    pub name: String,
    pub signed_in: bool,
    pub display_name: Option<String>,
    /// Whether the user can paste a code (or URL) instead of using the login window.
    pub supports_code_sign_in: bool,
    /// Page the user opens in their normal browser to obtain the code.
    pub code_url: Option<String>,
}

/// Account operations of a provider that supports signing in.
pub trait AccountAuth: Send + Sync {
    fn status(&self) -> AccountStatus;
    /// The page the in-app login window opens.
    fn login_url(&self) -> String;
    /// Completes a sign-in from what the user pasted (a code, or for GOG also the redirect URL)
    /// or what the login window captured; stores the tokens.
    fn sign_in_with_code(&self, input: &str) -> Result<(), AuthError>;
    /// Forgets the tokens.
    fn sign_out(&self) -> Result<(), AuthError>;
}

/// Percent-encodes everything except unreserved characters.
pub fn url_encode(text: &str) -> String {
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

/// Decodes `%XX` escapes and `+`; invalid escapes are kept as they are.
pub fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len()
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit() =>
            {
                let hex = &text[i + 1..i + 3];
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'%'));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The decoded value of query parameter `name` in a URL (or bare query string).
pub fn query_param(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?').map_or(url, |(_, q)| q);
    let query = query.split('#').next().unwrap_or_default();
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| url_decode(value))
    })
}

/// Accepts only plausible authorization codes: URL-safe characters, sensible length.
pub fn plausible_code(code: &str) -> Option<String> {
    let code = code.trim().trim_matches(['"', '\'']).trim();
    let ok = (8..=512).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    ok.then(|| code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_urls() {
        assert_eq!(url_encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(url_decode("a%20b%26c+d%2"), "a b&c d%2");
        assert_eq!(url_decode(&url_encode("x y/z?é")), "x y/z?é");
    }

    #[test]
    fn reads_query_parameters() {
        let url = "https://embed.gog.com/on_login_success?origin=client&code=ab%2Fc_D-9#frag";
        assert_eq!(query_param(url, "code").as_deref(), Some("ab/c_D-9"));
        assert_eq!(query_param(url, "origin").as_deref(), Some("client"));
        assert_eq!(query_param(url, "nope"), None);
        assert_eq!(
            query_param("code=xyz12345", "code").as_deref(),
            Some("xyz12345")
        );
    }

    #[test]
    fn code_plausibility() {
        assert_eq!(
            plausible_code(" \"abcd1234\" ").as_deref(),
            Some("abcd1234")
        );
        assert_eq!(plausible_code("short"), None);
        assert_eq!(plausible_code("has space inside 12345"), None);
        assert_eq!(plausible_code("https://x/?code=1"), None);
    }

    #[test]
    fn status_serializes_to_the_ui_contract() {
        let status = AccountStatus {
            provider: "gog".into(),
            name: "GOG".into(),
            signed_in: true,
            display_name: Some("Ann".into()),
            supports_code_sign_in: true,
            code_url: None,
        };
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            serde_json::json!({
                "provider": "gog", "name": "GOG", "signedIn": true, "displayName": "Ann",
                "supportsCodeSignIn": true, "codeUrl": null
            })
        );
    }

    #[test]
    fn keyring_error_text_is_readable() {
        let text = AuthError::NoKeyring("no secret service".into()).to_string();
        assert!(text.starts_with("No system keyring available (no secret service)"));
    }
}
