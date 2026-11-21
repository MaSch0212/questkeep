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
    for file in ["plugin.json", "games.json", "icon.svg"] {
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
    // (How often each one loaded the plugin differs.)
    let unnumbered = |list: Vec<questkeep_plugins::PluginInfo>| -> Vec<_> {
        list.into_iter()
            .map(|p| questkeep_plugins::PluginInfo { revision: 0, ..p })
            .collect()
    };
    assert_eq!(unnumbered(restarted.list()), unnumbered(manager.list()));
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

/// The example plugin with a `games` folder the user chooses; its `games.json` points the
/// games at `/mygames/...`, the way a store reports paths it saw in a mounted folder.
fn folder_source(dir: &Path, wasm: &Path) {
    example_source(dir, wasm);
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("plugin.json")).unwrap()).unwrap();
    manifest["permissions"]["folders"] = json!([
        { "key": "games", "label": "Games", "as": "/mygames" }
    ]);
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
    let games = json!([{
        "id": "bs", "name": "Beat Saber",
        "exe": "/mygames/My Instance/Beat Saber.exe",
        "installDir": "/mygames/My Instance/",
    }]);
    std::fs::write(dir.join("games.json"), games.to_string()).unwrap();
}

#[test]
fn chosen_folders_are_mounted_and_their_paths_translated() {
    let Some(wasm) = example_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let source = fx.root.join("dev/example-store");
    folder_source(&source, &wasm);

    // Before the user chooses, the plugin has no such folder: its paths mean nothing.
    let info = manager.install(&source).unwrap();
    assert_eq!(info.error, None);
    assert_eq!(info.folders[0].effective_path, None);
    let game = fx
        .provider("example-store")
        .scan_installed()
        .unwrap()
        .remove(0);
    assert_eq!(
        game.play_action.unwrap().target,
        "/mygames/My Instance/Beat Saber.exe"
    );

    // After choosing, the stores' paths below the mount are real paths on this PC.
    let first = fx.root.join("first");
    std::fs::create_dir_all(&first).unwrap();
    manager
        .set_folder("example-store", "games", Some(&first.display().to_string()))
        .unwrap();
    let first = PathBuf::from(manager.list()[0].folders[0].path.clone().unwrap());
    let game = fx
        .provider("example-store")
        .scan_installed()
        .unwrap()
        .remove(0);
    assert_eq!(
        PathBuf::from(game.play_action.unwrap().target),
        first.join("My Instance").join("Beat Saber.exe")
    );
    assert_eq!(
        PathBuf::from(game.install_dir.unwrap()),
        first.join("My Instance")
    );

    // Another folder, the plugin follows.
    let second = fx.root.join("second");
    std::fs::create_dir_all(&second).unwrap();
    manager
        .set_folder(
            "example-store",
            "games",
            Some(&second.display().to_string()),
        )
        .unwrap();
    let second = PathBuf::from(manager.list()[0].folders[0].path.clone().unwrap());
    let game = fx
        .provider("example-store")
        .scan_installed()
        .unwrap()
        .remove(0);
    assert_eq!(
        PathBuf::from(game.install_dir.unwrap()),
        second.join("My Instance")
    );

    // The choice is host-side state: not in the plugin's /data, which it can write.
    let state = std::fs::read_to_string(fx.paths.plugins_dir().join("plugins.json")).unwrap();
    assert!(state.contains("\"games\""), "{state}");
    let data = fx.paths.plugin_data_dir().join("example-store");
    if data.exists() {
        assert_eq!(std::fs::read_dir(&data).unwrap().count(), 0);
    }

    // An update keeps the choice; removing the plugin forgets it.
    let info = manager.install(&source).unwrap();
    assert_eq!(
        info.folders[0].path.as_deref().map(PathBuf::from),
        Some(second.clone())
    );
    manager.remove("example-store", true).unwrap();
    let info = manager.install(&source).unwrap();
    assert_eq!(info.folders[0].path, None);
}

