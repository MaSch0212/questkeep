//! Stores that run as sandboxed WebAssembly components.
//!
//! A [`WasmProvider`] is a [`LibraryProvider`] whose work happens inside a plugin. The plugin
//! sees its own folders and the folders and web hosts its manifest declares, nothing else; a
//! call that crashes or takes too long is stopped, its instance thrown away and the next call
//! starts a fresh one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::Duration;

use questkeep_plugin_sdk::wire::{self, CallArgs, Method, ProviderDescriptor};
use questkeep_store_api::auth::{AccountAuth, AccountStatus, AuthError, LoginWindow, WebSession};
use questkeep_store_api::model::{
    AccountInfo, Achievements, ActionKind, AddGameOption, GameMetadata, GameNews, ImportedGame,
    LauncherClose, LauncherProcess, PlayAction, TrackingMode,
};
use questkeep_store_api::settings::StoreSettings;
use questkeep_store_api::{ImageCandidate, LibraryProvider, ProviderError};
use serde::de::DeserializeOwned;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::access::{
    LoginSetup, PluginServices, StoreId, check_secret, gate_launcher, is_protected,
    known_folder_path, secret_entry,
};
use crate::error::{PluginError, Result};
use crate::manifest::{DATA_MOUNT, HostRule, PLUGIN_MOUNT, host_allowed, is_valid_id};

mod bindings {
    wasmtime::component::bindgen!({
        world: "provider",
        path: "../questkeep-plugin-sdk/wit",
    });
}

use bindings::questkeep::plugin::host;

/// How often the engine's clock advances; call deadlines are counted in these steps.
const TICK: Duration = Duration::from_millis(100);
/// What one instance may allocate.
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;
/// The largest answer a plugin may download.
const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Shown for stores without a usable icon of their own: a puzzle piece.
pub const GENERIC_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9h4a2.5 2.5 0 1 1 5 0h4v4a2.5 2.5 0 1 1 0 5v3H4z"/></svg>"#;

/// How long a plugin call may take before it is stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallLimits {
    /// Scans, metadata, images and everything else that may read many files or use the network.
    pub slow: Duration,
    /// Everything that must answer right away.
    pub quick: Duration,
}

impl Default for CallLimits {
    fn default() -> Self {
        Self {
            slow: Duration::from_secs(120),
            quick: Duration::from_secs(10),
        }
    }
}

fn is_slow(method: Method) -> bool {
    match method {
        Method::ScanInstalled
        | Method::ScanOwned
        | Method::ScanCategories
        | Method::Metadata
        | Method::PlayerCount
        | Method::News
        | Method::Achievements
        | Method::CoverCandidates
        | Method::BackgroundCandidates
        | Method::IconCandidates
        | Method::SignInWithCode => true,
        Method::InstallAction
        | Method::UninstallAction
        | Method::LauncherPage
        | Method::LaunchWithArgs
        | Method::CoversAreLowQuality
        | Method::HasOwnedSource
        | Method::Configure
        | Method::RunAddGameOption
        | Method::AccountStatus
        | Method::SignOut => false,
    }
}

/// The engine all plugins share, and the clock that stops runaway calls.
pub(crate) struct Runtime {
    engine: Engine,
    linker: Linker<HostState>,
    limits: CallLimits,
    /// Loaded stores; the clock only ticks while there are some.
    live: Arc<AtomicUsize>,
    ticking: Arc<Mutex<bool>>,
}

impl Runtime {
    pub(crate) fn new(limits: CallLimits) -> Result<Self> {
        let failed = |err: wasmtime::Error| {
            PluginError::Load(format!("The plugin engine could not start: {err}."))
        };
        let mut config = wasmtime::Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(failed)?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(failed)?;
        bindings::Provider::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(failed)?;
        Ok(Self {
            engine,
            linker,
            limits,
            live: Arc::new(AtomicUsize::new(0)),
            ticking: Arc::new(Mutex::new(false)),
        })
    }

    /// Counts a loaded store and makes sure the clock runs.
    fn track(&self) -> LiveGuard {
        self.live.fetch_add(1, Ordering::SeqCst);
        let mut ticking = self.ticking.lock().unwrap_or_else(PoisonError::into_inner);
        if !*ticking {
            let engine = self.engine.clone();
            let live = self.live.clone();
            let flag = self.ticking.clone();
            let spawned = std::thread::Builder::new()
                .name("plugin-clock".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(TICK);
                        {
                            let mut ticking = flag.lock().unwrap_or_else(PoisonError::into_inner);
                            if live.load(Ordering::SeqCst) == 0 {
                                *ticking = false;
                                return;
                            }
                        }
                        engine.increment_epoch();
                    }
                });
            match spawned {
                Ok(_) => *ticking = true,
                Err(err) => tracing::error!(%err, "could not start the plugin clock"),
            }
        }
        LiveGuard(self.live.clone())
    }

    fn ticks(deadline: Duration) -> u64 {
        // One more: the first tick may come right after the call starts.
        (deadline.as_millis() / TICK.as_millis()) as u64 + 2
    }
}

struct LiveGuard(Arc<AtomicUsize>);

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What one plugin's stores may reach.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sandbox {
    pub plugin_id: String,
    /// The installed copy, mounted read-only at `/plugin`.
    pub plugin_dir: PathBuf,
    /// The plugin's own data, mounted read-write at `/data`.
    pub data_dir: PathBuf,
    /// The folders the user may let the plugin read (`permissions.read` and the chosen or default
    /// `permissions.folders`): host path (already expanded) and guest path, read-only.
    pub mounts: Vec<(PathBuf, String)>,
    pub hosts: Vec<HostRule>,
    /// `permissions.filesystem`: the whole file system is in `mounts` (below `/host`).
    pub filesystem: bool,
    /// `permissions.registry`.
    pub registry: bool,
    pub home: Option<PathBuf>,
    /// Folders no path of a store may lead into when Questkeep launches or installs something:
    /// the installed plugins and their data, which plugins write to.
    pub protected: Vec<PathBuf>,
    /// `permissions.launcher.processNames`.
    pub launcher_names: Vec<String>,
    /// `permissions.login`, built into the window the shell opens.
    pub login: Option<LoginSetup>,
    pub services: PluginServices,
    /// The id of the store this sandbox belongs to, known once the plugin has said it.
    pub store_id: StoreId,
}

