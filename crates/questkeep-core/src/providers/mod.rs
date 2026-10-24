//! Library providers: one per store. Adding a provider means implementing [`LibraryProvider`] and
//! registering it; UI and database code stay untouched.

pub mod amazon;
pub mod epic;
pub mod gog;
pub mod heroic;
pub mod heroic_sideload;
pub mod lutris;
pub mod manual;
mod sqlite;
pub mod steam;
pub mod xbox;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::auth::{AccountAuth, TokenStore};
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
    /// Whether [`uninstall_action`](Self::uninstall_action) can work at all on this machine (the
    /// UI only offers Uninstall for such providers). Individual games may still fail, e.g. when
    /// their uninstaller is missing.
    fn can_uninstall(&self) -> bool {
        false
    }
    /// How to hand the uninstall of an installed game to its store client (e.g. the URI
    /// `steam://uninstall/440`, or the game's own uninstaller). The store or uninstaller asks the
    /// user to confirm; Questkeep never deletes files itself. `None` when this game can't be
    /// uninstalled from Questkeep.
    fn uninstall_action(&self, _source_game_id: &str) -> Option<PlayAction> {
        None
    }
    /// Store metadata (description, genres, ...). Called from a background thread with at most
    /// one request in flight per provider; `Ok(None)` means the store has no data for this game.
    fn metadata(&self, _source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        Ok(None)
    }
    /// Applies the provider's settings (`providers.<id>` in settings.json, `Null` when unset).
    /// Called before every scan, so a changed setting takes effect with the next scan.
    fn configure(&self, _config: &serde_json::Value) {}
    /// Account sign-in for this store, when the provider supports it (see [`crate::auth`]).
    fn account(&self) -> Option<&dyn AccountAuth> {
        None
    }
    /// Where a portrait cover (2:3) for the game may be found, best first. Local files are
    /// preferred over downloads by convention.
    fn cover_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// Whether this game's cover candidates are poor stand-ins for real 2:3 box art (e.g. square
    /// app logos). When true and a SteamGridDB key is configured, SteamGridDB is tried first and
    /// the provider's candidates only serve as fallback. Called right before
    /// [`cover_candidates`](Self::cover_candidates) for the same game.
    fn covers_are_low_quality(&self, _source_game_id: &str) -> bool {
        false
    }
    /// Where a wide background / hero image for the game may be found, best first.
    fn background_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
        Vec::new()
    }
    /// Where a small square game icon may be found, best first (local files before downloads).
    /// Called from a background thread, only for games that still lack an icon. The image
    /// fetcher decodes each candidate, rejects non-square ones and scales the first valid one to
    /// 32x32, so a candidate that turns out unusable is simply skipped. When this is empty (the
    /// default) the icon comes from the executable or, last, from the cover.
    fn icon_candidates(&self, _source_game_id: &str) -> Vec<ImageCandidate> {
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
    pub can_uninstall: bool,
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

    /// The providers shipped with Questkeep. Providers that support account sign-in keep their
    /// tokens in `tokens` (the system keyring in the app). Store answers worth keeping between
    /// runs go below `cache_dir` (`DataPaths::cache_dir`); `None` keeps them in memory only.
    pub fn with_builtin(tokens: Arc<dyn TokenStore>, cache_dir: Option<&Path>) -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(manual::ManualProvider));
        registry.register(Arc::new(steam::SteamProvider::detect()));
        let mut epic = epic::EpicProvider::detect()
            .with_account(Arc::new(epic::EpicAccount::new(tokens.clone())));
        if let Some(dir) = cache_dir {
            epic = epic.with_cache_dir(dir);
        }
        registry.register(Arc::new(epic));
        registry.register(Arc::new(
            xbox::XboxProvider::detect()
                .with_account(Arc::new(xbox::XboxAccount::new(tokens.clone()))),
        ));
        registry.register(Arc::new(
            gog::GogProvider::detect().with_account(Arc::new(gog::GogAccount::new(tokens))),
        ));
        // Lutris only exists on Linux; leaving it out elsewhere keeps the provider list honest.
        #[cfg(not(windows))]
        registry.register(Arc::new(lutris::LutrisProvider::detect()));
        #[cfg(not(windows))]
        registry.register(Arc::new(amazon::AmazonProvider::detect()));
        #[cfg(not(windows))]
        registry.register(Arc::new(heroic_sideload::HeroicSideloadProvider::detect()));
        registry
    }

    /// Adds or replaces the provider with the same id.
    pub fn register(&mut self, provider: Arc<dyn LibraryProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn LibraryProvider>> {
        self.providers.get(id).cloned()
    }

    /// All providers, ordered by id.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn LibraryProvider>> {
        self.providers.values()
    }

    pub fn infos(&self) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|p| ProviderInfo {
                id: p.id().to_string(),
                name: p.name().to_string(),
                can_scan: p.can_scan(),
                can_scan_owned: p.can_scan_owned(),
                can_uninstall: p.can_uninstall(),
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
    fn builtin_epic_and_gog_support_account_sign_in() {
        let registry =
            ProviderRegistry::with_builtin(Arc::new(crate::auth::MemoryTokenStore::new()), None);
        for id in ["epic", "gog"] {
            let provider = registry.get(id).unwrap();
            assert!(provider.can_scan_owned(), "{id}");
            let status = provider.account().unwrap().status();
            assert_eq!(status.provider, id);
            assert!(!status.signed_in && status.supports_code_sign_in && status.code_url.is_some());
            // Signed out: owned scan is silently skipped by Library (NotConfigured).
            assert!(matches!(
                provider.scan_owned(&serde_json::Value::Null),
                Err(ProviderError::NotConfigured(_))
            ));
        }
        assert!(registry.get("steam").unwrap().account().is_none());
        assert!(matches!(
            registry.get("epic").unwrap().metadata("Sugar"),
            Err(ProviderError::NotConfigured(_))
        ));
        assert!(
            registry
                .get("epic")
                .unwrap()
                .cover_candidates("Sugar")
                .is_empty()
        );
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
                can_scan_owned: false,
                can_uninstall: false
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
