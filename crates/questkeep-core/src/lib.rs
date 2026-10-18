//! Questkeep core: library model, persistence, providers, launching and images.
//!
//! This crate has no dependency on Tauri or any UI so it can be unit-tested headless.

pub mod error;
pub mod paths;

pub use error::{CoreError, Result};

/// Crate version, exposed to the UI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
