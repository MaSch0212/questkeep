//! Where sign-in tokens live: the system keyring (never a plain file).

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::AuthError;

/// Keyring service name; the keyring user is the provider id.
pub const KEYRING_SERVICE: &str = "dev.masch0212.questkeep";

/// Stores one opaque secret string per provider.
pub trait TokenStore: Send + Sync {
    fn load(&self, provider: &str) -> Result<Option<String>, AuthError>;
    fn save(&self, provider: &str, secret: &str) -> Result<(), AuthError>;
    /// Deleting something that does not exist is not an error.
    fn delete(&self, provider: &str) -> Result<(), AuthError>;
}

/// The tokens of one signed-in account.
///
/// Only the refresh token, account id and display name are persisted ([`StoredTokens`]): access
/// tokens are short-lived (and Epic's are longer than the Windows credential store accepts), so
/// they live in memory and are re-created from the refresh token after a restart.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds; `0` when unknown (always refreshed before use).
    pub expires_at: i64,
    pub account_id: Option<String>,
    pub display_name: Option<String>,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("account_id", &self.account_id)
            .field("display_name", &self.display_name)
            .finish()
    }
}

/// The persisted part of a [`TokenSet`].
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTokens {
    refresh_token: String,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
}

/// Credential blobs on Windows are limited to 2560 bytes (stored as UTF-16).
const MAX_STORED_CHARS: usize = 1200;

impl TokenSet {
    pub fn to_stored(&self) -> Result<String, AuthError> {
        let json = serde_json::to_string(&StoredTokens {
            refresh_token: self.refresh_token.clone(),
            account_id: self.account_id.clone(),
            display_name: self.display_name.clone(),
        })
        .map_err(|e| AuthError::Storage(e.to_string()))?;
        if json.chars().count() > MAX_STORED_CHARS {
            return Err(AuthError::Storage(
                "the sign-in data is too large for the system keyring".into(),
            ));
        }
        Ok(json)
    }

    /// Restores the persisted part; the access token is empty and expired, so the next use
    /// refreshes it. `None` for unreadable data (treated as signed out).
    pub fn from_stored(json: &str) -> Option<Self> {
        let stored: StoredTokens = serde_json::from_str(json).ok()?;
        (!stored.refresh_token.is_empty()).then_some(Self {
            access_token: String::new(),
            refresh_token: stored.refresh_token,
            expires_at: 0,
            account_id: stored.account_id,
            display_name: stored.display_name,
        })
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryTokenStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for MemoryTokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryTokenStore").finish_non_exhaustive()
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self, provider: &str) -> Result<Option<String>, AuthError> {
        Ok(self
            .secrets
            .lock()
            .map_err(|_| AuthError::Storage("poisoned".into()))?
            .get(provider)
            .cloned())
    }

    fn save(&self, provider: &str, secret: &str) -> Result<(), AuthError> {
        self.secrets
            .lock()
            .map_err(|_| AuthError::Storage("poisoned".into()))?
            .insert(provider.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, provider: &str) -> Result<(), AuthError> {
        self.secrets
            .lock()
            .map_err(|_| AuthError::Storage("poisoned".into()))?
            .remove(provider);
        Ok(())
    }
}

/// The system keyring: Windows Credential Manager, macOS Keychain, Linux Secret Service.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringTokenStore;

impl KeyringTokenStore {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
mod keyring_impl {
    use super::{AuthError, KEYRING_SERVICE, KeyringTokenStore, TokenStore};

    fn entry(provider: &str) -> Result<keyring::Entry, AuthError> {
        keyring::Entry::new(KEYRING_SERVICE, provider).map_err(map_error)
    }

    fn map_error(err: keyring::Error) -> AuthError {
        match err {
            keyring::Error::NoStorageAccess(e) => AuthError::NoKeyring(e.to_string()),
            keyring::Error::PlatformFailure(e) => AuthError::NoKeyring(e.to_string()),
            other => AuthError::Storage(other.to_string()),
        }
    }

    impl TokenStore for KeyringTokenStore {
        fn load(&self, provider: &str) -> Result<Option<String>, AuthError> {
            match entry(provider)?.get_password() {
                Ok(secret) => Ok(Some(secret)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(err) => Err(map_error(err)),
            }
        }

        fn save(&self, provider: &str, secret: &str) -> Result<(), AuthError> {
            entry(provider)?.set_password(secret).map_err(map_error)
        }

        fn delete(&self, provider: &str) -> Result<(), AuthError> {
            match entry(provider)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(err) => Err(map_error(err)),
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
impl TokenStore for KeyringTokenStore {
    fn load(&self, _provider: &str) -> Result<Option<String>, AuthError> {
        Err(AuthError::NoKeyring("unsupported platform".into()))
    }
    fn save(&self, _provider: &str, _secret: &str) -> Result<(), AuthError> {
        Err(AuthError::NoKeyring("unsupported platform".into()))
    }
    fn delete(&self, _provider: &str) -> Result<(), AuthError> {
        Err(AuthError::NoKeyring("unsupported platform".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> TokenSet {
        TokenSet {
            access_token: "ACCESSSECRET".into(),
            refresh_token: "REFRESHSECRET".into(),
            expires_at: 99,
            account_id: Some("acc".into()),
            display_name: Some("Name".into()),
        }
    }

    #[test]
    fn debug_never_shows_tokens() {
        let text = format!("{:?}", tokens());
        assert!(!text.contains("ACCESSSECRET") && !text.contains("REFRESHSECRET"));
        assert!(text.contains("Name"));
    }

    #[test]
    fn stored_form_keeps_only_the_refresh_token() {
        let json = tokens().to_stored().unwrap();
        assert!(!json.contains("ACCESSSECRET"));
        let back = TokenSet::from_stored(&json).unwrap();
        assert_eq!(back.refresh_token, "REFRESHSECRET");
        assert_eq!(back.access_token, "");
        assert_eq!(back.expires_at, 0);
        assert_eq!(back.display_name.as_deref(), Some("Name"));
        assert!(TokenSet::from_stored("garbage").is_none());
        assert!(TokenSet::from_stored(r#"{"refreshToken":""}"#).is_none());
    }

    #[test]
    fn oversized_blobs_are_refused() {
        let mut set = tokens();
        set.refresh_token = "x".repeat(2000);
        assert!(matches!(set.to_stored(), Err(AuthError::Storage(_))));
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryTokenStore::new();
        assert_eq!(store.load("gog").unwrap(), None);
        store.save("gog", "s").unwrap();
        assert_eq!(store.load("gog").unwrap().as_deref(), Some("s"));
        store.delete("gog").unwrap();
        store.delete("gog").unwrap();
        assert_eq!(store.load("gog").unwrap(), None);
    }
}