impl Sandbox {
    fn wasi(&self) -> std::result::Result<WasiCtx, String> {
        std::fs::create_dir_all(&self.data_dir).map_err(|e| {
            format!(
                "the plugin's data folder {} could not be created: {e}",
                self.data_dir.display()
            )
        })?;
        // Nothing is inherited: no environment, arguments, stdio or network.
        let mut builder = WasiCtxBuilder::new();
        builder
            .allow_blocking_current_thread(true)
            .allow_tcp(false)
            .allow_udp(false)
            .allow_ip_name_lookup(false);
        let mount = |builder: &mut WasiCtxBuilder, dir: &Path, guest: &str, perms: FsPerms| {
            builder
                .preopened_dir(dir, guest, perms)
                .map(|_| ())
                .map_err(|e| format!("the folder {} could not be opened: {e}", dir.display()))
        };
        mount(
            &mut builder,
            &self.plugin_dir,
            PLUGIN_MOUNT,
            FsPerms::ReadOnly,
        )?;
        mount(&mut builder, &self.data_dir, DATA_MOUNT, FsPerms::ReadWrite)?;
        for (dir, guest) in &self.mounts {
            // A store that is not installed has no folder: the plugin sees no mount.
            if dir.is_dir()
                && let Err(err) = mount(&mut builder, dir, guest, FsPerms::ReadOnly)
            {
                tracing::warn!(plugin = %self.plugin_id, %err, "skipped a folder permission");
            }
        }
        Ok(builder.build())
    }

    /// The real file behind a path the plugin sees, or `None` for a path outside its mounts.
    /// Good for what the host only reads (images).
    fn host_path(&self, guest: &str) -> Option<PathBuf> {
        self.resolve(guest, true)
    }

    /// What a path the plugin returns as a file of a game is on this PC: below a folder mount
    /// (`permissions.read` and `folders`) it is the real path, anything else is returned as it
    /// is. `/plugin` and `/data` never translate: the plugin writes `/data`, so a path there
    /// must not become something Questkeep launches. The same goes for every path that leads into
    /// the installed plugins or their data, also through the whole-file-system mount.
    fn translate(&self, path: &str) -> String {
        match self.resolve(path, false) {
            Some(host) if is_protected(&host, &self.protected) => path.to_string(),
            Some(host) => host.display().to_string(),
            None => path.to_string(),
        }
    }

    fn translate_action(&self, mut action: PlayAction) -> PlayAction {
        if action.kind == ActionKind::File {
            action.target = self.translate(&action.target);
        }
        action.working_dir = action.working_dir.map(|dir| self.translate(&dir));
        if action.tracking == TrackingMode::Directory {
            action.tracking_path = action.tracking_path.map(|dir| self.translate(&dir));
        }
        action
    }

    fn translate_game(&self, mut game: ImportedGame) -> ImportedGame {
        game.install_dir = game.install_dir.map(|dir| self.translate(&dir));
        game.play_action = game.play_action.map(|a| self.translate_action(a));
        game
    }

    /// The launcher as far as the manifest declares its processes, its command's paths
    /// translated.
    fn translate_launcher(&self, process: LauncherProcess) -> Option<LauncherProcess> {
        let mut process = gate_launcher(process, &self.launcher_names)?;
        if let LauncherClose::Command(action) = process.close {
            process.close = LauncherClose::Command(self.translate_action(action));
        }
        Some(process)
    }

    /// `own`: `/plugin` and `/data` count as mounts too.
    fn resolve(&self, guest: &str, own: bool) -> Option<PathBuf> {
        let guest = guest.replace('\\', "/");
        if !guest.starts_with('/') || guest.split('/').any(|part| part == "..") {
            return None;
        }
        let mut roots: Vec<(&str, &Path)> = Vec::new();
        if own {
            roots.push((PLUGIN_MOUNT, &self.plugin_dir));
            roots.push((DATA_MOUNT, &self.data_dir));
        }
        roots.extend(
            self.mounts
                .iter()
                .map(|(dir, g)| (g.as_str(), dir.as_path())),
        );
        roots
            .into_iter()
            .filter_map(|(root, dir)| {
                let below = guest.strip_prefix(root)?;
                (below.is_empty() || below.starts_with('/')).then(|| {
                    let mut path = dir.to_path_buf();
                    path.extend(below.split('/').filter(|p| !p.is_empty() && *p != "."));
                    (root.len(), path)
                })
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, path)| path)
    }
}

/// What the host functions of one instance work with.
struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    plugin_id: Arc<str>,
    hosts: Arc<[HostRule]>,
    http: ureq::Agent,
    sandbox: Arc<Sandbox>,
}

/// What a store needs for its website session: its login setup, the service and the store id.
type WebSessionParts<'a> = (&'a LoginSetup, &'a Arc<dyn WebSession>, &'a str);

impl HostState {
    fn registry_allowed(&self) -> std::result::Result<(), String> {
        if self.sandbox.registry {
            Ok(())
        } else {
            Err(
                "the plugin has no permission to read the registry (permissions.registry in \
                 plugin.json)"
                    .into(),
            )
        }
    }

    fn filesystem_allowed(&self) -> std::result::Result<(), String> {
        if self.sandbox.filesystem {
            Ok(())
        } else {
            Err(
                "the plugin has no permission to read the file system (permissions.filesystem \
                 in plugin.json)"
                    .into(),
            )
        }
    }

