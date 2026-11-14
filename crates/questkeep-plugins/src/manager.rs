//! The installed plugins: loading, unloading and reloading them while the app runs.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use questkeep_core::events::{CoreEvent, EventBus};
use questkeep_core::paths::DataPaths;
use questkeep_core::providers::ProviderRegistry;
use questkeep_store_api::LibraryProvider;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{PluginError, Result};
use crate::install::{self, StateEntry};
use crate::manifest::{PluginManifest, expand_path};
use crate::wasm::{CallLimits, Runtime, Sandbox, WasmProvider};

/// How long the source folder must be quiet before a watched plugin is reloaded.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(500);
/// What an extension may save as its settings.
const MAX_SETTINGS_BYTES: usize = 1024 * 1024;

/// An installed plugin, as the UI lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    /// The folder or zip the plugin was installed from.
    pub source: String,
    /// The source is still there: Reload copies it again.
    pub source_exists: bool,
    pub enabled: bool,
    /// Reload whenever the source folder changes.
    pub watch: bool,
    /// Enabled and running.
    pub loaded: bool,
    /// Why the plugin could not be loaded or reloaded.
    pub error: Option<String>,
    /// Ids of the plugin's loaded stores.
    pub providers: Vec<String>,
    pub has_extension: bool,
    /// The `extension` part of the manifest (its script, styles and declared settings) while the
    /// plugin is enabled and its manifest is readable; the UI loads the extension from it.
    #[ts(type = "Record<string, unknown> | null")]
    pub extension: Option<serde_json::Value>,
    /// Counts the times the plugin was loaded; a new number means its files may have changed.
    #[ts(type = "number")]
    pub revision: u64,
    /// How many themes the plugin brings.
    pub themes: u32,
    /// The `themes` part of the manifest (id, name, css file, parameters, preview) while the
    /// plugin is enabled; the UI registers the themes from it.
    #[ts(type = "Array<Record<string, unknown>>")]
    pub theme_defs: Vec<serde_json::Value>,
    pub permissions: PluginPermissions,
}

/// What a plugin's stores may reach, for display.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PluginPermissions {
    /// Folders the stores may read, as written in the manifest.
    pub read: Vec<String>,
    /// Hosts the stores may send requests to.
    pub http: Vec<String>,
}

struct Plugin {
    entry: StateEntry,
    /// The installed copy's manifest; `None` when it can't be read.
    manifest: Option<PluginManifest>,
    providers: Vec<Arc<WasmProvider>>,
    loaded: bool,
    error: Option<String>,
    revision: u64,
    /// Dropping it stops the watch.
    watcher: Option<RecommendedWatcher>,
}

impl Plugin {
    fn new(entry: StateEntry) -> Self {
        Self {
            entry,
            manifest: None,
            providers: Vec::new(),
            loaded: false,
            error: None,
            revision: 0,
            watcher: None,
        }
    }

    fn info(&self) -> PluginInfo {
        let manifest = self.manifest.as_ref();
        PluginInfo {
            id: self.entry.id.clone(),
            name: manifest.map_or_else(|| self.entry.id.clone(), |m| m.name.clone()),
            version: manifest.map(|m| m.version.clone()).unwrap_or_default(),
            description: manifest.and_then(|m| m.description.clone()),
            author: manifest.and_then(|m| m.author.clone()),
            source: self.entry.source.clone(),
            source_exists: Path::new(&self.entry.source).exists(),
            enabled: self.entry.enabled,
            watch: self.entry.watch,
            loaded: self.loaded,
            error: self.error.clone(),
            providers: self.providers.iter().map(|p| p.id().to_string()).collect(),
            has_extension: manifest.is_some_and(|m| m.extension.is_some()),
            extension: manifest
                .filter(|_| self.entry.enabled)
                .and_then(|m| m.extension.clone()),
            revision: self.revision,
            themes: manifest.map_or(0, |m| m.themes.len() as u32),
            theme_defs: manifest
                .filter(|_| self.entry.enabled)
                .map(|m| m.themes.clone())
                .unwrap_or_default(),
            permissions: manifest
                .map(|m| PluginPermissions {
                    read: m.permissions.read.iter().map(|r| r.path.clone()).collect(),
                    http: m.permissions.http.clone(),
                })
                .unwrap_or_default(),
        }
    }
}

