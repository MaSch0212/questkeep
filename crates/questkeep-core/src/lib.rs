//! Questkeep core: library model, persistence, providers, launching and images.
//!
//! This crate has no dependency on Tauri or any UI so it can be unit-tested headless.

pub mod auth;
pub mod db;
pub mod error;
pub mod events;
pub mod images;
pub mod launcher;
pub mod library;
pub mod model;
pub mod paths;
pub mod platform;
pub mod providers;
pub mod seed;
pub mod settings;
pub mod tray;

pub use error::{CoreError, Result};
pub use library::{AppInfo, Library};

/// Crate version, exposed to the UI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