    fn web_session(&self) -> std::result::Result<WebSessionParts<'_>, String> {
        let login = self.sandbox.login.as_ref().ok_or(
            "the plugin has no permission to use a website session (permissions.login in \
             plugin.json)",
        )?;
        if login.window.web_session.is_none() {
            return Err(
                "the plugin declares no website session (permissions.login.session)".into(),
            );
        }
        let web = self
            .sandbox
            .services
            .web
            .as_ref()
            .ok_or("website sessions are not available here")?;
        let store = self
            .sandbox
            .store_id
            .get()
            .ok_or("the store is not ready yet")?;
        Ok((login, web, store))
    }
}

/// A path of this PC as the plugin sees it.
fn guest(native: &Path) -> String {
    questkeep_plugin_sdk::host_path(&native.to_string_lossy())
        .unwrap_or_else(|| native.display().to_string())
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl host::Host for HostState {
    fn log(&mut self, level: host::LogLevel, message: String) {
        let plugin = &*self.plugin_id;
        match level {
            host::LogLevel::Debug => tracing::debug!(plugin, "{message}"),
            host::LogLevel::Info => tracing::info!(plugin, "{message}"),
            host::LogLevel::Warn => tracing::warn!(plugin, "{message}"),
            host::LogLevel::Error => tracing::error!(plugin, "{message}"),
        }
    }

    fn platform(&mut self) -> String {
        if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        }
        .into()
    }

    fn known_folder(&mut self, name: String) -> Option<String> {
        self.filesystem_allowed().ok()?;
        known_folder_path(&name, self.sandbox.home.as_deref()).map(|p| guest(&p))
    }

    fn registry_read(&mut self, path: String) -> std::result::Result<Option<String>, String> {
        self.registry_allowed()?;
        Ok(questkeep_store_api::platform::registry_value(&path))
    }

    fn registry_enum(&mut self, path: String) -> std::result::Result<Vec<String>, String> {
        self.registry_allowed()?;
        Ok(questkeep_store_api::platform::registry_subkeys(&path))
    }

    fn uninstall_entries(&mut self) -> std::result::Result<Vec<host::UninstallEntry>, String> {
        self.registry_allowed()?;
        Ok(questkeep_store_api::platform::uninstall_entries()
            .into_iter()
            .map(|e| host::UninstallEntry {
                key: e.key,
                display_name: e.display_name,
                publisher: e.publisher,
                install_location: e.install_location,
                uninstall_string: e.uninstall_string,
                display_icon: e.display_icon,
            })
            .collect())
    }

    fn appx_packages(&mut self) -> std::result::Result<Vec<host::AppxPackage>, String> {
        self.registry_allowed()?;
        Ok(questkeep_store_api::platform::appx_packages()
            .into_iter()
            .map(|p| host::AppxPackage {
                full_name: p.full_name,
                root: guest(&p.root),
                app_ids: p.app_ids,
            })
            .collect())
    }

    fn sqlite_query(
        &mut self,
        path: String,
        sql: String,
        params: Vec<String>,
    ) -> std::result::Result<String, String> {
        self.filesystem_allowed()?;
        let host_path = self
            .sandbox
            .host_path(&path)
            .ok_or_else(|| format!("\"{path}\" is not a path below /host"))?;
        let rows = questkeep_store_api::sqlite::query_rows(&host_path, "database", &sql, &params)
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&rows).map_err(|e| e.to_string())
    }

    fn secret_get(&mut self, key: String) -> std::result::Result<Option<String>, String> {
        check_secret(&key, None)?;
        self.sandbox
            .services
            .secrets
            .load(&secret_entry(&self.plugin_id, &key))
            .map_err(|e| e.to_string())
    }

    fn secret_set(&mut self, key: String, value: String) -> std::result::Result<(), String> {
        check_secret(&key, Some(&value))?;
        self.sandbox
            .services
            .secrets
            .save(&secret_entry(&self.plugin_id, &key), &value)
            .map_err(|e| e.to_string())
    }

    fn secret_delete(&mut self, key: String) -> std::result::Result<(), String> {
        check_secret(&key, None)?;
        self.sandbox
            .services
            .secrets
            .delete(&secret_entry(&self.plugin_id, &key))
            .map_err(|e| e.to_string())
    }

    fn session_refresh(&mut self) -> std::result::Result<Option<String>, String> {
        let (login, web, store) = self.web_session()?;
        web.refresh(store, &login.window)
    }

    fn session_renew(&mut self) -> std::result::Result<Option<String>, String> {
        let (login, web, store) = self.web_session()?;
        let Some(url) = login.window.web_session.and_then(|page| page.renew_url) else {
            return Ok(None);
        };
        web.renew(store, &login.window, url)
    }

    fn http_request(
        &mut self,
        request: host::Request,
    ) -> std::result::Result<host::Response, String> {
        check_url(&self.hosts, &request.url)?;
        tracing::debug!(plugin = &*self.plugin_id, method = %request.method, url = %request.url, "plugin request");
        let mut builder = ureq::http::Request::builder()
            .method(request.method.as_str())
            .uri(request.url.as_str());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let invalid = |e: ureq::http::Error| format!("the request is not valid: {e}");
        let answer = match request.body {
            Some(body) => self.http.run(builder.body(body).map_err(invalid)?),
            None => self.http.run(builder.body(()).map_err(invalid)?),
        };
        let mut response = answer.map_err(|e| e.to_string())?;
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_string())))
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|e| match e {
                ureq::Error::BodyExceedsLimit(_) => format!(
                    "the answer is larger than {} MB",
                    MAX_RESPONSE_BYTES / (1024 * 1024)
                ),
                other => other.to_string(),
            })?;
        Ok(host::Response {
            status: response.status().as_u16(),
            headers,
            body,
        })
    }
}