#[test]
fn folders_chosen_while_installing_are_mounted_by_the_first_load() {
    let Some(wasm) = example_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let source = fx.root.join("dev/example-store");
    folder_source(&source, &wasm);
    let chosen = fx.root.join("chosen");
    std::fs::create_dir_all(&chosen).unwrap();

    let folders =
        std::collections::BTreeMap::from([("games".to_string(), chosen.display().to_string())]);
    let info = manager.install_with_folders(&source, &folders).unwrap();
    assert_eq!(info.error, None);
    assert!(info.folders[0].exists);
    let chosen = PathBuf::from(info.folders[0].path.clone().unwrap());

    // The very first scan already sees the real folder, no reload in between.
    let game = fx
        .provider("example-store")
        .scan_installed()
        .unwrap()
        .remove(0);
    assert_eq!(
        PathBuf::from(game.install_dir.unwrap()),
        chosen.join("My Instance")
    );
}

// ---- broad permissions ------------------------------------------------------------------

use questkeep_plugins::PluginServices;
use questkeep_store_api::auth::{
    LoginWindow, MemoryTokenStore, Navigation, TokenStore, WebSession,
};

/// A plugin folder around the probe with exactly these permissions (and extra files).
fn probe_plugin(dir: &Path, wasm: &Path, plugin_id: &str, permissions: Value, files: &[&str]) {
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let manifest = json!({
        "id": plugin_id,
        "name": "Sandbox Probe",
        "version": "0.0.1",
        "apiVersion": 1,
        "providers": [{ "wasm": "bin/provider.wasm" }],
        "permissions": permissions,
    });
    std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
    std::fs::copy(wasm, dir.join("bin/provider.wasm")).unwrap();
    for file in files {
        std::fs::write(dir.join(file), "var reportSignedOut = REPORT_SIGNED_OUT;").unwrap();
    }
}

/// The path of `path` as a plugin with `permissions.filesystem` sees it.
fn guest(path: &Path) -> String {
    questkeep_plugin_sdk::host_path(&path.to_string_lossy()).unwrap()
}

/// A folder with a file and a SQLite database, outside every mount a plugin declares.
fn outside_files(root: &Path) -> (PathBuf, PathBuf) {
    let dir = root.join("outside");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("hello.txt");
    std::fs::write(&file, "hello").unwrap();
    let db = dir.join("games.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE t(id INTEGER, name TEXT); INSERT INTO t VALUES (1, 'alpha');")
        .unwrap();
    (file, db)
}

/// The probe's `broad` report as `key: value` lines.
fn broad_report(
    store: &Arc<dyn LibraryProvider>,
    file: &Path,
    db: &Path,
) -> HashMap<String, String> {
    store.configure(&json!({ "file": guest(file), "db": guest(db) }));
    let report = store
        .metadata("broad")
        .unwrap()
        .unwrap()
        .description
        .unwrap();
    report
        .lines()
        .filter_map(|line| line.split_once(": "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn memory_services() -> (Arc<MemoryTokenStore>, PluginServices) {
    let secrets = Arc::new(MemoryTokenStore::new());
    let services = PluginServices {
        secrets: secrets.clone(),
        web: None,
    };
    (secrets, services)
}

#[test]
fn without_the_broad_permissions_a_store_reaches_none_of_it() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let (secrets, services) = memory_services();
    manager.set_services(services);
    let (file, db) = outside_files(&fx.root);
    let source = fx.root.join("dev/probe");
    probe_plugin(&source, &wasm, "sandbox-probe", json!({}), &[]);
    assert_eq!(manager.install(&source).unwrap().error, None);

    let report = broad_report(&fx.provider("sandbox-probe"), &file, &db);
    let denied = |key: &str, text: &str| {
        assert!(
            report[key].starts_with("error") && report[key].contains(text),
            "{key}: {}",
            report[key]
        );
    };
    denied("host-read", "");
    denied("host-write", "");
    assert_eq!(report["known-home"], "");
    denied("registry-read", "no permission to read the registry");
    denied("registry-enum", "no permission to read the registry");
    denied("uninstall", "no permission to read the registry");
    denied("sqlite", "no permission to read the file system");
    denied("session", "no permission to use a website session");
    // Secrets are the plugin's own and need no permission; they live in the keyring under its id.
    assert_eq!(report["secret-set"], "ok");
    assert_eq!(report["secret-get"], "Some(\"kept\")");
    assert_eq!(
        secrets
            .load("plugin/sandbox-probe/note")
            .unwrap()
            .as_deref(),
        Some("kept")
    );
    assert_eq!(secrets.load("note").unwrap(), None);
}

#[test]
fn the_file_system_and_registry_permissions_open_exactly_that() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let (_secrets, services) = memory_services();
    manager.set_services(services);
    let (file, db) = outside_files(&fx.root);
    let source = fx.root.join("dev/probe");
    probe_plugin(
        &source,
        &wasm,
        "sandbox-probe",
        json!({ "filesystem": "read", "registry": "read" }),
        &[],
    );
    let info = manager.install(&source).unwrap();
    assert_eq!(info.error, None);
    assert!(info.permissions.filesystem && info.permissions.registry);

    let report = broad_report(&fx.provider("sandbox-probe"), &file, &db);
    assert_eq!(report["host-read"], "ok", "{report:?}");
    // Read-only, however much can be read.
    assert!(report["host-write"].starts_with("error"), "{report:?}");
    assert!(!PathBuf::from(format!("{}.new", file.display())).exists());
    assert!(report["known-home"].starts_with("/host"), "{report:?}");
    // The registry answers (empty off Windows) instead of refusing.
    assert_eq!(report["registry-read"], "ok");
    assert_eq!(report["registry-enum"], "ok");
    assert_eq!(report["uninstall"], "ok");
    assert_eq!(report["sqlite"], "ok [[String(\"alpha\")]]", "{report:?}");
    assert!(report["sqlite-write"].starts_with("error"), "{report:?}");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    // No login permission, no website session.
    assert!(report["session"].contains("no permission"), "{report:?}");
}

#[test]
fn plugins_cannot_read_each_others_secrets() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let (secrets, services) = memory_services();
    manager.set_services(services);
    let (file, db) = outside_files(&fx.root);
    // Two plugins in turn: both call their store "sandbox-probe".
    for plugin in ["sandbox", "sandbox-probe"] {
        let source = fx.root.join(format!("dev/{plugin}"));
        probe_plugin(&source, &wasm, plugin, json!({}), &[]);
        manager.install(&source).unwrap();
        let report = broad_report(&fx.provider("sandbox-probe"), &file, &db);
        assert_eq!(report["secret-set"], "ok");
        manager.remove(plugin, false).unwrap();
    }
    for plugin in ["sandbox", "sandbox-probe"] {
        let entry = format!("plugin/{plugin}/note");
        assert_eq!(
            secrets.load(&entry).unwrap().as_deref(),
            Some("kept"),
            "{entry}"
        );
    }
}