struct Inner {
    paths: DataPaths,
    registry: ProviderRegistry,
    events: EventBus,
    limits: CallLimits,
    /// Started with the first store plugin.
    runtime: OnceLock<std::result::Result<Arc<Runtime>, String>>,
    /// In install order.
    plugins: Mutex<Vec<Plugin>>,
    /// One change at a time (a watched reload may come in at any moment).
    changing: Mutex<()>,
}

/// The installed plugins. Clones share one list.
#[derive(Clone)]
pub struct PluginManager {
    inner: Arc<Inner>,
}

impl PluginManager {
    /// `registry` must be the one the library uses: plugin stores are added to and removed from
    /// it.
    pub fn new(paths: DataPaths, registry: ProviderRegistry, events: EventBus) -> Self {
        Self::with_call_limits(paths, registry, events, CallLimits::default())
    }

    /// [`new`](Self::new) with other deadlines for plugin calls (tests).
    pub fn with_call_limits(
        paths: DataPaths,
        registry: ProviderRegistry,
        events: EventBus,
        limits: CallLimits,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                paths,
                registry,
                events,
                limits,
                runtime: OnceLock::new(),
                plugins: Mutex::new(Vec::new()),
                changing: Mutex::new(()),
            }),
        }
    }

    /// Reads the plugin list and loads the enabled plugins (once, at startup). A plugin that
    /// fails is listed with its error; nothing here is fatal.
    pub fn load_enabled(&self) {
        let _change = self.inner.change();
        let dir = self.inner.paths.plugins_dir();
        install::clean_leftovers(&dir);
        let entries = install::load_state(&dir);
        if entries.is_empty() {
            return;
        }
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        *self.inner.plugins() = entries.into_iter().map(Plugin::new).collect();
        for id in &ids {
            self.inner.load(id);
            self.sync_watch(id);
        }
        tracing::info!(plugins = ids.len(), "plugins loaded");
        self.inner.changed();
    }

    /// Every installed plugin, in install order.
    pub fn list(&self) -> Vec<PluginInfo> {
        self.inner.plugins().iter().map(Plugin::info).collect()
    }

    /// Installs the plugin in `path` (a folder or a zip), enables and loads it. An installed
    /// plugin with the same id is replaced (an update).
    pub fn install(&self, path: &Path) -> Result<PluginInfo> {
        let _change = self.inner.change();
        let source = std::path::absolute(path).map_err(|e| PluginError::io(path, e))?;
        let staged = install::stage(&source, &self.inner.paths.plugins_dir(), None)?;
        let id = staged.manifest().id.clone();
        // The running copy lets go of its files before they are replaced.
        self.inner.unload(&id);
        if let Err(err) = staged.commit() {
            self.inner.load(&id);
            self.inner.changed();
            return Err(err);
        }
        {
            let mut plugins = self.inner.plugins();
            let source = source.display().to_string();
            match plugins.iter_mut().find(|p| p.entry.id == id) {
                Some(plugin) => {
                    plugin.entry.source = source;
                    plugin.entry.enabled = true;
                }
                None => plugins.push(Plugin::new(StateEntry {
                    id: id.clone(),
                    source,
                    enabled: true,
                    watch: false,
                })),
            }
        }
        let saved = self.inner.save();
        self.inner.load(&id);
        self.sync_watch(&id);
        self.inner.changed();
        tracing::info!(plugin = %id, source = %source.display(), "plugin installed");
        saved?;
        self.info(&id)
    }

    /// Loads the plugin anew: from its source when that still exists (copied again), otherwise
    /// from the installed copy. When the source is broken the running copy stays and the plugin
    /// shows the error.
    pub fn reload(&self, id: &str) -> Result<PluginInfo> {
        let _change = self.inner.change();
        let source = PathBuf::from(self.entry(id)?.source);
        if source.exists() {
            let staged = match install::stage(&source, &self.inner.paths.plugins_dir(), Some(id)) {
                Ok(staged) => staged,
                Err(err) => {
                    self.inner.set_error(id, format!("Reload failed: {err}"));
                    self.inner.changed();
                    return Err(err);
                }
            };
            self.inner.unload(id);
            if let Err(err) = staged.commit() {
                self.inner.load(id);
                self.inner.set_error(id, format!("Reload failed: {err}"));
                self.inner.changed();
                return Err(err);
            }
        } else {
            self.inner.unload(id);
        }
        self.inner.load(id);
        self.inner.changed();
        tracing::info!(plugin = id, "plugin reloaded");
        self.info(id)
    }

    /// Loads (enabled) or unloads (disabled) the plugin; its files stay either way.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<PluginInfo> {
        let _change = self.inner.change();
        self.update_entry(id, |entry| entry.enabled = enabled)?;
        let saved = self.inner.save();
        if enabled {
            self.inner.load(id);
        } else {
            self.inner.unload(id);
        }
        self.inner.changed();
        saved?;
        self.info(id)
    }

    /// Turns reloading on source changes on or off.
    pub fn set_watch(&self, id: &str, watch: bool) -> Result<PluginInfo> {
        let _change = self.inner.change();
        self.update_entry(id, |entry| entry.watch = watch)?;
        let saved = self.inner.save();
        self.sync_watch(id);
        self.inner.changed();
        saved?;
        self.info(id)
    }

    /// Unloads the plugin and deletes its installed copy; with `delete_data` also what the
    /// plugin stored itself. Games of its stores stay in the library.
    pub fn remove(&self, id: &str, delete_data: bool) -> Result<()> {
        let _change = self.inner.change();
        self.entry(id)?;
        self.inner.unload(id);
        let dir = self.inner.paths.plugins_dir();
        let removed = install::uninstall(&dir, id);
        if removed.is_ok() {
            self.inner.plugins().retain(|p| p.entry.id != id);
        } else {
            // Still installed, so still listed: switched off, with the reason.
            let _ = self.update_entry(id, |entry| entry.enabled = false);
        }
        let saved = self.inner.save();
        self.inner.changed();
        removed?;
        saved?;
        if delete_data {
            install::remove_dir(&self.inner.paths.plugin_data_dir().join(id))?;
        }
        tracing::info!(plugin = id, delete_data, "plugin removed");
        Ok(())
    }

    /// The installed copy of a plugin.
    pub fn plugin_dir(&self, id: &str) -> PathBuf {
        self.inner.paths.plugins_dir().join(id)
    }

    /// The manifest of an installed plugin (as loaded last).
    pub fn manifest(&self, id: &str) -> Option<PluginManifest> {
        self.inner
            .plugins()
            .iter()
            .find(|p| p.entry.id == id)
            .and_then(|p| p.manifest.clone())
    }

    /// The file `relative` of an installed, enabled plugin, for handing it to the UI (extension
    /// scripts, styles, images). `None` for anything that is not a file inside the plugin.
    pub fn file(&self, id: &str, relative: &str) -> Option<PathBuf> {
        let enabled = self
            .inner
            .plugins()
            .iter()
            .any(|p| p.entry.id == id && p.entry.enabled);
        if !enabled {
            return None;
        }
        let root = self.plugin_dir(id).canonicalize().ok()?;
        let mut path = root.clone();
        for part in relative.split('/').filter(|p| !p.is_empty()) {
            if part == "." || part == ".." || part.contains(['\\', ':']) {
                return None;
            }
            path.push(part);
        }
        // Links must not lead out of the plugin either.
        let path = path.canonicalize().ok()?;
        (path.starts_with(&root) && path.is_file()).then_some(path)
    }

    /// What the plugin's extension saved (`{}` when nothing yet). It lives with the plugin's own
    /// data, so removing that data removes it too.
    pub fn settings(&self, id: &str) -> Result<serde_json::Value> {
        self.entry(id)?;
        let file = self.settings_file(id);
        let empty = || serde_json::Value::Object(serde_json::Map::new());
        match std::fs::read_to_string(&file) {
            Ok(text) => Ok(serde_json::from_str(&text)
                .ok()
                .filter(serde_json::Value::is_object)
                .unwrap_or_else(empty)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(empty()),
            Err(err) => Err(PluginError::io(&file, err)),
        }
    }

    /// Replaces what the plugin's extension saved; `settings` must be a JSON object.
    pub fn save_settings(&self, id: &str, settings: &serde_json::Value) -> Result<()> {
        self.entry(id)?;
        if !settings.is_object() {
            return Err(PluginError::Invalid(
                "Plugin settings must be an object.".into(),
            ));
        }
        let text = serde_json::to_string_pretty(settings)
            .map_err(|e| PluginError::Invalid(e.to_string()))?;
        if text.len() > MAX_SETTINGS_BYTES {
            return Err(PluginError::Invalid(format!(
                "The settings of the plugin \"{id}\" are too large."
            )));
        }
        let file = self.settings_file(id);
        let dir = self.inner.paths.plugin_data_dir().join(id);
        std::fs::create_dir_all(&dir).map_err(|e| PluginError::io(&dir, e))?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| PluginError::io(&tmp, e))?;
        std::fs::rename(&tmp, &file).map_err(|e| PluginError::io(&file, e))
    }

    fn settings_file(&self, id: &str) -> PathBuf {
        self.inner
            .paths
            .plugin_data_dir()
            .join(id)
            .join("settings.json")
    }

    fn info(&self, id: &str) -> Result<PluginInfo> {
        self.inner
            .plugins()
            .iter()
            .find(|p| p.entry.id == id)
            .map(Plugin::info)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))
    }

    fn entry(&self, id: &str) -> Result<StateEntry> {
        self.inner
            .plugins()
            .iter()
            .find(|p| p.entry.id == id)
            .map(|p| p.entry.clone())
            .ok_or_else(|| PluginError::NotFound(id.to_string()))
    }

    fn update_entry(&self, id: &str, update: impl FnOnce(&mut StateEntry)) -> Result<()> {
        let mut plugins = self.inner.plugins();
        let plugin = plugins
            .iter_mut()
            .find(|p| p.entry.id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        update(&mut plugin.entry);
        Ok(())
    }

    /// Starts or stops watching the plugin's source folder, as its entry says.
    fn sync_watch(&self, id: &str) {
        let Ok(entry) = self.entry(id) else {
            return;
        };
        let source = PathBuf::from(&entry.source);
        let watcher = if entry.watch && source.is_dir() {
            watch(&source, id, Arc::downgrade(&self.inner))
        } else {
            None
        };
        if let Some(plugin) = self.inner.plugins().iter_mut().find(|p| p.entry.id == id) {
            plugin.watcher = watcher;
        }
    }
}

