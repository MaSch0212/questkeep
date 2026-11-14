//! Loads real store plugins: the example store (`examples/plugins/example-store`) and a
//! misbehaving one (`questkeep-plugin-sdk/examples/sandbox_probe.rs`). Both are built here for
//! `wasm32-wasip2`; without that target (`rustup target add wasm32-wasip2`) the tests are
//! skipped with a message.

// The skip message has to reach whoever runs the tests; the helpers here are test code too.
#![allow(clippy::print_stderr, clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use questkeep_core::events::{CoreEvent, EventBus};
use questkeep_core::paths::DataPaths;
use questkeep_core::providers::ProviderRegistry;
use questkeep_plugins::wasm::GENERIC_ICON;
use questkeep_plugins::{CallLimits, PluginManager};
use questkeep_store_api::model::{ActionKind, ImportedGame};
use questkeep_store_api::{ImageCandidate, LibraryProvider, ProviderError};
use serde_json::{Value, json};

const TARGET: &str = "wasm32-wasip2";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn target_installed() -> bool {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let Ok(output) = Command::new(rustc).args(["--print", "sysroot"]).output() else {
        return false;
    };
    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Path::new(&sysroot)
        .join("lib/rustlib")
        .join(TARGET)
        .is_dir()
}

/// Runs `cargo build --release --target wasm32-wasip2` with the given extra arguments and
/// returns the build's target folder. `None` (once, with a message) without the WASM target.
fn build(args: &[&str]) -> Option<PathBuf> {
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    if !*INSTALLED.get_or_init(target_installed) {
        eprintln!(
            "SKIPPED: the Rust target {TARGET} is not installed (rustup target add {TARGET}); \
             the store plugin tests did not run."
        );
        return None;
    }
    // Its own target folder: the outer cargo holds the lock of the one this test was built in.
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("plugin-wasm");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--release", "--target", TARGET, "--target-dir"])
        .arg(&target_dir)
        .args(args)
        .current_dir(repo_root())
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .expect("cargo should start");
    assert!(
        status.success(),
        "building the plugin failed: cargo {args:?}"
    );
    Some(target_dir.join(TARGET).join("release"))
}

fn example_wasm() -> Option<PathBuf> {
    static BUILT: OnceLock<Option<PathBuf>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let manifest = "examples/plugins/example-store/Cargo.toml";
            build(&["--manifest-path", manifest]).map(|dir| dir.join("example_store.wasm"))
        })
        .clone()
}