/// Whether the plugin may request this URL: https (or http where declared) on a declared host.
fn check_url(rules: &[HostRule], url: &str) -> std::result::Result<(), String> {
    let uri: ureq::http::Uri = url
        .parse()
        .map_err(|_| format!("\"{url}\" is not a valid address"))?;
    let (Some(scheme), Some(host)) = (uri.scheme_str(), uri.host()) else {
        return Err(format!("\"{url}\" is not a complete web address"));
    };
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return Err(format!(
            "only https addresses can be requested, not {scheme}"
        ));
    }
    if host_allowed(rules, scheme, host) {
        Ok(())
    } else if scheme.eq_ignore_ascii_case("http") && host_allowed(rules, "https", host) {
        Err(format!(
            "{host} may only be requested over https (the plugin does not declare http://{host})"
        ))
    } else {
        Err(format!(
            "the plugin has no permission to request {host} (it is not listed under \
             permissions.http in plugin.json)"
        ))
    }
}

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(concat!("Questkeep/", env!("CARGO_PKG_VERSION")))
        // The plugin gets every answer, also 404 and friends.
        .http_status_as_error(false)
        // A redirect may point at a host the plugin did not declare: the plugin gets the
        // redirect itself and has to ask again.
        .max_redirects(0)
        .max_redirects_will_error(false)
        .build()
        .into()
}

/// The icon as it may be put into the page: plugins hand in markup, so anything that could run
/// code is replaced by [`GENERIC_ICON`].
pub fn sanitize_icon(svg: &str) -> &str {
    let trimmed = svg.trim();
    let lower = trimmed.to_ascii_lowercase();
    let harmless = lower.starts_with("<svg")
        && !["<script", "onload=", "javascript:", "<foreignobject"]
            .iter()
            .any(|bad| lower.contains(bad))
        && !has_event_handler(&lower);
    if harmless { trimmed } else { GENERIC_ICON }
}

/// Whether the (lowercase) markup has an event handler attribute: `onclick=`, `onerror =`, ...
fn has_event_handler(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    lower.match_indices("on").any(|(at, _)| {
        let starts_word = at == 0
            || bytes[at - 1].is_ascii_whitespace()
            || matches!(bytes[at - 1], b'"' | b'\x27' | b'/');
        let rest = &lower[at + 2..];
        let name = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
        starts_word && name.len() < rest.len() && name.trim_start().starts_with('=')
    })
}

struct Live {
    store: Store<HostState>,
    bindings: bindings::Provider,
}

#[derive(Default)]
struct Slot {
    live: Option<Live>,
    /// The plugin was unloaded: no new instance is made.
    closed: bool,
}

/// A store implemented by a plugin's WASM component.
pub struct WasmProvider {
    runtime: Arc<Runtime>,
    component: Component,
    sandbox: Sandbox,
    id: String,
    name: String,
    icon: String,
    shop_url: Option<String>,
    /// The last `describe` answer.
    described: RwLock<ProviderDescriptor>,
    /// The last settings handed to `configure`, replayed into a fresh instance.
    config: Mutex<Option<serde_json::Value>>,
    slot: Mutex<Slot>,
    _live: LiveGuard,
}

impl WasmProvider {
    /// Compiles the component, starts it and asks what it is.
    pub(crate) fn load(runtime: Arc<Runtime>, wasm: &Path, sandbox: Sandbox) -> Result<Self> {
        // Each store of a plugin has its own id, and so its own website session.
        let sandbox = Sandbox {
            store_id: StoreId::default(),
            ..sandbox
        };
        let file = wasm
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Read into memory: the installed file stays free to be replaced or deleted.
        let bytes = std::fs::read(wasm).map_err(|e| PluginError::io(wasm, e))?;
        let component = Component::new(&runtime.engine, &bytes).map_err(|err| {
            tracing::warn!(plugin = %sandbox.plugin_id, error = ?err, "invalid store component");
            PluginError::Load(format!(
                "{file} is not a store plugin for this version of Questkeep (it must be a \
                 WebAssembly component built for wasm32-wasip2): {}.",
                err.root_cause()
            ))
        })?;
        let guard = runtime.track();
        let mut live = instantiate(&runtime, &component, &sandbox).map_err(|err| {
            tracing::warn!(plugin = %sandbox.plugin_id, error = ?err, "store did not start");
            PluginError::Load(format!(
                "{file} could not be started: {}.",
                err.root_cause()
            ))
        })?;
        live.store
            .set_epoch_deadline(Runtime::ticks(runtime.limits.quick));
        let described = describe(&mut live).map_err(|err| {
            PluginError::Load(format!(
                "{file} did not describe its store: {}.",
                err.root_cause()
            ))
        })?;

        let plugin = &sandbox.plugin_id;
        let _ = sandbox.store_id.set(described.id.clone());
        let own_id = described.id == *plugin
            || described
                .id
                .strip_prefix(plugin.as_str())
                .is_some_and(|rest| rest.starts_with('-'));
        if !own_id || !is_valid_id(&described.id) {
            return Err(PluginError::Load(format!(
                "The store id \"{}\" does not belong to the plugin: it must be \"{plugin}\" or \
                 start with \"{plugin}-\".",
                described.id
            )));
        }
        if described.name.trim().is_empty() {
            return Err(PluginError::Load(format!(
                "The store \"{}\" has no name.",
                described.id
            )));
        }
        Ok(Self {
            id: described.id.clone(),
            name: described.name.trim().to_string(),
            icon: sanitize_icon(&described.icon_svg).to_string(),
            shop_url: described
                .shop_url
                .clone()
                .filter(|url| url.starts_with("https://") || url.starts_with("http://")),
            described: RwLock::new(described),
            config: Mutex::new(None),
            slot: Mutex::new(Slot {
                live: Some(live),
                closed: false,
            }),
            runtime,
            component,
            sandbox,
            _live: guard,
        })
    }

