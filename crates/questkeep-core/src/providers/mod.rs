//! The provider registry. Stores are plugins: each lives in its own crate (`crates/stores/*`),
//! implements [`LibraryProvider`] from `questkeep-store-api`, and the app registers it here. Only
//! the manual provider is part of the core.

pub mod manual;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use questkeep_store_api::settings::StoreSettings;
pub use questkeep_store_api::{
    AddGameOption, ImageCandidate, LibraryProvider, ProviderError, platform, sqlite,
};

/// Public description of a registered provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    /// The store's logo as SVG markup (see [`LibraryProvider::icon_svg`]).
    pub icon: String,
    pub can_scan: bool,
    pub can_scan_owned: bool,
    pub can_uninstall: bool,
    /// Games can be shown in the store's launcher (see [`LibraryProvider::launcher_page`]).
    pub launcher_pages: bool,
    /// The store's website for buying games (see [`LibraryProvider::shop_url`]).
    pub shop_url: Option<String>,
    /// The store's client is on this PC (see [`LibraryProvider::is_installed`]).
    pub installed: bool,
    /// Syncing does something: the client is installed or owned games can be read without it.
    pub syncable: bool,
    /// Extra ways to add games (see [`LibraryProvider::add_game_options`]).
    pub add_game_options: Vec<AddGameOption>,
    /// The store's own settings (see [`LibraryProvider::settings`]).
    pub settings: StoreSettings,
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

    /// The manual provider plus the given stores (the app passes `questkeep_stores::builtin`).
    pub fn with_stores(stores: impl IntoIterator<Item = Arc<dyn LibraryProvider>>) -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(manual::ManualProvider));
        for store in stores {
            registry.register(store);
        }
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

    /// The secret settings fields of all providers, as (provider id, key).
    pub fn secret_settings(&self) -> Vec<(String, String)> {
        self.providers
            .values()
            .flat_map(|p| {
                let id = p.id().to_string();
                p.settings()
                    .secret_keys()
                    .map(|key| (id.clone(), key.to_string()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Every provider's info; `config` gives each provider's settings section (for
    /// [`LibraryProvider::has_owned_source`]).
    pub fn infos(&self, config: impl Fn(&str) -> serde_json::Value) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|p| {
                let installed = p.is_installed();
                ProviderInfo {
                    id: p.id().to_string(),
                    name: p.name().to_string(),
                    icon: p.icon_svg().to_string(),
                    can_scan: p.can_scan(),
                    can_scan_owned: p.can_scan_owned(),
                    can_uninstall: p.can_uninstall(),
                    launcher_pages: p.has_launcher_pages(),
                    shop_url: p.shop_url().map(str::to_string),
                    installed,
                    syncable: p.can_scan() && (installed || p.has_owned_source(&config(p.id()))),
                    add_game_options: if installed {
                        p.add_game_options()
                    } else {
                        Vec::new()
                    },
                    settings: p.settings(),
                }
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
    use crate::model::ImportedGame;

    struct Fake;
    impl LibraryProvider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn name(&self) -> &str {
            "Fake Store"
        }
        fn icon_svg(&self) -> &'static str {
            "<svg/>"
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
            registry.infos(|_| serde_json::Value::Null),
            vec![ProviderInfo {
                id: "fake".into(),
                name: "Fake Store".into(),
                icon: "<svg/>".into(),
                can_scan: true,
                can_scan_owned: false,
                can_uninstall: false,
                launcher_pages: false,
                shop_url: None,
                installed: false,
                syncable: false,
                add_game_options: Vec::new(),
                settings: StoreSettings::default(),
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
