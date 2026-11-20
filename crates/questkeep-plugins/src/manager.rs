//! The installed plugins: loading, unloading and reloading them while the app runs.

use std::collections::BTreeMap;
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
use crate::install::{self, RegistryState, StateEntry};
use crate::manifest::{FolderPermission, PluginManifest, expand_path, folder_path};
use crate::registry;
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
    /// The manifest's `icon` (a path inside the plugin folder) while the plugin is enabled; the
    /// UI turns it into an image URL with the plugin's file scheme.
    pub icon: Option<String>,
    /// The folder or zip the plugin was installed from.
    pub source: String,
    /// The source is still there: Reload copies it again.
    pub source_exists: bool,
    pub enabled: bool,
    /// Reload whenever the source folder changes.
    pub watch: bool,
    /// Set when the plugin was installed from the plugin registry (its `source` is then the cached
    /// zip): the installed registry version and whether it is pinned (no update offers).
    pub registry: Option<RegistryState>,
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
    /// The folders the plugin asks the user for (`permissions.folders`) and what each points to.
    pub folders: Vec<PluginFolder>,
}

/// One folder a plugin asks the user for, and what it points to now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PluginFolder {
    pub key: String,
    pub label: String,
    pub hint: Option<String>,
    /// Where the plugin's stores see it.
    pub mount: String,
    /// The default as written in the manifest (`%VAR%` and `~` not expanded).
    pub default: Option<String>,
    /// The default with `%VAR%` and `~` expanded; `None` without a default or while a variable
    /// it uses is not set.
    pub default_path: Option<String>,
    /// `default_path` is an existing folder.
    pub default_exists: bool,
    /// The folder the user chose; `None` while the default applies.
    pub path: Option<String>,
    /// What the stores get: the choice, else the expanded default.
    pub effective_path: Option<String>,
    /// `effective_path` is an existing folder (only then it is mounted).
    pub exists: bool,
}

/// A folder a plugin asks the user for, as the install dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderRequest {
    pub key: String,
    pub label: String,
    pub hint: Option<String>,
    pub default: Option<String>,
}

impl From<&FolderPermission> for FolderRequest {
    fn from(folder: &FolderPermission) -> Self {
        Self {
            key: folder.key.clone(),
            label: folder.label.clone(),
            hint: folder.hint.clone(),
            default: folder.default.clone(),
        }
    }
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
    /// Folders the user chooses for the plugin: it may read each one that is set (or has a
    /// default), nothing else of them.
    pub folders: Vec<FolderRequest>,
}