#[test]
fn paths_into_installed_plugins_and_their_data_are_never_translated() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let (outside, _db) = outside_files(&fx.root);
    let game = outside.parent().unwrap().join("game");
    std::fs::create_dir_all(&game).unwrap();
    let source = fx.root.join("dev/probe");
    probe_plugin(
        &source,
        &wasm,
        "sandbox-probe",
        json!({ "filesystem": "read" }),
        &[],
    );
    manager.install(&source).unwrap();
    std::fs::create_dir_all(fx.paths.plugin_data_dir().join("sandbox-probe")).unwrap();

    let in_data = fx.paths.plugin_data_dir().join("sandbox-probe/evil.exe");
    let in_plugins = fx
        .paths
        .plugins_dir()
        .join("sandbox-probe/bin/provider.wasm");
    let paths = vec![
        guest(&game),
        guest(&in_data),
        guest(&in_plugins),
        "/data/evil.exe".to_string(),
        "/plugin/bin/provider.wasm".to_string(),
    ];
    let store = fx.provider("sandbox-probe");
    store.configure(&json!({ "paths": paths }));
    let dirs: Vec<String> = store
        .scan_installed()
        .unwrap()
        .into_iter()
        .map(|g| g.install_dir.unwrap())
        .collect();
    // Below the file system mount a path becomes the real one...
    assert_eq!(Path::new(&dirs[0]), game);
    // ...except where the plugins and their data live: those stay as the plugin wrote them.
    assert_eq!(dirs[1..], paths[1..], "{dirs:?}");
}

#[test]
fn the_launcher_is_only_closed_by_declared_names() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let source = fx.root.join("dev/probe");

    probe_plugin(&source, &wasm, "sandbox-probe", json!({}), &[]);
    manager.install(&source).unwrap();
    let store = fx.provider("sandbox-probe");
    store.configure(&json!({ "launcher": true }));
    assert!(
        store.launcher_process().is_none(),
        "no declaration, no launcher process"
    );
    manager.remove("sandbox-probe", false).unwrap();

    probe_plugin(
        &source,
        &wasm,
        "sandbox-probe",
        json!({ "launcher": { "processNames": ["Steam.exe"] } }),
        &[],
    );
    let info = manager.install(&source).unwrap();
    assert_eq!(info.permissions.launcher, ["Steam.exe"]);
    let store = fx.provider("sandbox-probe");
    store.configure(&json!({ "launcher": true }));
    let process = store.launcher_process().unwrap();
    // The plugin also named evil.exe: not declared, not closed.
    assert_eq!(process.process_names, ["steam.exe"]);
}