    /// Stops the store for good (its plugin was unloaded): the instance is dropped and later
    /// calls fail.
    pub(crate) fn close(&self) {
        let mut slot = self.slot();
        slot.closed = true;
        slot.live = None;
    }

    fn slot(&self) -> MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn described(&self) -> std::sync::RwLockReadGuard<'_, ProviderDescriptor> {
        self.described
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `work` in the instance (a new one when the last call broke it) with a deadline.
    fn run<T>(
        &self,
        deadline: Duration,
        work: impl FnOnce(&mut Live) -> wasmtime::Result<T>,
    ) -> std::result::Result<T, ProviderError> {
        let mut slot = self.slot();
        if slot.closed {
            return Err(ProviderError::NotFound(format!(
                "The {} plugin is not loaded any more.",
                self.name
            )));
        }
        if slot.live.is_none() {
            let mut live = instantiate(&self.runtime, &self.component, &self.sandbox)
                .map_err(|err| self.failure(&err, self.runtime.limits.quick))?;
            // The new instance knows nothing of the settings the old one was given.
            let config = self
                .config
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let Some(config) = config {
                live.store
                    .set_epoch_deadline(Runtime::ticks(self.runtime.limits.quick));
                let args = encode(&CallArgs::config(&config))?;
                match live
                    .bindings
                    .call_call(&mut live.store, Method::Configure.name(), &args)
                {
                    Ok(_) => {}
                    Err(err) => return Err(self.failure(&err, self.runtime.limits.quick)),
                }
            }
            tracing::info!(provider = %self.id, "started a fresh plugin instance");
            slot.live = Some(live);
        }
        let Some(live) = slot.live.as_mut() else {
            return Err(ProviderError::Unsupported);
        };
        live.store.set_epoch_deadline(Runtime::ticks(deadline));
        match work(live) {
            Ok(value) => Ok(value),
            Err(err) => {
                // Whatever state the instance is in now, it is not trusted again.
                slot.live = None;
                Err(self.failure(&err, deadline))
            }
        }
    }

    fn failure(&self, err: &wasmtime::Error, deadline: Duration) -> ProviderError {
        let timed_out = err.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt);
        tracing::warn!(provider = %self.id, timed_out, error = ?err, "plugin call failed");
        ProviderError::Parse(if timed_out {
            format!(
                "The {} plugin did not answer within {} seconds and was stopped.",
                self.name,
                deadline.as_secs().max(1)
            )
        } else {
            format!(
                "The {} plugin stopped with an error: {}.",
                self.name,
                err.root_cause()
            )
        })
    }

    fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        args: &CallArgs,
    ) -> std::result::Result<T, ProviderError> {
        let args = encode(args)?;
        let deadline = if is_slow(method) {
            self.runtime.limits.slow
        } else {
            self.runtime.limits.quick
        };
        let answer = self
            .run(deadline, |live| {
                live.bindings
                    .call_call(&mut live.store, method.name(), &args)
            })?
            .map_err(|err| self.provider_error(err))?;
        serde_json::from_str(&answer).map_err(|err| {
            tracing::warn!(provider = %self.id, method = method.name(), %err, "unreadable plugin answer");
            ProviderError::Parse(format!(
                "The {} plugin gave an answer Questkeep does not understand ({}).",
                self.name,
                method.name()
            ))
        })
    }

    /// For the methods that cannot report errors: the default answer, and a log line.
    fn call_or_default<T: DeserializeOwned + Default>(&self, method: Method, args: &CallArgs) -> T {
        self.call(method, args).unwrap_or_else(|err| {
            if !matches!(err, ProviderError::Unsupported) {
                tracing::warn!(provider = %self.id, method = method.name(), %err, "plugin call failed");
            }
            T::default()
        })
    }

    fn provider_error(&self, err: bindings::ProviderError) -> ProviderError {
        match err {
            bindings::ProviderError::NotFound(m) => ProviderError::NotFound(m),
            bindings::ProviderError::Io(m) => ProviderError::Io {
                path: format!("a file of the {} plugin", self.name),
                source: std::io::Error::other(m),
            },
            bindings::ProviderError::Parse(m) => ProviderError::Parse(m),
            bindings::ProviderError::Unsupported => ProviderError::Unsupported,
            bindings::ProviderError::NotConfigured(m) => ProviderError::NotConfigured(m),
            bindings::ProviderError::Network(m) => ProviderError::Network(m),
        }
    }

    /// Asks the plugin again what it is (after `configure`); the fixed parts stay.
    fn refresh(&self) {
        let fresh = self.run(self.runtime.limits.quick, describe);
        match fresh {
            Ok(fresh) if fresh.id == self.id => {
                *self
                    .described
                    .write()
                    .unwrap_or_else(PoisonError::into_inner) = fresh;
            }
            Ok(fresh) => {
                tracing::warn!(provider = %self.id, now = %fresh.id, "a plugin store changed its id; ignored");
            }
            Err(err) => tracing::warn!(provider = %self.id, %err, "could not describe the store"),
        }
    }

    /// A play action of a store call, its file paths translated.
    fn action(&self, method: Method, args: &CallArgs) -> Option<PlayAction> {
        let action: Option<PlayAction> = self.call_or_default(method, args);
        action.map(|a| self.sandbox.translate_action(a))
    }

    fn images(&self, method: Method, source_game_id: &str) -> Vec<ImageCandidate> {
        self.call_or_default::<Vec<wire::ImageCandidate>>(method, &CallArgs::game(source_game_id))
            .into_iter()
            .filter_map(|candidate| match candidate {
                wire::ImageCandidate::Url(url) => Some(ImageCandidate::Url(url)),
                wire::ImageCandidate::File(path) => {
                    let host = self.sandbox.host_path(&path);
                    if host.is_none() {
                        tracing::warn!(provider = %self.id, path, "image outside the plugin's folders");
                    }
                    host.map(ImageCandidate::File)
                }
            })
            .collect()
    }
}