fn probe_wasm() -> Option<PathBuf> {
    static BUILT: OnceLock<Option<PathBuf>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            build(&["-p", "questkeep-plugin-sdk", "--example", "sandbox_probe"])
                .map(|dir| dir.join("examples/sandbox_probe.wasm"))
        })
        .clone()
}

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    paths: DataPaths,
    registry: ProviderRegistry,
    events: EventBus,
    changes: Arc<AtomicUsize>,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let events = EventBus::new();
        let changes = Arc::new(AtomicUsize::new(0));
        let counter = changes.clone();
        events.subscribe(move |event| {
            if matches!(event, CoreEvent::PluginsChanged) {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        Self {
            paths: DataPaths::new(root.join("data")),
            registry: ProviderRegistry::new(),
            root,
            events,
            changes,
            _tmp: tmp,
        }
    }

    fn manager(&self, limits: CallLimits) -> PluginManager {
        PluginManager::with_call_limits(
            self.paths.clone(),
            self.registry.clone(),
            self.events.clone(),
            limits,
        )
    }

    fn provider(&self, id: &str) -> Arc<dyn LibraryProvider> {
        self.registry
            .get(id)
            .expect("the store should be registered")
    }
}

fn names(games: &[ImportedGame]) -> Vec<&str> {
    games.iter().map(|g| g.name.as_str()).collect()
}

/// The example plugin as a developer has it after `build.ps1`.
fn example_source(dir: &Path, wasm: &Path) {
    let example = repo_root().join("examples/plugins/example-store");
    std::fs::create_dir_all(dir).unwrap();
    for file in ["plugin.json", "games.json"] {
        std::fs::copy(example.join(file), dir.join(file)).unwrap();
    }
    std::fs::copy(wasm, dir.join("provider.wasm")).unwrap();
}

#[test]
fn example_store_loads_scans_reloads_and_unloads() {
    let Some(wasm) = example_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let source = fx.root.join("dev/example-store");
    example_source(&source, &wasm);

    let info = manager.install(&source).unwrap();
    assert_eq!(info.error, None);
    assert!(info.loaded && info.enabled);
    assert_eq!(info.providers, ["example-store"]);
    assert_eq!(info.name, "Example Store");
    assert_eq!(fx.changes.load(Ordering::SeqCst), 1);

    // What the app knows without calling the plugin.
    let store = fx.provider("example-store");
    assert_eq!(store.name(), "Example Store");
    assert!(store.icon_svg().starts_with("<svg") && store.icon_svg() != GENERIC_ICON);
    assert_eq!(store.shop_url(), Some("https://example.com/store"));
    assert!(store.can_scan() && store.can_scan_owned() && store.is_installed());
    assert!(!store.can_uninstall() && !store.has_launcher_pages());
    assert!(store.launcher().is_none() && store.account().is_none());
    let settings = store.settings();
    assert_eq!(settings.options.len(), 1);
    assert_eq!(settings.options[0].key, "includeDemos");
    assert!(settings.credentials.is_none());
    let listed = fx.registry.infos(|_| Value::Null);
    assert_eq!(listed.len(), 1);
    assert!(listed[0].syncable && listed[0].installed);
    assert_eq!(listed[0].settings, settings);

    // Scans read games.json through the /plugin mount.
    let games = store.scan_installed().unwrap();
    assert_eq!(names(&games), ["Starfall Odyssey", "Pixel Kart"]);
    assert_eq!(games[0].source_game_id, "starfall");
    assert_eq!(games[0].playtime_seconds, Some(5400));
    assert_eq!(
        games[0].install_dir.as_deref(),
        Some("C:/Games/Starfall Odyssey")
    );
    let action = games[0].play_action.as_ref().unwrap();
    assert_eq!(action.kind, ActionKind::File);
    assert_eq!(action.target, "C:/Games/Starfall Odyssey/starfall.exe");
    let owned = store.scan_owned(&Value::Null).unwrap();
    assert_eq!(
        names(&owned),
        ["Starfall Odyssey", "Pixel Kart", "Moonlit Garden"]
    );
    assert!(!owned[2].installed && owned[2].play_action.is_none());

    // A store option changes the result.
    store.configure(&json!({ "includeDemos": true }));
    assert_eq!(
        names(&store.scan_installed().unwrap()),
        ["Starfall Odyssey", "Pixel Kart", "Starfall Odyssey Demo"]
    );
    store.configure(&Value::Null);
    assert_eq!(store.scan_installed().unwrap().len(), 2);

    // Metadata, covers and the defaults of everything the plugin does not implement.
    let metadata = store.metadata("starfall").unwrap().unwrap();
    assert_eq!(
        metadata.description.as_deref(),
        Some("Chart a course through a collapsing galaxy.")
    );
    assert_eq!(metadata.genres, ["Adventure", "Strategy"]);
    assert_eq!(metadata.release_date.as_deref(), Some("2024-03-14"));
    assert_eq!(metadata.links[0].url, "https://example.com/store/starfall");
    assert_eq!(store.metadata("unknown").unwrap(), None);
    assert_eq!(
        store.cover_candidates("starfall"),
        [ImageCandidate::Url(
            "https://example.com/covers/starfall.jpg".into()
        )]
    );
    assert!(store.cover_candidates("pixel-kart").is_empty());
    assert!(store.background_candidates("starfall").is_empty());
    assert!(store.install_action("starfall").is_none());
    assert!(store.launch_with_args("starfall", "-x").is_none());
    assert!(!store.covers_are_low_quality("starfall"));
    assert_eq!(store.player_count("starfall").unwrap(), None);
    assert_eq!(
        store.scan_categories(&Value::Null).unwrap(),
        None::<HashMap<_, _>>
    );
    assert!(matches!(
        store.run_add_game_option("x"),
        Err(ProviderError::Unsupported)
    ));

    // Reload picks up a changed games.json from the source folder.
    let games_file = source.join("games.json");
    let mut list: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&games_file).unwrap()).unwrap();
    list.push(json!({ "id": "new-one", "name": "Brand New", "exe": "C:/Games/New/new.exe" }));
    std::fs::write(&games_file, serde_json::to_string(&list).unwrap()).unwrap();
    assert_eq!(store.scan_installed().unwrap().len(), 2);
    let info = manager.reload("example-store").unwrap();
    assert!(info.loaded && info.error.is_none());
    let reloaded = fx.provider("example-store");
    assert_eq!(
        names(&reloaded.scan_installed().unwrap()),
        ["Starfall Odyssey", "Pixel Kart", "Brand New"]
    );
    // The store from before the reload is closed, not left running on replaced files.
    let err = store.scan_installed().unwrap_err().to_string();
    assert!(err.contains("not loaded any more"), "{err}");
    drop(store);

    // Disabling unloads the store, enabling brings it back.
    let info = manager.set_enabled("example-store", false).unwrap();
    assert!(!info.loaded && info.providers.is_empty());
    assert!(fx.registry.get("example-store").is_none());
    assert!(reloaded.scan_installed().is_err());
    let info = manager.set_enabled("example-store", true).unwrap();
    assert_eq!(info.providers, ["example-store"]);
    assert_eq!(
        fx.provider("example-store").scan_installed().unwrap().len(),
        3
    );

    // The next start loads it from the installed copy.
    let next = Fixture::new();
    let restarted =
        PluginManager::new(fx.paths.clone(), next.registry.clone(), next.events.clone());
    restarted.load_enabled();
    assert_eq!(restarted.list(), manager.list());
    assert_eq!(
        next.registry.get("example-store").unwrap().name(),
        "Example Store"
    );
    assert_eq!(next.changes.load(Ordering::SeqCst), 1);
    // (Two apps on one data folder don't happen: let go of the files before they are deleted.)
    next.registry.remove("example-store");
    drop(restarted);

    // Removing unloads and deletes the copy; the source is untouched.
    manager.remove("example-store", true).unwrap();
    assert!(fx.registry.get("example-store").is_none());
    assert!(manager.list().is_empty());
    assert!(!manager.plugin_dir("example-store").exists());
    assert!(source.join("provider.wasm").is_file());
}