/// A website session service that records what the plugin asked for.
#[derive(Default)]
struct FakeWeb {
    refreshed: std::sync::Mutex<Vec<String>>,
    cleared: std::sync::Mutex<Vec<String>>,
}

impl WebSession for FakeWeb {
    fn refresh(&self, provider: &str, window: &LoginWindow) -> Result<Option<String>, String> {
        assert!(window.web_session.is_some());
        self.refreshed.lock().unwrap().push(provider.to_string());
        Ok(Some("https://questkeep.invalid/s?x=1".into()))
    }

    fn clear(&self, provider: &str) -> Result<(), String> {
        self.cleared.lock().unwrap().push(provider.to_string());
        Ok(())
    }
}

#[test]
fn a_store_with_a_login_permission_has_an_account_and_a_website_session() {
    let Some(wasm) = probe_wasm() else {
        return;
    };
    let fx = Fixture::new();
    let manager = fx.manager(CallLimits::default());
    let (secrets, mut services) = memory_services();
    let web = Arc::new(FakeWeb::default());
    services.web = Some(web.clone());
    manager.set_services(services);
    let (file, db) = outside_files(&fx.root);
    let source = fx.root.join("dev/probe");

    // Without the permission there is no account, whatever the plugin says.
    probe_plugin(&source, &wasm, "sandbox-probe", json!({}), &[]);
    manager.install(&source).unwrap();
    assert!(fx.provider("sandbox-probe").account().is_none());
    manager.remove("sandbox-probe", false).unwrap();

    probe_plugin(
        &source,
        &wasm,
        "sandbox-probe",
        json!({ "login": {
            "title": "Sign in to Example",
            "url": "https://auth.example.com/login",
            "domains": ["example.com"],
            "capture": { "param": "code", "urlPrefix": "https://auth.example.com/done" },
            "session": { "url": "https://www.example.com/account", "script": "session.js" }
        } }),
        &["session.js"],
    );
    let info = manager.install(&source).unwrap();
    assert_eq!(info.error, None);
    assert_eq!(info.permissions.login, ["example.com"]);
    let store = fx.provider("sandbox-probe");
    let account = store.account().expect("an account");

    let status = account.status();
    assert!(!status.signed_in);
    assert_eq!(
        (status.provider.as_str(), status.name.as_str()),
        ("sandbox-probe", "Sandbox Probe")
    );
    account.sign_in_with_code("tok-1").unwrap();
    let status = account.status();
    assert!(status.signed_in);
    assert_eq!(status.display_name.as_deref(), Some("tok-1"));
    // The token went to the keyring under the plugin's id, not to a file.
    assert_eq!(
        secrets
            .load("plugin/sandbox-probe/token")
            .unwrap()
            .as_deref(),
        Some("tok-1")
    );

    // The window: the page the manifest names, locked to its domains.
    assert_eq!(account.login_url(), "https://auth.example.com/login");
    let window = account.login_window();
    assert_eq!(
        window.navigation("https://login.example.com/x"),
        Navigation::Allow
    );
    assert_eq!(window.navigation("https://evil.net/"), Navigation::Block);
    assert_eq!(
        window.navigation("https://auth.example.com/done?code=abc"),
        Navigation::Capture("abc".into())
    );

    // The store's own session page, loaded in the store's profile.
    store.configure(&json!({ "file": guest(&file), "db": guest(&db) }));
    let report = store
        .metadata("broad")
        .unwrap()
        .unwrap()
        .description
        .unwrap();
    assert!(
        report.contains("session: ok Some(\"https://questkeep.invalid/s?x=1\")"),
        "{report}"
    );
    assert_eq!(*web.refreshed.lock().unwrap(), ["sandbox-probe"]);

    account.sign_out().unwrap();
    assert!(!account.status().signed_in);
    assert_eq!(secrets.load("plugin/sandbox-probe/token").unwrap(), None);
    assert_eq!(*web.cleared.lock().unwrap(), ["sandbox-probe"]);
}
