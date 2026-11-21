//! Test fixture, not an example to copy: a store that misbehaves on purpose. The app's plugin
//! tests build it for `wasm32-wasip2` and check that the sandbox contains it.
//!
//! - `scan_installed` panics (a trap) while the option `trap` is on, and otherwise reports one
//!   game per call, numbered by a counter that restarts with every new instance.
//! - `metadata("loop")` never returns.
//! - `metadata("probe")` reports what the plugin could reach, as the description.
//! - `metadata("broad")` reports what the broad permissions give (file system below `/host`,
//!   registry, known folders, SQLite, secrets, website session); the files to try come from the
//!   options `file` and `db` (paths as the plugin sees them).
//! - The option `paths` makes `scan_installed` report one game per path with it as `install_dir`
//!   (to see how the app translates them), and `launcher` gives the store a launcher process.
//! - The store has an account: signing in stores the pasted text as the secret `token`.

use std::cell::{Cell, RefCell};

use questkeep_plugin_sdk::model::{
    AccountInfo, GameMetadata, ImportedGame, LauncherClose, LauncherProcess,
};
use questkeep_plugin_sdk::{
    Provider, ProviderError, export_provider, http, known_folder, registry, secrets, session,
    sqlite,
};

#[derive(Default)]
struct Probe {
    trap: Cell<bool>,
    calls: Cell<u32>,
    config: RefCell<serde_json::Value>,
}

impl Probe {
    fn option(&self, name: &str) -> serde_json::Value {
        self.config.borrow()[name].clone()
    }

    fn broad_report(&self) -> String {
        let file = self.option("file");
        let db = self.option("db");
        let lines = [
            format!(
                "host-read: {}",
                outcome(std::fs::read(file.as_str().unwrap_or("/host/none")))
            ),
            format!(
                "host-write: {}",
                outcome(std::fs::write(
                    format!("{}.new", file.as_str().unwrap_or("/host/none")),
                    "x"
                ))
            ),
            format!("known-home: {}", known_folder("home").unwrap_or_default()),
            format!(
                "registry-read: {}",
                outcome(registry::read(r"HKCU\Software\x\y"))
            ),
            format!(
                "registry-enum: {}",
                outcome(registry::subkeys(r"HKCU\Software"))
            ),
            format!("uninstall: {}", outcome(registry::uninstall_entries())),
            format!(
                "sqlite: {}",
                match sqlite::query(
                    db.as_str().unwrap_or("/host/none"),
                    "SELECT name FROM t ORDER BY id",
                    &[]
                ) {
                    Ok(rows) => format!("ok {:?}", rows.rows),
                    Err(err) => format!("error: {err}"),
                }
            ),
            format!(
                "sqlite-write: {}",
                outcome(sqlite::query(
                    db.as_str().unwrap_or("/host/none"),
                    "DELETE FROM t",
                    &[]
                ))
            ),
            format!("secret-set: {}", outcome(secrets::set("note", "kept"))),
            format!("secret-get: {:?}", secrets::get("note").ok().flatten()),
            format!(
                "session: {}",
                match session::refresh() {
                    Ok(found) => format!("ok {found:?}"),
                    Err(err) => format!("error: {err}"),
                }
            ),
        ];
        lines.join(
            "
",
        )
    }
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
        *self.config.borrow_mut() = config.clone();
    }

    fn launcher_process(&self) -> Option<LauncherProcess> {
        (self.option("launcher") == true).then(|| LauncherProcess {
            process_names: vec!["steam.exe".into(), "evil.exe".into()],
            close: LauncherClose::ForceQuit,
        })
    }

    fn has_account(&self) -> bool {
        true
    }

    fn account_status(&self) -> AccountInfo {
        let token = secrets::get("token").ok().flatten();
        AccountInfo {
            signed_in: token.is_some(),
            display_name: token,
            ..AccountInfo::default()
        }
    }

    fn sign_in_with_code(&self, input: &str) -> Result<(), ProviderError> {
        secrets::set("token", input)
    }

    fn sign_out(&self) -> Result<(), ProviderError> {
        secrets::delete("token")
    }

    #[allow(clippy::panic)]
    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if self.trap.get() {
            panic!("the probe was told to trap");
        }
        if let Some(paths) = self.option("paths").as_array() {
            return Ok(paths
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let mut game = ImportedGame::installed(i.to_string(), "Path");
                    game.install_dir = path.as_str().map(str::to_string);
                    game
                })
                .collect());
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
            "broad" => Ok(Some(GameMetadata {
                description: Some(self.broad_report()),
                ..GameMetadata::default()
            })),
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
