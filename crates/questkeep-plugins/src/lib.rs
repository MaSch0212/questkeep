//! Questkeep plugins.
//!
//! A plugin is a folder with a `plugin.json` ([`manifest`]). Installing copies it into the data
//! folder ([`install`]); the [`PluginManager`] loads, unloads and reloads the installed plugins
//! while the app runs. A plugin's stores are WebAssembly components that run sandboxed
//! ([`wasm`]) and show up in the core's provider registry like the built-in stores; they are
//! written with `questkeep-plugin-sdk`. Extensions and themes are only carried along here: the
//! UI loads them.

pub mod error;
pub mod install;
pub mod manager;
pub mod manifest;
pub mod wasm;

pub use error::PluginError;
pub use manager::{PluginInfo, PluginManager, PluginPermissions};
pub use manifest::PluginManifest;
pub use wasm::CallLimits;
