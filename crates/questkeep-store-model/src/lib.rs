//! The data Questkeep stores exchange with the app. This crate has no OS or network dependency, so
//! WASM store plugins (see `questkeep-plugin-sdk`) use the very same types as the built-in stores.

pub mod model;
pub mod settings;
