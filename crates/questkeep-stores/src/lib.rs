//! The store plugins shipped with Questkeep. Each store is its own crate implementing
//! [`LibraryProvider`]; this crate only lists them. Adding a store means adding its crate here.

use std::path::Path;
use std::sync::Arc;

use questkeep_store_api::LibraryProvider;
use questkeep_store_api::auth::{TokenStore, WebSession};

use questkeep_store_amazon as amazon;
use questkeep_store_epic as epic;
use questkeep_store_gog as gog;
use questkeep_store_prism as prism;
use questkeep_store_steam as steam;
use questkeep_store_ubisoft as ubisoft;
use questkeep_store_xbox as xbox;

/// The built-in stores. Stores that support account sign-in keep their tokens in `tokens` (the
/// system keyring in the app). Store answers worth keeping between runs go below `cache_dir`;
/// `None` keeps them in memory only. `web` keeps the store sessions of website sign-ins; the app
/// supplies it.
pub fn builtin(
    tokens: Arc<dyn TokenStore>,
    cache_dir: Option<&Path>,
    web: Option<Arc<dyn WebSession>>,
) -> Vec<Arc<dyn LibraryProvider>> {
    let mut stores: Vec<Arc<dyn LibraryProvider>> = Vec::new();
    stores.push(Arc::new(steam::SteamProvider::detect().with_account(
        Arc::new(steam::SteamAccount::new(tokens.clone(), web.clone())),
    )));
    let mut epic =
        epic::EpicProvider::detect().with_account(Arc::new(epic::EpicAccount::new(tokens.clone())));
    if let Some(dir) = cache_dir {
        epic = epic.with_cache_dir(dir);
    }
    stores.push(Arc::new(epic));
    stores.push(Arc::new(
        xbox::XboxProvider::detect().with_account(Arc::new(xbox::XboxAccount::new(tokens.clone()))),
    ));
    stores.push(Arc::new(
        gog::GogProvider::detect().with_account(Arc::new(gog::GogAccount::new(tokens.clone()))),
    ));
    stores.push(Arc::new(ubisoft::UbisoftProvider::detect()));
    stores.push(Arc::new(prism::PrismProvider::detect()));
    // Battle.net and the EA app only exist on Windows (elsewhere they run under Wine, where Heroic
    // or Lutris already see them).
    #[cfg(windows)]
    {
        use questkeep_store_api::auth::WebAccount;
        use questkeep_store_battlenet as battlenet;
        use questkeep_store_ea as ea;
        let web_account = |spec| Arc::new(WebAccount::new(spec, tokens.clone(), web.clone()));
        stores.push(Arc::new(
            battlenet::BattleNetProvider::detect()
                .with_account(web_account(battlenet::ACCOUNT_SPEC)),
        ));
        stores.push(Arc::new(
            ea::EaProvider::detect().with_account(web_account(ea::ACCOUNT_SPEC)),
        ));
    }
    // Lutris only exists on Linux; leaving it out elsewhere keeps the store list honest.
    #[cfg(not(windows))]
    stores.push(Arc::new(questkeep_store_lutris::LutrisProvider::detect()));
    // Amazon reads the Amazon Games app (installed) and an Amazon sign-in (owned) on Windows, and
    // Heroic, which is signed in itself, elsewhere.
    let amazon = amazon::AmazonProvider::detect();
    let amazon = if cfg!(windows) {
        amazon.with_account(Arc::new(amazon::AmazonAccount::new(tokens)))
    } else {
        amazon
    };
    stores.push(Arc::new(amazon));
    #[cfg(not(windows))]
    stores.push(Arc::new(
        questkeep_store_heroic::sideload::HeroicSideloadProvider::detect(),
    ));
    stores
}

#[cfg(test)]
mod tests {
    use questkeep_store_api::ProviderError;
    use questkeep_store_api::auth::MemoryTokenStore;

    use super::*;

    fn stores() -> Vec<Arc<dyn LibraryProvider>> {
        builtin(Arc::new(MemoryTokenStore::new()), None, None)
    }

    fn store(id: &str) -> Arc<dyn LibraryProvider> {
        stores().into_iter().find(|p| p.id() == id).unwrap()
    }

    #[test]
    fn builtin_stores_ship_a_tintable_svg_icon() {
        for provider in stores() {
            let svg = provider.icon_svg().trim();
            let id = provider.id();
            assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"), "{id}");
            assert!(
                svg.contains("viewBox=") && svg.contains("currentColor"),
                "{id}"
            );
            assert!(!svg.contains("<script"), "{id}");
        }
    }

    #[test]
    fn only_the_steam_key_is_a_secret_setting() {
        // The keyring entry `setting:steam.apiKey` holds keys saved by earlier versions.
        let secrets: Vec<(String, String)> = stores()
            .iter()
            .flat_map(|p| {
                let id = p.id().to_string();
                p.settings()
                    .secret_keys()
                    .map(|k| (id.clone(), k.to_string()))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(secrets, [("steam".to_string(), "apiKey".to_string())]);
    }

    #[test]
    fn builtin_epic_and_gog_support_account_sign_in() {
        for id in ["epic", "gog"] {
            let provider = store(id);
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
        // Steam signs in through the store website: no code to paste.
        let steam = store("steam").account().unwrap().status();
        assert!(!steam.signed_in && !steam.supports_code_sign_in);
        let epic = store("epic");
        assert!(matches!(
            epic.metadata("Sugar"),
            Err(ProviderError::NotConfigured(_))
        ));
        assert!(epic.cover_candidates("Sugar").is_empty());
    }
}