/// A plugin folder around the probe component.
fn probe_source(dir: &Path, wasm: &Path, plugin_id: &str, store_dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let manifest = json!({
        "id": plugin_id,
        "name": "Sandbox Probe",
        "version": "0.0.1",
        "apiVersion": 1,
        "providers": [{ "wasm": "bin/provider.wasm" }],
        "permissions": {
            "read": [
                { "path": store_dir.display().to_string(), "as": "/store" },
                { "path": "%QUESTKEEP_TEST_NO_SUCH_VARIABLE%/x", "as": "/unset" },
                { "path": store_dir.join("missing").display().to_string(), "as": "/missing" },
            ],
            "http": ["declared.example"],
        },
    });
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::copy(wasm, dir.join("bin/provider.wasm")).unwrap();
}

fn short_limits() -> CallLimits {
    CallLimits {
        slow: Duration::from_millis(1500),
        quick: Duration::from_secs(10),
    }
}

#[test]
fn a_misbehaving_store_stays_in_its_sandbox() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(short_limits());
    let store_dir = fx.root.join("some-store");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::write(store_dir.join("hello.txt"), "hello").unwrap();
    let source = fx.root.join("dev/probe");
    probe_source(&source, &wasm, "sandbox-probe", &store_dir);

    let info = manager.install(&source).unwrap();
    assert_eq!(info.error, None);
    assert_eq!(info.providers, ["sandbox-probe"]);
    assert_eq!(info.permissions.http, ["declared.example"]);
    assert_eq!(info.permissions.read.len(), 3);
    let store = fx.provider("sandbox-probe");
    // Its icon carries a script hook: the generic one is shown instead.
    assert_eq!(store.icon_svg(), GENERIC_ICON);

    // What the plugin can and cannot reach.
    let report = store
        .metadata("probe")
        .unwrap()
        .unwrap()
        .description
        .unwrap();
    let outcome: HashMap<&str, &str> = report
        .lines()
        .filter_map(|line| line.split_once(": "))
        .collect();
    let ok = |key: &str| assert_eq!(outcome[key], "ok", "{key} in\n{report}");
    let denied = |key: &str| assert!(outcome[key].starts_with("error"), "{key} in\n{report}");
    ok("plugin-read");
    denied("plugin-write");
    ok("data-write");
    ok("data-read");
    ok("mount-read");
    denied("mount-write");
    denied("escape");
    denied("socket");
    assert_eq!(outcome["env"], "0", "{report}");
    assert!(
        outcome["http-undeclared"].contains("no permission to request undeclared.example"),
        "{report}"
    );
    let data = fx.paths.plugin_data_dir().join("sandbox-probe");
    assert_eq!(std::fs::read_to_string(data.join("note.txt")).unwrap(), "x");
    assert!(!manager.plugin_dir("sandbox-probe").join("new.txt").exists());
    assert!(!store_dir.join("new.txt").exists());

    // The instance lives across calls...
    let id = |games: Vec<ImportedGame>| games[0].source_game_id.clone();
    assert_eq!(id(store.scan_installed().unwrap()), "1");
    assert_eq!(id(store.scan_installed().unwrap()), "2");

    // ...until a call runs away: it is stopped, and the next call gets a fresh instance.
    let started = Instant::now();
    let err = store.metadata("loop").unwrap_err().to_string();
    assert!(err.contains("did not answer within"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(id(store.scan_installed().unwrap()), "1");

    // A crash (panic) is an error, not the end of the app. The fresh instance gets the
    // settings again, so it keeps crashing until they change.
    store.configure(&json!({ "trap": true }));
    for _ in 0..2 {
        let err = store.scan_installed().unwrap_err().to_string();
        assert!(
            err.contains("Sandbox Probe plugin stopped with an error"),
            "{err}"
        );
    }
    store.configure(&json!({ "trap": false }));
    assert_eq!(id(store.scan_installed().unwrap()), "1");
    assert!(manager.list()[0].loaded);

    manager.remove("sandbox-probe", false).unwrap();
    assert!(data.join("note.txt").is_file());
}

struct Taken;

impl LibraryProvider for Taken {
    fn id(&self) -> &str {
        "sandbox-probe"
    }
    fn name(&self) -> &str {
        "Taken"
    }
    fn icon_svg(&self) -> &str {
        "<svg/>"
    }
    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        Ok(Vec::new())
    }
}