fn encode(args: &CallArgs) -> std::result::Result<String, ProviderError> {
    serde_json::to_string(args).map_err(|e| ProviderError::Parse(e.to_string()))
}

fn instantiate(
    runtime: &Runtime,
    component: &Component,
    sandbox: &Sandbox,
) -> wasmtime::Result<Live> {
    let state = HostState {
        wasi: sandbox.wasi().map_err(wasmtime::Error::msg)?,
        table: ResourceTable::new(),
        limits: StoreLimitsBuilder::new().memory_size(MEMORY_LIMIT).build(),
        plugin_id: sandbox.plugin_id.as_str().into(),
        hosts: sandbox.hosts.clone().into(),
        http: http_agent(),
        sandbox: Arc::new(sandbox.clone()),
    };
    let mut store = Store::new(&runtime.engine, state);
    store.limiter(|state| &mut state.limits);
    store.set_epoch_deadline(Runtime::ticks(runtime.limits.quick));
    let bindings = bindings::Provider::instantiate(&mut store, component, &runtime.linker)?;
    Ok(Live { store, bindings })
}

fn describe(live: &mut Live) -> wasmtime::Result<ProviderDescriptor> {
    let json = live.bindings.call_describe(&mut live.store)?;
    serde_json::from_str(&json)
        .map_err(|e| wasmtime::Error::msg(format!("the description is not valid ({e})")))
}

impl LibraryProvider for WasmProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn icon_svg(&self) -> &str {
        &self.icon
    }

    fn can_scan(&self) -> bool {
        self.described().can_scan
    }

    fn scan_installed(&self) -> std::result::Result<Vec<ImportedGame>, ProviderError> {
        let games: Vec<ImportedGame> = self.call(Method::ScanInstalled, &CallArgs::none())?;
        Ok(games
            .into_iter()
            .map(|g| self.sandbox.translate_game(g))
            .collect())
    }

    fn can_scan_owned(&self) -> bool {
        self.described().can_scan_owned
    }

    fn scan_owned(
        &self,
        config: &serde_json::Value,
    ) -> std::result::Result<Vec<ImportedGame>, ProviderError> {
        let games: Vec<ImportedGame> = self.call(Method::ScanOwned, &CallArgs::config(config))?;
        Ok(games
            .into_iter()
            .map(|g| self.sandbox.translate_game(g))
            .collect())
    }

    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        self.action(Method::InstallAction, &CallArgs::game(source_game_id))
    }

    fn can_uninstall(&self) -> bool {
        self.described().can_uninstall
    }

    fn uninstall_action(&self, source_game_id: &str) -> Option<PlayAction> {
        self.action(Method::UninstallAction, &CallArgs::game(source_game_id))
    }

    fn has_launcher_pages(&self) -> bool {
        self.described().has_launcher_pages
    }

    fn shop_url(&self) -> Option<&str> {
        self.shop_url.as_deref()
    }

    fn launcher_page(&self, source_game_id: &str) -> Option<PlayAction> {
        self.action(Method::LauncherPage, &CallArgs::game(source_game_id))
    }

    fn scan_categories(
        &self,
        config: &serde_json::Value,
    ) -> std::result::Result<Option<HashMap<String, Vec<String>>>, ProviderError> {
        self.call(Method::ScanCategories, &CallArgs::config(config))
    }

    fn metadata(
        &self,
        source_game_id: &str,
    ) -> std::result::Result<Option<GameMetadata>, ProviderError> {
        self.call(Method::Metadata, &CallArgs::game(source_game_id))
    }

    fn player_count(
        &self,
        source_game_id: &str,
    ) -> std::result::Result<Option<u32>, ProviderError> {
        self.call(Method::PlayerCount, &CallArgs::game(source_game_id))
    }

    fn news(&self, source_game_id: &str) -> std::result::Result<Option<GameNews>, ProviderError> {
        self.call(Method::News, &CallArgs::game(source_game_id))
    }

    fn achievements(
        &self,
        config: &serde_json::Value,
        source_game_id: &str,
    ) -> std::result::Result<Option<Achievements>, ProviderError> {
        let args = CallArgs {
            game_id: source_game_id.to_string(),
            ..CallArgs::config(config)
        };
        self.call(Method::Achievements, &args)
    }

    fn configure(&self, config: &serde_json::Value) {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = Some(config.clone());
        self.call_or_default::<()>(Method::Configure, &CallArgs::config(config));
        self.refresh();
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.images(Method::CoverCandidates, source_game_id)
    }

    fn covers_are_low_quality(&self, source_game_id: &str) -> bool {
        self.call_or_default(Method::CoversAreLowQuality, &CallArgs::game(source_game_id))
    }

    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.images(Method::BackgroundCandidates, source_game_id)
    }

    fn icon_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.images(Method::IconCandidates, source_game_id)
    }

    fn launch_with_args(&self, source_game_id: &str, args: &str) -> Option<PlayAction> {
        let call = CallArgs {
            args: args.to_string(),
            ..CallArgs::game(source_game_id)
        };
        self.action(Method::LaunchWithArgs, &call)
    }

    fn launcher(&self) -> Option<PlayAction> {
        let action = self.described().launcher.clone()?;
        Some(self.sandbox.translate_action(action))
    }

    fn launcher_process(&self) -> Option<LauncherProcess> {
        let process = self.described().launcher_process.clone()?;
        self.sandbox.translate_launcher(process)
    }

    fn account(&self) -> Option<&dyn AccountAuth> {
        (self.sandbox.login.is_some() && self.described().has_account)
            .then_some(self as &dyn AccountAuth)
    }

    fn is_installed(&self) -> bool {
        self.described().installed
    }

    fn has_owned_source(&self, config: &serde_json::Value) -> bool {
        self.call_or_default(Method::HasOwnedSource, &CallArgs::config(config))
    }

    /// Plugin stores get option switches only: credentials would put secrets in their hands.
    fn settings(&self) -> StoreSettings {
        StoreSettings {
            options: self.described().settings.options.clone(),
            credentials: None,
            notice: self.described().settings.notice.clone(),
        }
    }

    fn add_game_options(&self) -> Vec<AddGameOption> {
        self.described().add_game_options.clone()
    }

    fn run_add_game_option(&self, option_id: &str) -> std::result::Result<(), ProviderError> {
        let args = CallArgs {
            option_id: option_id.to_string(),
            ..CallArgs::none()
        };
        self.call(Method::RunAddGameOption, &args)
    }
}

