//! SDK for Questkeep store plugins.
//!
//! A store plugin is a WebAssembly component (`wasm32-wasip2`) that reports games. Implement
//! [`Provider`] for a `Default` type and hand it to [`export_provider!`]:
//!
//! ```
//! use questkeep_plugin_sdk::model::ImportedGame;
//! use questkeep_plugin_sdk::{Provider, ProviderError, export_provider};
//!
//! #[derive(Default)]
//! struct MyStore;
//!
//! impl Provider for MyStore {
//!     fn id(&self) -> String {
//!         "my-store".into()
//!     }
//!     fn name(&self) -> String {
//!         "My Store".into()
//!     }
//!     fn is_installed(&self) -> bool {
//!         true
//!     }
//!     fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
//!         Ok(vec![ImportedGame::installed("1", "First Game")])
//!     }
//! }
//!
//! export_provider!(MyStore);
//! ```
//!
//! What a plugin can reach:
//!
//! - **Files** through `std::fs`: `/plugin` (the installed plugin folder, read-only), `/data`
//!   (the plugin's own folder, read-write) and every folder declared under `permissions.read`
//!   in `plugin.json` (read-only, at its `as` path).
//! - **The web** through [`http`], only on the hosts declared under `permissions.http`.
//! - **The app's log** through [`log`].
//!
//! Nothing else: no environment, no sockets, no other files.
//!
//! # Wire format
//!
//! The component exports two functions (see `wit/plugin.wit`): `describe()` and
//! `call(method, args)`, both speaking JSON. [`wire`] documents every method, its arguments and
//! its answer; the app and this SDK share that module, so authors never deal with it.

pub use questkeep_store_model::{model, settings};

pub mod http;
pub mod log;
mod provider;
pub mod wire;

pub use provider::{Provider, ProviderError, describe, descriptor, dispatch};
pub use wire::{ImageCandidate, ProviderDescriptor};

/// The component glue generated from `wit/plugin.wit`. Plugins don't use it directly.
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
// wit-bindgen's generated guest code passes raw pointers across the component boundary; this is
// the only module of the SDK that may contain unsafe code.
#[allow(unsafe_code, clippy::all)]
pub mod bindings {
    wit_bindgen::generate!({
        world: "provider",
        path: "wit",
        pub_export_macro: true,
        export_macro_name: "__export_provider_bindings",
        default_bindings_module: "questkeep_plugin_sdk::bindings",
    });

    impl From<crate::ProviderError> for ProviderError {
        fn from(err: crate::ProviderError) -> Self {
            match err {
                crate::ProviderError::NotFound(m) => ProviderError::NotFound(m),
                crate::ProviderError::Io(m) => ProviderError::Io(m),
                crate::ProviderError::Parse(m) => ProviderError::Parse(m),
                crate::ProviderError::Unsupported => ProviderError::Unsupported,
                crate::ProviderError::NotConfigured(m) => ProviderError::NotConfigured(m),
                crate::ProviderError::Network(m) => ProviderError::Network(m),
            }
        }
    }
}

/// Makes a panic show up in Questkeep's log before it stops the plugin call.
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub fn __init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            log::error(format!("the plugin panicked: {info}"));
        }));
    });
}

/// Exports a [`Provider`] as the plugin's store. The type must implement `Default`; one value
/// is created when the plugin is loaded and lives until it is unloaded.
///
/// On other targets than `wasm32` this only checks the type, so a plugin crate also builds (and
/// can be unit-tested) natively.
#[macro_export]
macro_rules! export_provider {
    ($provider:ty) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            ::std::thread_local! {
                static PROVIDER: $provider = <$provider as ::core::default::Default>::default();
            }

            struct Export;

            impl $crate::bindings::Guest for Export {
                fn describe() -> ::std::string::String {
                    $crate::__init();
                    PROVIDER.with(|p| $crate::describe(p))
                }

                fn call(
                    method: ::std::string::String,
                    args: ::std::string::String,
                ) -> ::core::result::Result<::std::string::String, $crate::bindings::ProviderError>
                {
                    $crate::__init();
                    PROVIDER
                        .with(|p| $crate::dispatch(p, &method, &args))
                        .map_err(::core::convert::Into::into)
                }
            }

    $crate::bindings::__export_provider_bindings!(Export with_types_in $crate::bindings);
        };

        #[cfg(not(target_arch = "wasm32"))]
        const _: fn() = || {
            fn is_provider<T: $crate::Provider + ::core::default::Default>() {}
            is_provider::<$provider>();
        };
    };
}