#[test]
fn store_ids_must_belong_to_the_plugin_and_be_free() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(short_limits());
    let store_dir = fx.root.join("some-store");

    // The store calls itself "sandbox-probe", the plugin is someone else.
    let source = fx.root.join("dev/other");
    probe_source(&source, &wasm, "other-plugin", &store_dir);
    let info = manager.install(&source).unwrap();
    assert!(!info.loaded && info.providers.is_empty());
    let error = info.error.unwrap();
    assert!(error.contains("does not belong to the plugin"), "{error}");
    assert_eq!(fx.registry.iter().count(), 0);

    // The id is taken by a store that is not this plugin's.
    fx.registry.insert(Arc::new(Taken));
    let source = fx.root.join("dev/probe");
    probe_source(&source, &wasm, "sandbox-probe", &store_dir);
    let info = manager.install(&source).unwrap();
    let error = info.error.unwrap();
    assert!(error.contains("already used by another store"), "{error}");
    assert_eq!(fx.provider("sandbox-probe").name(), "Taken");

    // Once it is free, Reload gets the plugin going.
    fx.registry.remove("sandbox-probe");
    let info = manager.reload("sandbox-probe").unwrap();
    assert!(info.loaded && info.error.is_none());
    assert_eq!(fx.provider("sandbox-probe").name(), "Sandbox Probe");
    // Removing the plugin must not take someone else's store with it.
    manager.remove("other-plugin", false).unwrap();
    assert!(fx.registry.get("sandbox-probe").is_some());
}
