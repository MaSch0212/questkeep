//! The plugin interface of Questkeep stores. A store crate implements [`LibraryProvider`] and
//! describes itself through it: its games, how to start and install them, its account sign-in and
//! its settings. The app registers the stores it ships; the core only ever sees this interface.

pub mod auth;
pub mod model;
pub mod platform;
mod provider;
pub mod settings;
pub mod sqlite;

pub use provider::*;

/// An HTTP agent for store requests: a timeout and Questkeep's user agent.
pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .user_agent(concat!("Questkeep/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}