impl Inner {
    fn plugins(&self) -> MutexGuard<'_, Vec<Plugin>> {
        self.plugins.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn change(&self) -> MutexGuard<'_, ()> {
        self.changing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn changed(&self) {
        self.events.publish(CoreEvent::PluginsChanged);
    }

    fn save(&self) -> Result<()> {
        let entries: Vec<StateEntry> = self.plugins().iter().map(|p| p.entry.clone()).collect();
        install::save_state(&self.paths.plugins_dir(), &entries).inspect_err(|err| {
            tracing::error!(%err, "could not save the plugin list");
        })
    }

    fn set_error(&self, id: &str, error: String) {
        if let Some(plugin) = self.plugins().iter_mut().find(|p| p.entry.id == id) {
            plugin.error = Some(error);
        }
    }

    fn runtime(&self) -> Result<Arc<Runtime>> {
        self.runtime
            .get_or_init(|| {
                Runtime::new(self.limits)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
            .map_err(PluginError::Load)
    }

    /// Takes the plugin's stores out of the app.
    fn unload(&self, id: &str) {
        let providers = {
            let mut plugins = self.plugins();
            let Some(plugin) = plugins.iter_mut().find(|p| p.entry.id == id) else {
                return;
            };
            plugin.loaded = false;
            std::mem::take(&mut plugin.providers)
        };
        for provider in providers {
            self.registry.remove(provider.id());
            // Calls still under way finish; later ones fail instead of using deleted files.
            provider.close();
        }
    }

    /// Reads the installed copy's manifest and, for an enabled plugin, starts its stores. The
    /// outcome (also a failure) is kept with the plugin.
    fn load(&self, id: &str) {
        let enabled = {
            let plugins = self.plugins();
            match plugins.iter().find(|p| p.entry.id == id) {
                Some(plugin) => plugin.entry.enabled,
                None => return,
            }
        };
        let dir = self.paths.plugins_dir().join(id);
        let manifest = PluginManifest::load(&dir).and_then(|manifest| {
            if manifest.id == id {
                Ok(manifest)
            } else {
                Err(PluginError::Invalid(format!(
                    "The installed plugin \"{id}\" says it is \"{}\".",
                    manifest.id
                )))
            }
        });
        let (manifest, outcome) = match manifest {
            Ok(manifest) if enabled => {
                let outcome = self.load_providers(&manifest, &dir);
                (Some(manifest), outcome)
            }
            Ok(manifest) => (Some(manifest), Ok(Vec::new())),
            Err(err) => (None, Err(err)),
        };
        if let Err(err) = &outcome {
            tracing::warn!(plugin = id, %err, "plugin could not be loaded");
        }
        let mut plugins = self.plugins();
        let Some(plugin) = plugins.iter_mut().find(|p| p.entry.id == id) else {
            return;
        };
        plugin.manifest = manifest;
        plugin.revision += 1;
        match outcome {
            Ok(providers) => {
                for provider in &providers {
                    self.registry.insert(provider.clone());
                }
                plugin.providers = providers;
                plugin.loaded = enabled;
                plugin.error = None;
            }
            Err(err) => {
                plugin.providers = Vec::new();
                plugin.loaded = false;
                plugin.error = Some(err.to_string());
            }
        }
    }

    /// Starts every store of the plugin, or none.
    fn load_providers(
        &self,
        manifest: &PluginManifest,
        dir: &Path,
    ) -> Result<Vec<Arc<WasmProvider>>> {
        if manifest.providers.is_empty() {
            return Ok(Vec::new());
        }
        let runtime = self.runtime()?;
        let home = home_dir();
        let sandbox = Sandbox {
            plugin_id: manifest.id.clone(),
            plugin_dir: dir.to_path_buf(),
            data_dir: self.paths.plugin_data_dir().join(&manifest.id),
            mounts: manifest
                .permissions
                .read
                .iter()
                .filter_map(|read| {
                    let path = expand_path(&read.path, |v| std::env::var(v).ok(), home.as_deref())?;
                    Some((path, read.mount.clone()))
                })
                .collect(),
            hosts: manifest.host_rules(),
        };
        let mut providers: Vec<Arc<WasmProvider>> = Vec::new();
        for entry in &manifest.providers {
            let provider =
                WasmProvider::load(runtime.clone(), &dir.join(&entry.wasm), sandbox.clone())?;
            let id = provider.id();
            if providers.iter().any(|p| p.id() == id) {
                return Err(PluginError::Load(format!(
                    "Two stores of the plugin use the id \"{id}\"."
                )));
            }
            // The plugin's own stores were unloaded before: whoever has the id is someone else.
            if self.registry.get(id).is_some() {
                return Err(PluginError::Load(format!(
                    "The store id \"{id}\" is already used by another store."
                )));
            }
            tracing::info!(plugin = %manifest.id, provider = id, "plugin store loaded");
            providers.push(Arc::new(provider));
        }
        Ok(providers)
    }
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Whether a change at `path` in the source folder `root` matters: build output, dependencies
/// and hidden folders don't.
fn is_relevant(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    let mut parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    // The last part is the changed file or folder itself; folders are judged by name too.
    let last = parts.pop();
    !parts.iter().any(|p| install::is_ignored_dir(p))
        && !last.is_some_and(|name| install::is_ignored_dir(&name) && path.is_dir())
}

/// Watches a plugin's source folder and reloads the plugin once it has been quiet for
/// [`WATCH_DEBOUNCE`]. The watch ends when the returned watcher is dropped.
fn watch(source: &Path, id: &str, manager: Weak<Inner>) -> Option<RecommendedWatcher> {
    let (changes, quiet) = mpsc::channel::<()>();
    let root = source.to_path_buf();
    let created =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(event) => {
                let relevant = !matches!(event.kind, notify::EventKind::Access(_))
                    && event.paths.iter().any(|p| is_relevant(&root, p));
                if relevant {
                    let _ = changes.send(());
                }
            }
            Err(err) => tracing::debug!(%err, "plugin source watch error"),
        });
    let mut watcher = match created {
        Ok(watcher) => watcher,
        Err(err) => {
            tracing::warn!(plugin = id, %err, "could not watch the plugin's source");
            return None;
        }
    };
    if let Err(err) = watcher.watch(source, RecursiveMode::Recursive) {
        tracing::warn!(plugin = id, %err, "could not watch the plugin's source");
        return None;
    }
    let id = id.to_string();
    let spawned = std::thread::Builder::new()
        .name("plugin-watch".into())
        .spawn(move || {
            // Ends when the watcher (the sender) is gone.
            while quiet.recv().is_ok() {
                loop {
                    match quiet.recv_timeout(WATCH_DEBOUNCE) {
                        Ok(()) => {}
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                let Some(inner) = manager.upgrade() else {
                    return;
                };
                tracing::info!(plugin = %id, "plugin source changed: reloading");
                if let Err(err) = (PluginManager { inner }).reload(&id) {
                    tracing::warn!(plugin = %id, %err, "reload after a source change failed");
                }
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the plugin watch thread");
        return None;
    }
    Some(watcher)
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<String> = self
            .inner
            .plugins()
            .iter()
            .map(|p| p.entry.id.clone())
            .collect();
        f.debug_struct("PluginManager")
            .field("plugins", &ids)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::install::testing::write_plugin;

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

        fn manager(&self) -> PluginManager {
            PluginManager::new(
                self.paths.clone(),
                self.registry.clone(),
                self.events.clone(),
            )
        }

        fn changes(&self) -> usize {
            self.changes.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn serves_plugin_files_and_keeps_extension_settings() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let source = fx.root.join("src/ext");
        write_plugin(&source, "ext", "1.0.0");
        std::fs::create_dir_all(source.join("ui")).unwrap();
        std::fs::write(source.join("ui/main.js"), "export function activate() {}").unwrap();
        std::fs::write(fx.root.join("secret.txt"), "no").unwrap();
        let first = manager.install(&source).unwrap();

        let file = manager.file("ext", "ui/main.js").unwrap();
        assert!(file.ends_with("main.js") && file.is_file());
        assert_eq!(manager.file("ext", "/ui//main.js"), Some(file));
        for outside in [
            "../../secret.txt",
            "ui/../../../secret.txt",
            "ui",
            "missing.js",
            "C:/Windows/win.ini",
            r"ui\main.js",
            "",
        ] {
            assert_eq!(manager.file("ext", outside), None, "{outside}");
        }
        assert_eq!(manager.file("nope", "ui/main.js"), None);

        // Each load gets a new number, so the UI knows to fetch the files again.
        let reloaded = manager.reload("ext").unwrap();
        assert!(reloaded.revision > first.revision);

        assert_eq!(manager.settings("ext").unwrap(), serde_json::json!({}));
        let saved = serde_json::json!({ "greeting": "hi", "count": 3 });
        manager.save_settings("ext", &saved).unwrap();
        assert_eq!(manager.settings("ext").unwrap(), saved);
        assert!(
            manager
                .save_settings("ext", &serde_json::json!([1]))
                .is_err()
        );
        assert!(manager.settings("nope").is_err());

        // A disabled plugin hands out no files.
        manager.set_enabled("ext", false).unwrap();
        assert_eq!(manager.file("ext", "ui/main.js"), None);
        // Its settings go with its data.
        manager.remove("ext", true).unwrap();
        assert!(!fx.paths.plugin_data_dir().join("ext").exists());
    }

    #[test]
    fn installs_lists_disables_and_removes() {
        let fx = Fixture::new();
        let manager = fx.manager();
        assert!(manager.list().is_empty());
        manager.load_enabled();
        assert_eq!(fx.changes(), 0);

        let source = fx.root.join("src/theme-pack");
        write_plugin(&source, "theme-pack", "1.0.0");
        let info = manager.install(&source).unwrap();
        assert_eq!(info.id, "theme-pack");
        assert_eq!(info.name, "Plugin theme-pack");
        assert_eq!(info.version, "1.0.0");
        assert!(info.enabled && info.loaded && info.source_exists && !info.watch);
        assert_eq!(info.error, None);
        assert_eq!(info.themes, 1);
        assert_eq!(info.theme_defs.len(), 1);
        assert!(!info.has_extension && info.providers.is_empty());
        assert_eq!(Path::new(&info.source), source);
        assert_eq!(fx.changes(), 1);
        assert!(
            manager
                .plugin_dir("theme-pack")
                .join("plugin.json")
                .is_file()
        );
        assert_eq!(manager.manifest("theme-pack").unwrap().themes.len(), 1);
        assert!(manager.manifest("nope").is_none());

        let info = manager.set_enabled("theme-pack", false).unwrap();
        assert!(!info.enabled && !info.loaded);
        // Disabled plugins keep their files and are still described.
        assert_eq!(info.name, "Plugin theme-pack");
        assert_eq!(fx.changes(), 2);

        // A second manager (the next start) reads the same list.
        let again = fx.manager();
        again.load_enabled();
        // (How often each one loaded the plugin differs.)
        let unnumbered = |list: Vec<PluginInfo>| -> Vec<PluginInfo> {
            list.into_iter()
                .map(|p| PluginInfo { revision: 0, ..p })
                .collect()
        };
        assert_eq!(unnumbered(again.list()), unnumbered(manager.list()));

        manager.set_enabled("theme-pack", true).unwrap();
        std::fs::create_dir_all(fx.paths.plugin_data_dir().join("theme-pack")).unwrap();
        manager.remove("theme-pack", false).unwrap();
        assert!(manager.list().is_empty());
        assert!(!manager.plugin_dir("theme-pack").exists());
        assert!(fx.paths.plugin_data_dir().join("theme-pack").is_dir());
        assert!(install::load_state(&fx.paths.plugins_dir()).is_empty());

        manager.install(&source).unwrap();
        manager.remove("theme-pack", true).unwrap();
        assert!(!fx.paths.plugin_data_dir().join("theme-pack").exists());
    }

    #[test]
    fn update_and_reload_copy_the_source_again() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let source = fx.root.join("src");
        write_plugin(&source, "pack", "1.0.0");
        manager.install(&source).unwrap();

        write_plugin(&source, "pack", "1.1.0");
        assert_eq!(manager.list()[0].version, "1.0.0");
        assert_eq!(manager.reload("pack").unwrap().version, "1.1.0");

        // Installing the same id again is an update, not a second plugin.
        write_plugin(&source, "pack", "2.0.0");
        assert_eq!(manager.install(&source).unwrap().version, "2.0.0");
        assert_eq!(manager.list().len(), 1);

        // A broken source: the installed copy stays, the plugin shows why.
        std::fs::write(source.join("plugin.json"), "{ nope").unwrap();
        assert!(manager.reload("pack").is_err());
        let info = &manager.list()[0];
        assert_eq!(info.version, "2.0.0");
        assert!(info.loaded);
        assert!(info.error.as_deref().unwrap().starts_with("Reload failed:"));

        // Without the source, Reload loads the installed copy (and clears the error).
        std::fs::remove_dir_all(&source).unwrap();
        let info = manager.reload("pack").unwrap();
        assert!(!info.source_exists && info.loaded);
        assert_eq!(info.error, None);
        assert_eq!(info.version, "2.0.0");
    }

    #[test]
    fn unknown_plugins_and_broken_installs_are_reported() {
        let fx = Fixture::new();
        let manager = fx.manager();
        for result in [
            manager.reload("ghost"),
            manager.set_enabled("ghost", true),
            manager.set_watch("ghost", true),
        ] {
            assert_eq!(
                result.unwrap_err().to_string(),
                "There is no plugin \"ghost\"."
            );
        }
        assert!(manager.remove("ghost", true).is_err());
        assert!(manager.install(&fx.root.join("missing")).is_err());
        assert_eq!(fx.changes(), 0);

        // An installed copy that went bad is listed with its error instead of stopping the app.
        let source = fx.root.join("src");
        write_plugin(&source, "pack", "1.0.0");
        manager.install(&source).unwrap();
        std::fs::write(manager.plugin_dir("pack").join("plugin.json"), "broken").unwrap();
        let next = fx.manager();
        next.load_enabled();
        let info = &next.list()[0];
        assert_eq!((info.id.as_str(), info.name.as_str()), ("pack", "pack"));
        assert!(info.enabled && !info.loaded);
        assert!(
            info.error
                .as_deref()
                .unwrap()
                .contains("plugin.json is not valid")
        );

        // A store file that is no WebAssembly component.
        let manifest = serde_json::json!({
            "id": "pack", "name": "Pack", "version": "3", "apiVersion": 1,
            "providers": [{ "wasm": "provider.wasm" }],
        });
        std::fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
        std::fs::write(source.join("provider.wasm"), "not wasm").unwrap();
        let info = next.install(&source).unwrap();
        assert!(info.enabled && !info.loaded && info.providers.is_empty());
        let error = info.error.unwrap();
        assert!(
            error.contains("provider.wasm is not a store plugin"),
            "{error}"
        );
        assert_eq!(fx.registry.iter().count(), 0);
    }

    #[test]
    fn watching_reloads_after_a_source_change() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let source = fx.root.join("src");
        write_plugin(&source, "pack", "1.0.0");
        manager.install(&source).unwrap();
        assert!(manager.set_watch("pack", true).unwrap().watch);
        assert!(install::load_state(&fx.paths.plugins_dir())[0].watch);

        // Ignored folders don't count...
        std::fs::create_dir_all(source.join("target/debug")).unwrap();
        std::fs::write(source.join("target/debug/x.o"), "x").unwrap();
        std::thread::sleep(WATCH_DEBOUNCE * 3);
        let before = fx.changes();
        // ...a real change does.
        write_plugin(&source, "pack", "1.0.1");
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while manager.list()[0].version != "1.0.1" {
            assert!(
                std::time::Instant::now() < deadline,
                "the plugin was not reloaded"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(fx.changes() > before);

        // Off again: changes are not picked up any more.
        manager.set_watch("pack", false).unwrap();
        write_plugin(&source, "pack", "1.0.2");
        std::thread::sleep(WATCH_DEBOUNCE * 3);
        assert_eq!(manager.list()[0].version, "1.0.1");
    }

    #[test]
    fn relevant_changes() {
        let root = Path::new("/src/plugin");
        assert!(is_relevant(root, &root.join("games.json")));
        assert!(is_relevant(root, &root.join("src/lib.rs")));
        assert!(is_relevant(root, &root.join("provider.wasm")));
        assert!(!is_relevant(
            root,
            &root.join("target/wasm32-wasip2/release/x.wasm")
        ));
        assert!(!is_relevant(root, &root.join(".git/index")));
        assert!(!is_relevant(root, &root.join("node_modules/a/b.js")));
        // A file that merely looks hidden is part of the plugin.
        assert!(is_relevant(root, &root.join(".env")));
    }
}
