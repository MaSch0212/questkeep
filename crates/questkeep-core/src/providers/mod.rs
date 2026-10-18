//! Library providers: one per store. Adding a provider means implementing [`LibraryProvider`] and
//! registering it; UI and database code stay untouched.

pub mod manual;
pub mod steam;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::ImportedGame;

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
    /// Games owned in the store account (installed or not). Not implemented by any v0.1 provider.
    fn scan_owned(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        Err(ProviderError::Unsupported)
    }
    /// Where a portrait cover for the game may be found, best first. Local files are preferred
    /// over downloads by convention.
    fn cover_candidates(&self, _source_game_id: &str) -> Vec<CoverCandidate> {
        Vec::new()
    }
}

/// A possible source for a game's cover image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverCandidate {
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
                can_scan: true
            }]
        );
        assert!(matches!(
            registry.get("fake").unwrap().scan_owned(),
            Err(ProviderError::Unsupported)
        ));
    }
}
