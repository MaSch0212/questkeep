//! Library providers: one per store. Adding a provider means implementing [`LibraryProvider`] and
//! registering it; UI and database code stay untouched.

pub mod epic;
pub mod manual;
pub mod steam;
pub mod xbox;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{GameMetadata, ImportedGame, PlayAction};

/// Errors a provider can report. Messages are shown to users.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The store client is not installed or could not be located.
    #[error("{0}")]
    NotFound(String),
    #[error("could not read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Parse(String),
    #[error("this provider does not support that operation")]
    Unsupported,
    /// The operation needs user configuration (e.g. an API key) that is missing.
    #[error("{0}")]
    NotConfigured(String),
    #[error("network error: {0}")]
    Network(String),
}

/// A store integration that reports games. Implementations must be plain data in, plain data out
/// (no UI or database access) so they can later run as sandboxed plugins.
pub trait LibraryProvider: Send + Sync {
    /// Stable id stored in `games.source`, e.g. `steam`.
    fn id(&self) -> &str;
    /// Human-readable name, e.g. `Steam`.
    fn name(&self) -> &str;
    /// Whether [`scan_installed`](Self::scan_installed) is meaningful for this provider.
    fn can_scan(&self) -> bool {
        true
    }
    /// Games currently installed on this machine.
    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError>;
    /// Whether [`scan_owned`](Self::scan_owned) is implemented.
    fn can_scan_owned(&self) -> bool {
        false
    }
    /// Games owned in the store account, installed or not. Entries for games that are not
    /// installed must have `installed: false`. `config` is this provider's section of the user
    /// settings (`settings.providers.<id>`, `null` when absent), e.g. an API key. Return
    /// `NotConfigured` when required configuration is missing.
    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        Err(ProviderError::Unsupported)
    }
    /// How to install a game that is owned but not installed (usually a store URI such as
    /// `steam://install/440`). `None` when the provider can't install games.
    fn install_action(&self, _source_game_id: &str) -> Option<PlayAction> {
        None
    }
    /// Store metadata (description, genres, ...). Called from a background thread with at most
    /// one request in flight per provider; `Ok(None)` means the store has no data for this game.
    fn metadata(&self, _source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        Ok(None)
    }
    /// Where a portrait cover (2:3) for the game may be found, best first. Local files are
    /// preferred over downloads by convention.
    fn cover_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// Where a wide background / hero image for the game may be found, best first.
    fn background_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
}

/// A possible source for a game image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageCandidate {
    File(std::path::PathBuf),
    Url(String),
}

/// Public description of a registered provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub can_scan: bool,
    pub can_scan_owned: bool,
}

/// Providers looked up by id.
#[derive(Clone, Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Arc<dyn LibraryProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The providers shipped with Questkeep.
    pub fn with_builtin() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(manual::ManualProvider));
        registry.register(Arc::new(steam::SteamProvider::detect()));
        registry.register(Arc::new(epic::EpicProvider::detect()));
        registry.register(Arc::new(xbox::XboxProvider::detect()));
        registry
    }

    /// Adds or replaces the provider with the same id.
    pub fn register(&mut self, provider: Arc<dyn LibraryProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn LibraryProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn infos(&self) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|p| ProviderInfo {
                id: p.id().to_string(),
                name: p.name().to_string(),
                can_scan: p.can_scan(),
                can_scan_owned: p.can_scan_owned(),
            })
            .collect()
    }
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.providers.keys()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl LibraryProvider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn name(&self) -> &str {
            "Fake Store"
        }
        fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
            Ok(vec![])
        }
    }

    #[test]
    fn registry_looks_up_by_id() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(Fake));
        assert_eq!(registry.get("fake").unwrap().name(), "Fake Store");
        assert!(registry.get("nope").is_none());
        assert_eq!(
            registry.infos(),
            vec![ProviderInfo {
                id: "fake".into(),
                name: "Fake Store".into(),
                can_scan: true,
                can_scan_owned: false
            }]
        );
        assert!(matches!(
            registry
                .get("fake")
                .unwrap()
                .scan_owned(&serde_json::Value::Null),
            Err(ProviderError::Unsupported)
        ));
    }
}