impl AccountAuth for WasmProvider {
    fn status(&self) -> AccountStatus {
        let info: AccountInfo = self.call_or_default(Method::AccountStatus, &CallArgs::none());
        AccountStatus {
            provider: self.id.clone(),
            name: self.name.clone(),
            signed_in: info.signed_in,
            display_name: info.display_name,
            supports_code_sign_in: info.supports_code_sign_in,
            code_url: info.code_url,
            code_hint: info.code_hint,
        }
    }

    fn login_url(&self) -> String {
        self.sandbox
            .login
            .as_ref()
            .map(|login| login.url.clone())
            .unwrap_or_default()
    }

    fn login_window(&self) -> LoginWindow {
        self.sandbox
            .login
            .as_ref()
            .map(|login| login.window)
            .unwrap_or(LoginWindow {
                title: "",
                // No login permission: a window that can go nowhere.
                domains: &["invalid"],
                rule: None,
                user_agent: None,
                script: None,
                web_session: None,
                capture: |_| None,
            })
    }

    fn sign_in_with_code(&self, input: &str) -> std::result::Result<(), AuthError> {
        let args = CallArgs {
            input: input.to_string(),
            ..CallArgs::none()
        };
        self.call::<()>(Method::SignInWithCode, &args)
            .map_err(|e| match e {
                ProviderError::Network(m) => AuthError::Network(m),
                other => AuthError::Rejected(other.to_string()),
            })
    }

