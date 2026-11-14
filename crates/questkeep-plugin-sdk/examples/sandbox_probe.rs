//! Test fixture, not an example to copy: a store that misbehaves on purpose. The app's plugin
//! tests build it for `wasm32-wasip2` and check that the sandbox contains it.
//!
//! - `scan_installed` panics (a trap) while the option `trap` is on, and otherwise reports one
//!   game per call, numbered by a counter that restarts with every new instance.
//! - `metadata("loop")` never returns.
//! - `metadata("probe")` reports what the plugin could reach, as the description.

use std::cell::Cell;

use questkeep_plugin_sdk::model::{GameMetadata, ImportedGame};
use questkeep_plugin_sdk::{Provider, ProviderError, export_provider, http};

#[derive(Default)]
struct Probe {
    trap: Cell<bool>,
    calls: Cell<u32>,
}

fn outcome<T, E: std::fmt::Display>(result: Result<T, E>) -> String {
    match result {
        Ok(_) => "ok".into(),
        Err(err) => format!("error: {err}"),
    }
}

impl Provider for Probe {
    fn id(&self) -> String {
        "sandbox-probe".into()
    }

    fn name(&self) -> String {
        "Sandbox Probe".into()
    }

    fn icon_svg(&self) -> String {
        r#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#.into()
    }

    fn is_installed(&self) -> bool {
        true
    }

    fn configure(&self, config: &serde_json::Value) {
        self.trap.set(config["trap"] == true);
    }

    #[allow(clippy::panic)]
    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if self.trap.get() {
            panic!("the probe was told to trap");
        }
        self.calls.set(self.calls.get() + 1);
        Ok(vec![ImportedGame::installed(
            self.calls.get().to_string(),
            "Call",
        )])
    }

    #[allow(clippy::empty_loop)]
    fn metadata(&self, source_game_id: &str) -> Result<Option<GameMetadata>, ProviderError> {
        match source_game_id {
            "loop" => loop {},
            "probe" => {
                let lines = [
                    format!(
                        "plugin-read: {}",
                        outcome(std::fs::read("/plugin/plugin.json"))
                    ),
                    format!(
                        "plugin-write: {}",
                        outcome(std::fs::write("/plugin/new.txt", "x"))
                    ),
                    format!(
                        "data-write: {}",
                        outcome(std::fs::write("/data/note.txt", "x"))
                    ),
                    format!("data-read: {}", outcome(std::fs::read("/data/note.txt"))),
                    format!("mount-read: {}", outcome(std::fs::read("/store/hello.txt"))),
                    format!(
                        "mount-write: {}",
                        outcome(std::fs::write("/store/new.txt", "x"))
                    ),
                    format!(
                        "escape: {}",
                        outcome(std::fs::read("/plugin/../plugins.json"))
                    ),
                    format!("env: {}", std::env::vars().count()),
                    format!(
                        "socket: {}",
                        outcome(std::net::TcpStream::connect("127.0.0.1:9"))
                    ),
                    format!(
                        "http-undeclared: {}",
                        outcome(http::get("https://undeclared.example/"))
                    ),
                ];
                Ok(Some(GameMetadata {
                    description: Some(lines.join("\n")),
                    ..GameMetadata::default()
                }))
            }
            _ => Ok(None),
        }
    }
}

export_provider!(Probe);