/// A plugin that is about to be installed, for the install dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PluginPreview {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    /// The plugin's icon as a `data:` URL (it is not installed yet, so it has no file URL).
    pub icon: Option<String>,
    /// How many stores the plugin brings.
    pub stores: u32,
    pub has_extension: bool,
    /// The names of the plugin's themes.
    pub themes: Vec<String>,
    pub permissions: PluginPermissions,
    /// The folders the plugin asks for, with their defaults and, when an installed copy is
    /// replaced, what the user chose for them (`path`): the install dialog lets the user pick.
    pub folders: Vec<PluginFolder>,
    /// The version that is installed now and would be replaced.
    pub installed_version: Option<String>,
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
            icon: manifest
                .filter(|_| self.entry.enabled)
                .and_then(|m| m.icon.clone()),
            source: self.entry.source.clone(),
            source_exists: Path::new(&self.entry.source).exists(),
            enabled: self.entry.enabled,
            watch: self.entry.watch,
            registry: self.entry.registry.clone(),
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
                    folders: m.permissions.folders.iter().map(Into::into).collect(),
                })
                .unwrap_or_default(),
            folders: manifest
                .map(|m| {
                    let home = home_dir();
                    m.permissions
                        .folders
                        .iter()
                        .map(|f| plugin_folder(f, self.entry.folders.get(&f.key), home.as_deref()))
                        .collect()
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
    /// One registry download or install at a time (taken before `changing`).
    registry_ops: Mutex<()>,
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
                registry_ops: Mutex::new(()),
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

    /// Whether any installed plugin came from the plugin registry (and so may get updates).
    pub fn has_registry_plugins(&self) -> bool {
        self.inner
            .plugins()
            .iter()
            .any(|p| p.entry.registry.is_some())
    }

    /// What the installed copy of `id` asks for (`None`: not installed). A copy whose manifest
    /// cannot be read counts as asking for nothing, so anything a new version asks is new.
    pub(crate) fn installed_shape(&self, id: &str) -> Option<registry::PluginShape> {
        let plugins = self.inner.plugins();
        let plugin = plugins.iter().find(|p| p.entry.id == id)?;
        Some(
            plugin
                .manifest
                .as_ref()
                .map(|m| registry::PluginShape {
                    permissions: PluginPermissions {
                        read: m.permissions.read.iter().map(|r| r.path.clone()).collect(),
                        http: m.permissions.http.clone(),
                        folders: m.permissions.folders.iter().map(Into::into).collect(),
                    },
                    stores: m.providers.len() as u32,
                    has_extension: m.extension.is_some(),
                })
                .unwrap_or_default(),
        )
    }

    /// Reads and checks the plugin in `path` (a folder or a zip) the way an install would,
    /// without installing it: what the install dialog shows before the user agrees.
    pub fn preview(&self, path: &Path) -> Result<PluginPreview> {
        let _change = self.inner.change();
        let source = std::path::absolute(path).map_err(|e| PluginError::io(path, e))?;
        // The checked copy is thrown away again when `staged` is dropped.
        let staged = install::stage(&source, &self.inner.paths.plugins_dir(), None)?;
        let manifest = staged.manifest();
        let (installed_version, chosen) = {
            let plugins = self.inner.plugins();
            let installed = plugins.iter().find(|p| p.entry.id == manifest.id);
            (
                installed.map(|p| {
                    p.manifest
                        .as_ref()
                        .map(|m| m.version.clone())
                        .unwrap_or_default()
                }),
                installed
                    .map(|p| p.entry.folders.clone())
                    .unwrap_or_default(),
            )
        };
        let home = home_dir();
        Ok(PluginPreview {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            icon: staged.icon_data_url()?,
            stores: manifest.providers.len() as u32,
            has_extension: manifest.extension.is_some(),
            themes: manifest
                .themes
                .iter()
                .map(|theme| {
                    theme
                        .get("name")
                        .and_then(|name| name.as_str())
                        .unwrap_or("?")
                        .to_string()
                })
                .collect(),
            permissions: PluginPermissions {
                read: manifest
                    .permissions
                    .read
                    .iter()
                    .map(|r| r.path.clone())
                    .collect(),
                http: manifest.permissions.http.clone(),
                folders: manifest
                    .permissions
                    .folders
                    .iter()
                    .map(Into::into)
                    .collect(),
            },
            folders: manifest
                .permissions
                .folders
                .iter()
                .map(|f| plugin_folder(f, chosen.get(&f.key), home.as_deref()))
                .collect(),
            installed_version,
        })
    }

    /// Installs the plugin in `path` (a folder or a zip), enables and loads it. An installed
    /// plugin with the same id is replaced (an update).
    pub fn install(&self, path: &Path) -> Result<PluginInfo> {
        self.install_with_folders(path, &BTreeMap::new())
    }

    /// [`install`](Self::install) with the folders the user chose for the plugin's
    /// `permissions.folders` (key to absolute path). They are checked like
    /// [`set_folder`](Self::set_folder) checks one, before anything changes (one bad entry
    /// refuses the whole install and an installed copy stays), and saved together with the
    /// install, so the first load already mounts them. Earlier choices stay unless `folders`
    /// names the key again; a blank path there goes back to the default.
    pub fn install_with_folders(
        &self,
        path: &Path,
        folders: &BTreeMap<String, String>,
    ) -> Result<PluginInfo> {
        let _change = self.inner.change();
        let source = std::path::absolute(path).map_err(|e| PluginError::io(path, e))?;
        let staged = install::stage(&source, &self.inner.paths.plugins_dir(), None)?;
        let id = staged.manifest().id.clone();
        let folders = self.check_install_folders(staged.manifest(), folders)?;
        // The running copy lets go of its files before they are replaced.
        self.inner.unload(&id);
        if let Err(err) = staged.commit() {
            self.inner.load(&id);
            self.inner.changed();
            return Err(err);
        }
        self.record_install(&id, source.display().to_string(), None, folders);
        let saved = self.inner.save();
        self.inner.load(&id);
        self.sync_watch(&id);
        self.inner.changed();
        tracing::info!(plugin = %id, source = %source.display(), "plugin installed");
        saved?;
        self.info(&id)
    }

    /// Installs version `version` of the registry plugin `id` from its cached zip `zip`, enables
    /// and loads it; `pinned` is remembered (no update offers). The zip's `plugin.json` must have
    /// that id and version, or nothing is installed. When a copy is replaced, the plugin's data
    /// folder is backed up first (see [`registry::backup_plugin_data`]); if that fails the old copy
    /// stays.
    pub fn install_registry(
        &self,
        id: &str,
        version: &str,
        zip: &Path,
        pinned: bool,
    ) -> Result<PluginInfo> {
        self.install_registry_with_folders(id, version, zip, pinned, &BTreeMap::new())
    }

    /// [`install_registry`](Self::install_registry) with the user's folders, see
    /// [`install_with_folders`](Self::install_with_folders).
    pub fn install_registry_with_folders(
        &self,
        id: &str,
        version: &str,
        zip: &Path,
        pinned: bool,
        folders: &BTreeMap<String, String>,
    ) -> Result<PluginInfo> {
        let _change = self.inner.change();
        let source = std::path::absolute(zip).map_err(|e| PluginError::io(zip, e))?;
        let staged = install::stage(&source, &self.inner.paths.plugins_dir(), None)?;
        let found = staged.manifest();
        if found.id != id || found.version != version {
            return Err(registry::mismatch(id, version, &found.id, &found.version));
        }
        let folders = self.check_install_folders(found, folders)?;
        let old_version = self
            .inner
            .plugins()
            .iter()
            .find(|p| p.entry.id == id)
            .map(|p| p.manifest.as_ref().map(|m| m.version.clone()));
        // The running copy lets go of its files (and of its data folder) before anything moves.
        self.inner.unload(id);
        if let Some(old_version) = old_version {
            let old_version = old_version.unwrap_or_else(|| "unknown".into());
            if let Err(err) = registry::backup_plugin_data(&self.inner.paths, id, &old_version) {
                self.inner.load(id);
                self.inner.changed();
                return Err(err);
            }
        }
        if let Err(err) = staged.commit() {
            self.inner.load(id);
            self.inner.changed();
            return Err(err);
        }
        self.record_install(
            id,
            source.display().to_string(),
            Some(RegistryState {
                version: version.to_string(),
                pinned,
            }),
            folders,
        );
        let saved = self.inner.save();
        self.inner.load(id);
        self.inner.changed();
        registry::prune_cache(&self.inner.paths, id, &source);
        tracing::info!(
            plugin = id,
            version,
            pinned,
            "plugin installed from the registry"
        );
        saved?;
        self.info(id)
    }

    /// Notes an install in the list: a new entry, or the existing one pointed at the new source.
    /// `folders` are the checked choices of this install: they win over earlier ones.
    fn record_install(
        &self,
        id: &str,
        source: String,
        registry: Option<RegistryState>,
        folders: BTreeMap<String, Option<String>>,
    ) {
        let mut plugins = self.inner.plugins();
        match plugins.iter_mut().find(|p| p.entry.id == id) {
            Some(plugin) => {
                plugin.entry.source = source;
                plugin.entry.enabled = true;
                plugin.entry.registry = registry;
                for (key, path) in folders {
                    match path {
                        Some(path) => plugin.entry.folders.insert(key, path),
                        None => plugin.entry.folders.remove(&key),
                    };
                }
            }
            None => plugins.push(Plugin::new(StateEntry {
                id: id.to_string(),
                source,
                enabled: true,
                watch: false,
                registry,
                folders: folders
                    .into_iter()
                    .filter_map(|(key, path)| Some((key, path?)))
                    .collect(),
            })),
        }
    }

    pub(crate) fn paths(&self) -> &DataPaths {
        &self.inner.paths
    }

    pub(crate) fn events(&self) -> &EventBus {
        &self.inner.events
    }

    /// Held while a registry download and the install that follows it run.
    pub(crate) fn registry_ops(&self) -> MutexGuard<'_, ()> {
        self.inner
            .registry_ops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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

    /// Points the plugin's folder `key` (one its manifest declares in `permissions.folders`) at
    /// `path`, or back to its default with `None`, and loads the plugin's stores again so they
    /// see the new folder. The choice is kept in `plugins.json` (see [`StateEntry::folders`]).
    pub fn set_folder(&self, id: &str, key: &str, path: Option<&str>) -> Result<PluginInfo> {
        let _change = self.inner.change();
        let manifest = self
            .manifest(id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        if !manifest.permissions.folders.iter().any(|f| f.key == key) {
            return Err(PluginError::Invalid(format!(
                "The plugin \"{}\" does not ask for a folder \"{key}\".",
                manifest.name
            )));
        }
        let chosen = match path.map(str::trim).filter(|p| !p.is_empty()) {
            Some(path) => Some(self.check_folder(id, Path::new(path))?),
            None => None,
        };
        self.update_entry(id, |entry| match &chosen {
            Some(path) => {
                entry.folders.insert(key.to_string(), path.clone());
            }
            None => {
                entry.folders.remove(key);
            }
        })?;
        let saved = self.inner.save();
        self.inner.unload(id);
        self.inner.load(id);
        self.inner.changed();
        tracing::info!(plugin = id, key, chosen = ?chosen, "plugin folder set");
        saved?;
        self.info(id)
    }

    /// The folders chosen for an install, checked against the staged manifest: every key is
    /// one it declares and every path passes [`check_folder`](Self::check_folder). A blank path
    /// (`None` in the result) means "back to the default". Returns the paths as they are stored.
    fn check_install_folders(
        &self,
        manifest: &PluginManifest,
        folders: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, Option<String>>> {
        let mut checked = BTreeMap::new();
        for (key, path) in folders {
            if !manifest.permissions.folders.iter().any(|f| &f.key == key) {
                return Err(PluginError::Invalid(format!(
                    "The plugin \"{}\" does not ask for a folder \"{key}\".",
                    manifest.name
                )));
            }
            let path = path.trim();
            let stored = if path.is_empty() {
                None
            } else {
                Some(self.check_folder(&manifest.id, Path::new(path))?)
            };
            checked.insert(key.clone(), stored);
        }
        Ok(checked)
    }

    /// The folder as it is stored: absolute, existing, a folder, canonical (without the
    /// `\\?\` prefix Windows adds), and not Questkeep's own data or the plugin's own files.
    fn check_folder(&self, id: &str, path: &Path) -> Result<String> {
        if !path.is_absolute() {
            return Err(PluginError::Invalid(format!(
                "\"{}\" is not a full path. Choose a folder with the folder picker.",
                path.display()
            )));
        }
        let real = std::fs::canonicalize(path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                PluginError::Invalid(format!("The folder {} does not exist.", path.display()))
            } else {
                PluginError::io(path, err)
            }
        })?;
        if !real.is_dir() {
            return Err(PluginError::Invalid(format!(
                "{} is not a folder.",
                path.display()
            )));
        }
        let own = |dir: PathBuf| std::fs::canonicalize(&dir).unwrap_or(dir);
        let data = own(self.inner.paths.root().to_path_buf());
        // Inside Questkeep's data (the library database, settings, other plugins' data) or a
        // folder that contains all of it: the plugin could read those.
        if real.starts_with(&data) || data.starts_with(&real) {
            return Err(PluginError::Invalid(
                "That folder is Questkeep's own data folder, or contains it. Choose another \
                 folder."
                    .into(),
            ));
        }
        if real.starts_with(own(self.plugin_dir(id))) {
            return Err(PluginError::Invalid(
                "That folder is the plugin's own installed folder. Choose another folder.".into(),
            ));
        }
        Ok(plain_path(&real).display().to_string())
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
        if let Err(err) = registry::remove_cache(&self.inner.paths, id) {
            tracing::warn!(plugin = id, %err, "could not delete the cached plugin download");
        }
        if delete_data {
            install::remove_dir(&self.inner.paths.plugin_data_dir().join(id))?;
            registry::remove_backups(&self.inner.paths, id)?;
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
        let (enabled, chosen) = {
            let plugins = self.plugins();
            match plugins.iter().find(|p| p.entry.id == id) {
                Some(plugin) => (plugin.entry.enabled, plugin.entry.folders.clone()),
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
                let outcome = self.load_providers(&manifest, &dir, &chosen);
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
        chosen: &BTreeMap<String, String>,
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
            // The chosen folders are checked when they are chosen and again when mounted; a
            // missing one is simply not mounted.
            mounts: manifest
                .permissions
                .read
                .iter()
                .filter_map(|read| {
                    let path = expand_path(&read.path, |v| std::env::var(v).ok(), home.as_deref())?;
                    Some((path, read.mount.clone()))
                })
                .chain(manifest.permissions.folders.iter().filter_map(|folder| {
                    let path = folder_path(
                        folder,
                        chosen.get(&folder.key).map(String::as_str),
                        |v| std::env::var(v).ok(),
                        home.as_deref(),
                    )?;
                    Some((path, folder.mount.clone()))
                }))
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

/// A declared folder with what the user chose for it.
fn plugin_folder(
    folder: &FolderPermission,
    chosen: Option<&String>,
    home: Option<&Path>,
) -> PluginFolder {
    let effective = folder_path(
        folder,
        chosen.map(String::as_str),
        |v| std::env::var(v).ok(),
        home,
    );
    let default = folder_path(folder, None, |v| std::env::var(v).ok(), home);
    PluginFolder {
        key: folder.key.clone(),
        label: folder.label.clone(),
        hint: folder.hint.clone(),
        mount: folder.mount.clone(),
        default: folder.default.clone(),
        default_exists: default.as_deref().is_some_and(Path::is_dir),
        default_path: default.map(|p| p.display().to_string()),
        path: chosen.cloned(),
        exists: effective.as_deref().is_some_and(Path::is_dir),
        effective_path: effective.map(|p| p.display().to_string()),
    }
}

/// A canonical path without the `\\?\` prefix Windows puts on it (`\\?\C:\x` becomes `C:\x`,
/// `\\?\UNC\server\share` becomes `\\server\share`), so people see the path they know.
fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
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
        // Looking at a plugin installs nothing.
        let preview = manager.preview(&source).unwrap();
        assert_eq!(preview.id, "theme-pack");
        assert_eq!(preview.version, "1.0.0");
        assert_eq!(preview.themes.len(), 1);
        assert_eq!(preview.stores, 0);
        assert!(!preview.has_extension && preview.installed_version.is_none());
        assert_eq!(preview.icon, None);
        assert!(manager.list().is_empty() && fx.changes() == 0);
        assert!(!manager.plugin_dir("theme-pack").exists());
        assert!(manager.preview(&fx.root.join("nope")).is_err());

        let info = manager.install(&source).unwrap();
        assert_eq!(
            manager
                .preview(&source)
                .unwrap()
                .installed_version
                .as_deref(),
            Some("1.0.0")
        );
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
    fn an_icon_is_a_data_url_in_the_preview_and_a_path_in_the_list() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let source = fx.root.join("src/pic");
        write_plugin(&source, "pic", "1.0.0");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source.join("plugin.json")).unwrap()).unwrap();
        manifest["icon"] = "img/logo.svg".into();
        std::fs::write(source.join("plugin.json"), manifest.to_string()).unwrap();
        std::fs::create_dir_all(source.join("img")).unwrap();
        std::fs::write(source.join("img/logo.svg"), "<svg/>").unwrap();

        let preview = manager.preview(&source).unwrap();
        assert_eq!(
            preview.icon.as_deref(),
            Some("data:image/svg+xml;base64,PHN2Zy8+")
        );
        let info = manager.install(&source).unwrap();
        assert_eq!(info.icon.as_deref(), Some("img/logo.svg"));
        assert!(manager.file("pic", "img/logo.svg").is_some());
        // A disabled plugin hands out no files, so it offers no icon either.
        assert_eq!(manager.set_enabled("pic", false).unwrap().icon, None);

        // A bad icon keeps a plugin from being previewed or installed.
        std::fs::write(source.join("img/logo.svg"), "<svg><script/></svg>").unwrap();
        assert!(manager.preview(&source).is_err());
        assert!(manager.install(&source).is_err());
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

    /// A plugin zip like the registry serves one.
    fn registry_zip(fx: &Fixture, id: &str, version: &str, inside: (&str, &str)) -> PathBuf {
        let zip = fx.root.join(format!("zips/{id}-{version}.zip"));
        std::fs::create_dir_all(zip.parent().unwrap()).unwrap();
        let manifest = serde_json::json!({
            "id": inside.0, "name": "P", "version": inside.1, "apiVersion": 1,
            "themes": [{ "id": "t", "name": "T", "css": "theme.css" }],
        })
        .to_string();
        crate::install::testing::write_zip(
            &zip,
            &[
                ("plugin.json", manifest.as_bytes()),
                ("theme.css", version.as_bytes()),
            ],
        );
        zip
    }

    #[test]
    fn registry_installs_check_the_zip_and_record_where_it_came_from() {
        let fx = Fixture::new();
        let manager = fx.manager();
        // Another plugin or another version than requested: nothing is installed.
        let wrong_id = registry_zip(&fx, "pack", "1.0.0", ("other", "1.0.0"));
        let err = manager
            .install_registry("pack", "1.0.0", &wrong_id, false)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("\"other\"") && err.contains("not \"pack\""),
            "{err}"
        );
        let wrong_version = registry_zip(&fx, "pack", "1.0.0", ("pack", "1.0.1"));
        let err = manager
            .install_registry("pack", "1.0.0", &wrong_version, false)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("1.0.1") && err.contains("registry says 1.0.0"),
            "{err}"
        );
        assert!(manager.list().is_empty() && fx.changes() == 0);
        assert!(!manager.plugin_dir("pack").exists() && !manager.plugin_dir("other").exists());

        let zip = registry_zip(&fx, "pack", "1.0.0", ("pack", "1.0.0"));
        let info = manager
            .install_registry("pack", "1.0.0", &zip, true)
            .unwrap();
        assert_eq!(
            info.registry,
            Some(RegistryState {
                version: "1.0.0".into(),
                pinned: true
            })
        );
        assert_eq!(Path::new(&info.source), zip);
        assert!(info.loaded && fx.changes() == 1);
        // The state survives a restart.
        let again = fx.manager();
        again.load_enabled();
        assert_eq!(again.list()[0].registry, info.registry);

        // Without data there is nothing to back up.
        assert!(!fx.paths.plugin_data_backup_dir().exists());

        // Installing the same id from a folder takes it out of the registry's hands.
        let source = fx.root.join("src/pack");
        write_plugin(&source, "pack", "5.0.0");
        let info = manager.install(&source).unwrap();
        assert_eq!(info.registry, None);
        assert!(
            install::load_state(&fx.paths.plugins_dir())[0]
                .registry
                .is_none()
        );
    }

    #[test]
    fn registry_updates_back_up_the_data_and_keep_three_backups() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let data = fx.paths.plugin_data_dir().join("pack");
        for n in 1..=5 {
            let version = format!("{n}.0.0");
            let zip = registry_zip(&fx, "pack", &version, ("pack", &version));
            manager
                .install_registry("pack", &version, &zip, false)
                .unwrap();
            std::fs::create_dir_all(data.join("sub")).unwrap();
            std::fs::write(data.join("sub/state.txt"), format!("data of {version}")).unwrap();
        }
        assert_eq!(manager.list()[0].version, "5.0.0");
        // The copies made before the swaps (1.0.0 .. 4.0.0), the newest three are kept.
        let mut kept: Vec<String> =
            std::fs::read_dir(fx.paths.plugin_data_backup_dir().join("pack"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
        kept.sort();
        assert_eq!(kept, ["2.0.0", "3.0.0", "4.0.0"]);
        let backup = fx
            .paths
            .plugin_data_backup_dir()
            .join("pack/4.0.0/sub/state.txt");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "data of 4.0.0");
        // The live data is untouched.
        assert_eq!(
            std::fs::read_to_string(data.join("sub/state.txt")).unwrap(),
            "data of 5.0.0"
        );

        // Removing keeps the backups unless the data goes too.
        manager.remove("pack", false).unwrap();
        assert!(fx.paths.plugin_data_backup_dir().join("pack").is_dir());
        assert!(!fx.paths.plugin_cache_dir().join("pack").exists());
        let zip = registry_zip(&fx, "pack", "6.0.0", ("pack", "6.0.0"));
        manager
            .install_registry("pack", "6.0.0", &zip, false)
            .unwrap();
        manager.remove("pack", true).unwrap();
        assert!(!fx.paths.plugin_data_backup_dir().join("pack").exists());
        assert!(!data.exists());
    }

    #[test]
    fn a_failed_backup_stops_the_update() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let zip = registry_zip(&fx, "pack", "1.0.0", ("pack", "1.0.0"));
        manager
            .install_registry("pack", "1.0.0", &zip, false)
            .unwrap();
        let data = fx.paths.plugin_data_dir().join("pack");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("state.txt"), "precious").unwrap();
        // A file where the backup folder has to go.
        std::fs::create_dir_all(fx.paths.plugin_data_backup_dir()).unwrap();
        std::fs::write(fx.paths.plugin_data_backup_dir().join("pack"), "in the way").unwrap();

        let newer = registry_zip(&fx, "pack", "2.0.0", ("pack", "2.0.0"));
        let err = manager
            .install_registry("pack", "2.0.0", &newer, false)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("back up the data") && err.contains("Nothing was changed"),
            "{err}"
        );
        let info = &manager.list()[0];
        assert_eq!(info.version, "1.0.0");
        assert!(info.loaded);
        assert_eq!(info.registry.as_ref().unwrap().version, "1.0.0");
        assert_eq!(
            std::fs::read_to_string(data.join("state.txt")).unwrap(),
            "precious"
        );
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

    /// A plugin without stores that asks for two folders.
    fn write_folder_plugin(dir: &Path, version: &str, default: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        let manifest = serde_json::json!({
            "id": "folders", "name": "Folders", "version": version, "apiVersion": 1,
            "permissions": { "folders": [
                { "key": "games", "label": "Games", "hint": "Where they are", "as": "/games",
                  "default": default.display().to_string() },
                { "key": "other", "label": "Other", "as": "/other" }
            ] }
        });
        std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
    }

    #[test]
    fn folders_the_user_chooses_are_checked_kept_and_removed() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let default = fx.root.join("default-games");
        let chosen = fx.root.join("chosen");
        std::fs::create_dir_all(&chosen).unwrap();
        let source = fx.root.join("src/folders");
        write_folder_plugin(&source, "1.0.0", &default);

        // The install dialog lists what the plugin asks for.
        let preview = manager.preview(&source).unwrap();
        assert_eq!(preview.permissions.folders.len(), 2);
        assert_eq!(preview.permissions.folders[0].label, "Games");
        assert_eq!(
            preview.permissions.folders[0].hint.as_deref(),
            Some("Where they are")
        );

        // Nothing chosen: the default applies, and it is not there yet.
        let info = manager.install(&source).unwrap();
        let [games, other] = &info.folders[..] else {
            panic!("two folders");
        };
        assert_eq!(games.mount, "/games");
        assert_eq!(games.path, None);
        assert_eq!(
            games.effective_path.as_deref(),
            Some(default.display().to_string().as_str())
        );
        assert!(!games.exists);
        assert_eq!(other.effective_path, None);
        assert!(!other.exists);
        assert_eq!(info.permissions.folders.len(), 2);

        // Choosing stores the real path in plugins.json and reports it.
        let info = manager
            .set_folder("folders", "games", Some(&chosen.display().to_string()))
            .unwrap();
        let stored = install::load_state(&fx.paths.plugins_dir())[0].folders["games"].clone();
        assert_eq!(
            std::fs::canonicalize(&stored).unwrap(),
            std::fs::canonicalize(&chosen).unwrap()
        );
        assert!(!stored.starts_with(r"\\?\"), "{stored}");
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        assert_eq!(
            info.folders[0].effective_path.as_deref(),
            Some(stored.as_str())
        );
        assert!(info.folders[0].exists);
        assert_eq!(fx.changes(), 2);
        // It is not in the plugin's own writable data folder.
        assert!(!fx.paths.plugin_data_dir().join("folders").exists());

        // It survives a restart, an update and an install from another source.
        let again = fx.manager();
        again.load_enabled();
        assert_eq!(
            again.list()[0].folders[0].path.as_deref(),
            Some(stored.as_str())
        );
        write_folder_plugin(&source, "2.0.0", &default);
        let info = manager.install(&source).unwrap();
        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        let moved = fx.root.join("elsewhere/folders");
        write_folder_plugin(&moved, "3.0.0", &default);
        let info = manager.install(&moved).unwrap();
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        let info = manager.reload("folders").unwrap();
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));

        // A deleted folder is still the choice, but is reported missing.
        std::fs::remove_dir_all(&chosen).unwrap();
        let info = manager.list().remove(0);
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        assert!(!info.folders[0].exists);

        // Back to the default.
        let info = manager.set_folder("folders", "games", None).unwrap();
        assert_eq!(info.folders[0].path, None);
        assert_eq!(
            info.folders[0].effective_path.as_deref(),
            Some(default.display().to_string().as_str())
        );
        assert!(
            install::load_state(&fx.paths.plugins_dir())[0]
                .folders
                .is_empty()
        );

        // Removing the plugin removes the choices.
        std::fs::create_dir_all(&chosen).unwrap();
        manager
            .set_folder("folders", "other", Some(&chosen.display().to_string()))
            .unwrap();
        manager.remove("folders", false).unwrap();
        assert!(install::load_state(&fx.paths.plugins_dir()).is_empty());
        let info = manager.install(&source).unwrap();
        assert!(info.folders.iter().all(|f| f.path.is_none()));
    }

    #[test]
    fn a_folder_choice_is_refused_when_it_cannot_work() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let source = fx.root.join("src/folders");
        write_folder_plugin(&source, "1.0.0", &fx.root.join("default"));
        manager.install(&source).unwrap();
        let file = fx.root.join("a-file.txt");
        std::fs::write(&file, "x").unwrap();
        let inside_data = fx.paths.plugin_data_dir().join("folders");
        std::fs::create_dir_all(&inside_data).unwrap();
        let ask = |key: &str, path: &str| {
            manager
                .set_folder("folders", key, Some(path))
                .unwrap_err()
                .to_string()
        };
        let path = |p: &Path| p.display().to_string();
        assert!(ask("nope", &path(&fx.root)).contains("does not ask for a folder \"nope\""));
        assert!(ask("games", "relative/dir").contains("not a full path"));
        assert!(ask("games", &path(&fx.root.join("missing"))).contains("does not exist"));
        assert!(ask("games", &path(&file)).contains("not a folder"));
        // Questkeep's own data: the folder itself, below it, and what contains it.
        for own in [
            fx.paths.root().to_path_buf(),
            fx.paths.plugins_dir(),
            inside_data,
            fx.root.clone(),
        ] {
            assert!(
                ask("games", &path(&own)).contains("own data folder"),
                "{own:?}"
            );
        }
        let err = manager
            .set_folder("unknown", "games", Some(&path(&fx.root)))
            .unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
        // Nothing was stored by the refusals.
        assert!(
            install::load_state(&fx.paths.plugins_dir())[0]
                .folders
                .is_empty()
        );
        assert_eq!(fx.changes(), 1);
    }

    #[test]
    fn folders_chosen_in_the_install_dialog_are_checked_first_and_kept() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let default = fx.root.join("default-games");
        let chosen = fx.root.join("chosen");
        let other = fx.root.join("other");
        std::fs::create_dir_all(&chosen).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let source = fx.root.join("src/folders");
        write_folder_plugin(&source, "1.0.0", &default);
        let path = |p: &Path| p.display().to_string();
        let choose = |pairs: &[(&str, String)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect()
        };

        // The dialog sees the default, expanded, and that it is missing.
        let preview = manager.preview(&source).unwrap();
        let games = &preview.folders[0];
        assert_eq!(games.default_path.as_deref(), Some(path(&default).as_str()));
        assert!(!games.default_exists);
        assert_eq!(games.path, None);
        assert_eq!(preview.folders[1].default_path, None);

        // A bad entry fails the whole install: nothing is installed or written.
        let bad = choose(&[
            ("games", path(&chosen)),
            ("other", path(&fx.root.join("missing"))),
        ]);
        let err = manager.install_with_folders(&source, &bad).unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
        assert!(manager.list().is_empty());
        assert!(install::load_state(&fx.paths.plugins_dir()).is_empty());
        assert!(!fx.paths.plugins_dir().join("folders").exists());
        let undeclared = choose(&[("nope", path(&chosen))]);
        let err = manager
            .install_with_folders(&source, &undeclared)
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("does not ask for a folder \"nope\"")
        );
        // Questkeep's own data is refused like in Settings.
        let own = choose(&[("games", path(fx.paths.root()))]);
        let err = manager.install_with_folders(&source, &own).unwrap_err();
        assert!(err.to_string().contains("own data folder"), "{err}");
        assert!(manager.list().is_empty());

        // A good choice is stored with the install and already applies to the first load.
        let good = choose(&[("games", path(&chosen))]);
        let info = manager.install_with_folders(&source, &good).unwrap();
        let stored = install::load_state(&fx.paths.plugins_dir())[0].folders["games"].clone();
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        assert!(info.folders[0].exists);
        assert_eq!(info.folders[1].path, None);

        // The dialog of an update preselects the choice.
        let preview = manager.preview(&source).unwrap();
        assert_eq!(preview.folders[0].path.as_deref(), Some(stored.as_str()));
        assert!(preview.folders[0].exists);

        // An update without folders keeps it; a failing one keeps the old copy and the choice.
        write_folder_plugin(&source, "2.0.0", &default);
        let info = manager.install(&source).unwrap();
        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        write_folder_plugin(&source, "3.0.0", &default);
        let bad = choose(&[("other", path(&fx.root.join("missing")))]);
        assert!(manager.install_with_folders(&source, &bad).is_err());
        let info = manager.list().remove(0);
        assert_eq!(info.version, "2.0.0");
        assert!(info.loaded);
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        assert_eq!(info.folders[1].path, None);

        // An override replaces one key and leaves the other alone.
        let more = choose(&[("other", path(&other))]);
        let info = manager.install_with_folders(&source, &more).unwrap();
        assert_eq!(info.version, "3.0.0");
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        assert!(info.folders[1].path.is_some());
        // A blank path goes back to the default.
        let blank = choose(&[("other", "  ".into())]);
        let info = manager.install_with_folders(&source, &blank).unwrap();
        assert_eq!(info.folders[1].path, None);
        assert_eq!(info.folders[0].path.as_deref(), Some(stored.as_str()));
        let info = manager
            .install_with_folders(&source, &choose(&[("other", path(&other))]))
            .unwrap();
        assert!(info.folders[1].path.is_some());
        let again = choose(&[("games", path(&other))]);
        let info = manager.install_with_folders(&source, &again).unwrap();
        assert_eq!(
            std::fs::canonicalize(info.folders[0].path.as_deref().unwrap()).unwrap(),
            std::fs::canonicalize(&other).unwrap()
        );
        assert!(info.folders[1].path.is_some());
    }

    #[test]
    fn registry_installs_take_the_dialog_folders_too() {
        let fx = Fixture::new();
        let manager = fx.manager();
        let chosen = fx.root.join("chosen");
        std::fs::create_dir_all(&chosen).unwrap();
        let zip = fx.root.join("zips/folders-1.0.0.zip");
        std::fs::create_dir_all(zip.parent().unwrap()).unwrap();
        let manifest = serde_json::json!({
            "id": "folders", "name": "Folders", "version": "1.0.0", "apiVersion": 1,
            "permissions": { "folders": [{ "key": "games", "label": "Games", "as": "/games" }] }
        })
        .to_string();
        crate::install::testing::write_zip(&zip, &[("plugin.json", manifest.as_bytes())]);
        let ask = |path: &Path| BTreeMap::from([("games".to_string(), path.display().to_string())]);

        let err = manager
            .install_registry_with_folders(
                "folders",
                "1.0.0",
                &zip,
                false,
                &ask(&fx.root.join("x")),
            )
            .unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
        assert!(manager.list().is_empty());

        let info = manager
            .install_registry_with_folders("folders", "1.0.0", &zip, false, &ask(&chosen))
            .unwrap();
        assert!(info.folders[0].path.is_some() && info.folders[0].exists);
        assert!(info.registry.is_some());
    }

    #[test]
    fn verbatim_prefixes_do_not_reach_the_user() {
        assert_eq!(
            plain_path(Path::new(r"\\?\C:\Games\x")),
            PathBuf::from(r"C:\Games\x")
        );
        assert_eq!(
            plain_path(Path::new(r"\\?\UNC\server\share\x")),
            PathBuf::from(r"\\server\share\x")
        );
        // Other device paths are left alone.
        assert_eq!(
            plain_path(Path::new(r"\\?\Volume{1}\x")),
            PathBuf::from(r"\\?\Volume{1}\x")
        );
        assert_eq!(plain_path(Path::new("/home/me")), PathBuf::from("/home/me"));
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