    fn sign_out(&self) -> std::result::Result<(), AuthError> {
        self.call::<()>(Method::SignOut, &CallArgs::none())
            .map_err(|e| AuthError::Rejected(e.to_string()))?;
        let has_session = self
            .sandbox
            .login
            .as_ref()
            .is_some_and(|login| login.window.web_session.is_some());
        if let (true, Some(web)) = (has_session, self.sandbox.services.web.as_ref()) {
            web.clear(&self.id).map_err(AuthError::Storage)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harmless_icons_pass_and_others_become_the_generic_one() {
        let good = r#"<svg viewBox="0 0 24 24"><path d="M1 1h2" stroke="currentColor"/></svg>"#;
        assert_eq!(sanitize_icon(good), good);
        assert_eq!(sanitize_icon(&format!("  {good}\n")), good);
        // "on" inside harmless words and values is fine.
        let on = r#"<svg><path d="M1 1" stroke-linejoin="round" font="mono=1"/></svg>"#;
        assert_eq!(sanitize_icon(on), on);
        for bad in [
            "",
            "<p>hi</p>",
            "<?xml version=\"1.0\"?><svg/>",
            "<svg><script>alert(1)</script></svg>",
            "<svg><SCRIPT>alert(1)</SCRIPT></svg>",
            "<svg onload=\"alert(1)\"/>",
            "<svg OnLoad=alert(1)></svg>",
            "<svg><a href=\"javascript:alert(1)\">x</a></svg>",
            "<svg><a href=\"JavaScript:alert(1)\">x</a></svg>",
            "<svg><foreignObject><body/></foreignObject></svg>",
            "<svg><image onerror=\"alert(1)\" href=\"x\"/></svg>",
            "<svg><g onclick = 'x()'/></svg>",
        ] {
            assert_eq!(sanitize_icon(bad), GENERIC_ICON, "{bad}");
        }
        assert_eq!(sanitize_icon(GENERIC_ICON), GENERIC_ICON);
    }

    fn rules(entries: &[&str]) -> Vec<HostRule> {
        entries.iter().filter_map(|e| HostRule::parse(e)).collect()
    }

    #[test]
    fn requests_need_a_declared_host() {
        let rules = rules(&["api.example.com", "*.cdn.example.com", "http://localhost"]);
        assert!(check_url(&rules, "https://api.example.com/v1/games?x=1").is_ok());
        assert!(check_url(&rules, "https://API.example.com:8443/").is_ok());
        assert!(check_url(&rules, "https://img.cdn.example.com/a.png").is_ok());
        assert!(check_url(&rules, "http://localhost:8080/x").is_ok());
        let err = check_url(&rules, "http://api.example.com/").unwrap_err();
        assert!(err.contains("only be requested over https"), "{err}");
        let err = check_url(&rules, "https://evil.example.net/").unwrap_err();
        assert!(
            err.contains("no permission to request evil.example.net"),
            "{err}"
        );
        // The host is what counts, not what the address looks like.
        assert!(check_url(&rules, "https://api.example.com@evil.net/").is_err());
        assert!(check_url(&rules, "https://evil.net/?api.example.com").is_err());
        assert!(check_url(&rules, "https://evil.net/#@api.example.com").is_err());
        assert!(check_url(&rules, "https://api.example.com.evil.net/").is_err());
        assert!(check_url(&rules, "file:///C:/secret.txt").is_err());
        assert!(check_url(&rules, "ftp://api.example.com/").is_err());
        assert!(check_url(&rules, "/relative").is_err());
        assert!(check_url(&rules, "not a url").is_err());
        assert!(check_url(&[], "https://api.example.com/").is_err());
    }

    #[test]
    fn guest_paths_map_into_the_mounts_only() {
        let sandbox = Sandbox {
            plugin_id: "p".into(),
            plugin_dir: PathBuf::from("/host/plugins/p"),
            data_dir: PathBuf::from("/host/plugin-data/p"),
            mounts: vec![
                (PathBuf::from("/games/store"), "/store".into()),
                (PathBuf::from("/games/deep"), "/store/deep".into()),
            ],
            hosts: Vec::new(),
            ..Sandbox::default()
        };
        let path = |guest: &str| sandbox.host_path(guest);
        assert_eq!(
            path("/plugin/img/a.png"),
            Some(PathBuf::from("/host/plugins/p/img/a.png"))
        );
        assert_eq!(path("/data"), Some(PathBuf::from("/host/plugin-data/p")));
        assert_eq!(
            path("/store/x/./y.jpg"),
            Some(PathBuf::from("/games/store/x/y.jpg"))
        );
        assert_eq!(
            path("/store/deep/z.jpg"),
            Some(PathBuf::from("/games/deep/z.jpg"))
        );
        assert_eq!(
            path("\\plugin\\a.png"),
            Some(PathBuf::from("/host/plugins/p/a.png"))
        );
        for outside in [
            "/plugin/../secret",
            "/plugins/x",
            "/etc/passwd",
            "plugin/a.png",
            "C:/Windows/x.png",
            "/storeroom/a",
            "",
        ] {
            assert_eq!(path(outside), None, "{outside}");
        }
    }

    fn folder_sandbox() -> Sandbox {
        Sandbox {
            plugin_id: "p".into(),
            plugin_dir: PathBuf::from("/host/plugins/p"),
            data_dir: PathBuf::from("/host/plugin-data/p"),
            mounts: vec![
                (PathBuf::from("/games/instances"), "/instances".into()),
                (PathBuf::from("/games/more"), "/instances/more".into()),
            ],
            hosts: Vec::new(),
            ..Sandbox::default()
        }
    }

    #[test]
    fn paths_below_a_folder_mount_become_real_paths() {
        let sandbox = folder_sandbox();
        let host = |guest: &str| PathBuf::from(sandbox.translate(guest));
        assert_eq!(
            host("/instances/My Instance/Beat Saber.exe"),
            PathBuf::from("/games/instances/My Instance/Beat Saber.exe")
        );
        // The mount itself, with trailing slashes and dots, and the longest mount wins.
        assert_eq!(host("/instances"), PathBuf::from("/games/instances"));
        assert_eq!(host("/instances/"), PathBuf::from("/games/instances"));
        assert_eq!(
            host("/instances//a/./b/"),
            PathBuf::from("/games/instances/a/b")
        );
        assert_eq!(host("/instances/more/x"), PathBuf::from("/games/more/x"));
        assert_eq!(
            host("\\instances\\a\\b.exe"),
            PathBuf::from("/games/instances/a/b.exe")
        );
    }

    #[test]
    fn other_paths_stay_as_they_are() {
        let sandbox = folder_sandbox();
        for same in [
            // Escapes out of the mount are not resolved, not even partly.
            "/instances/../x",
            "/instances/a/../../x",
            "/instances/..",
            // Not below a mount: the name has to match whole parts, and case.
            "/instancesx/a.exe",
            "/Instances/a.exe",
            "/INSTANCES",
            "/other/a.exe",
            "C:/Games/a.exe",
            "D:\\Games\\a.exe",
            "relative/a.exe",
            "",
            // The plugin's own folders never become launchable: it writes /data.
            "/data/evil.exe",
            "/plugin/a.exe",
        ] {
            assert_eq!(sandbox.translate(same), same);
        }
        // Images may still come from the plugin's own folders.
        assert_eq!(
            sandbox.host_path("/data/a.png"),
            Some(PathBuf::from("/host/plugin-data/p/a.png"))
        );
    }

    #[test]
    fn play_actions_and_games_translate_their_file_paths() {
        let sandbox = folder_sandbox();
        let mut action = PlayAction::file("/instances/Game/Game.exe");
        action.working_dir = Some("/instances/Game".into());
        action.args = Some("/instances/not-a-path".into());
        action.tracking = TrackingMode::Directory;
        action.tracking_path = Some("/instances/Game".into());
        let done = sandbox.translate_action(action.clone());
        assert_eq!(
            PathBuf::from(&done.target),
            PathBuf::from("/games/instances/Game/Game.exe")
        );
        assert_eq!(
            PathBuf::from(done.working_dir.unwrap()),
            PathBuf::from("/games/instances/Game")
        );
        assert_eq!(
            PathBuf::from(done.tracking_path.unwrap()),
            PathBuf::from("/games/instances/Game")
        );
        assert_eq!(done.args.as_deref(), Some("/instances/not-a-path"));
        // A URI is not a path; a tracked name is not either.
        let mut uri = PlayAction::uri("/instances/x");
        uri.tracking = TrackingMode::Name;
        uri.tracking_path = Some("/instances/x".into());
        assert_eq!(sandbox.translate_action(uri.clone()), uri);

        let mut game = ImportedGame::installed("1", "G");
        game.install_dir = Some("/instances/Game".into());
        game.play_action = Some(PlayAction::file("/instances/Game/Game.exe"));
        let game = sandbox.translate_game(game);
        assert_eq!(
            PathBuf::from(game.install_dir.unwrap()),
            PathBuf::from("/games/instances/Game")
        );
        assert_eq!(
            PathBuf::from(game.play_action.unwrap().target),
            PathBuf::from("/games/instances/Game/Game.exe")
        );
    }

    #[test]
    fn slow_and_quick_methods() {
        assert!(is_slow(Method::ScanInstalled) && is_slow(Method::Metadata));
        assert!(!is_slow(Method::Configure) && !is_slow(Method::InstallAction));
        assert_eq!(Runtime::ticks(Duration::from_secs(10)), 102);
        assert!(Runtime::ticks(Duration::ZERO) >= 1);
    }
}
