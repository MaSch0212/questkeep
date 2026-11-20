//! The plugin registry (see `docs/PLUGIN_REGISTRY.md`): fetching and caching its index, picking
//! the versions this Questkeep can run, downloading hash-pinned plugin zips and the flows the
//! plugin browser needs (listing, preview, install, update check).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use questkeep_core::paths::DataPaths;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{PluginError, Result};
use crate::install::{self, RegistryState};
use crate::manager::{FolderRequest, PluginInfo, PluginManager, PluginPermissions, PluginPreview};
use crate::manifest::{API_VERSION, is_valid_id};

/// Where the app looks for the registry index.
pub const DEFAULT_REGISTRY_URL: &str = "https://questkeep.github.io/plugins/index.json";
/// The newest index format this version of Questkeep understands.
pub const SUPPORTED_SCHEMA: u64 = 1;
/// File in the plugins folder with the last good index.
pub const CACHE_FILE: &str = "registry-cache.json";

const TIMEOUT: Duration = Duration::from_secs(15);
/// A whole plugin download may take this long (the connection itself must answer within
/// [`TIMEOUT`]).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_INDEX_BYTES: usize = 5 * 1024 * 1024;
const MAX_ICON_BYTES: usize = 200 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024;
const MAX_REDIRECTS: u32 = 5;
/// Icons are fetched this many at a time.
const ICON_PARALLELISM: usize = 8;
/// Backups of a plugin's data kept per plugin.
const KEEP_BACKUPS: usize = 3;

/// The index address: the default, or in debug builds `QUESTKEEP_REGISTRY_URL` (to try a local
/// registry; `http://localhost` is accepted then).
pub fn registry_url() -> String {
    if cfg!(debug_assertions)
        && let Ok(url) = std::env::var("QUESTKEEP_REGISTRY_URL")
        && !url.trim().is_empty()
    {
        return url.trim().to_string();
    }
    DEFAULT_REGISTRY_URL.to_string()
}

/// Which registry to ask and which Questkeep version decides what is compatible.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    pub url: String,
    /// Major, minor and patch of the running app (a pre-release counts as its release).
    pub app_version: Version,
}

impl RegistryConfig {
    /// The registry of this build ([`registry_url`]) for the app version `app_version`.
    pub fn new(app_version: &str) -> Self {
        Self::with_url(registry_url(), app_version)
    }

    pub fn with_url(url: String, app_version: &str) -> Self {
        let app_version = parse_version_lenient(app_version).unwrap_or_else(|| {
            tracing::warn!(app_version, "the app version is not a version number");
            Version::new(0, 0, 0)
        });
        Self {
            url,
            app_version: Version::new(app_version.major, app_version.minor, app_version.patch),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The index

/// `index.json` as the registry publishes it. Unknown fields are ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryIndex {
    pub schema: u64,
    #[serde(default)]
    pub generated: Option<String>,
    pub plugins: Vec<RegistryPlugin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPlugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Relative to the index address.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub versions: Vec<RegistryVersion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryVersion {
    pub version: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    pub api_version: u32,
    pub min_app: String,
    #[serde(default)]
    pub released: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub yanked: bool,
}

impl RegistryVersion {
    /// The version as a semver number (the index promises one; entries without are dropped
    /// when the index is read).
    pub fn semver(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }
}

impl RegistryIndex {
    /// Reads an index. A newer `schema` than [`SUPPORTED_SCHEMA`] is refused (the app needs an
    /// update); plugins with an invalid id and versions that are no semver number are skipped,
    /// so one bad entry does not hide the rest.
    pub fn parse(text: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
            PluginError::Network(format!(
                "The plugin registry sent something unexpected: {e}."
            ))
        })?;
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                PluginError::Network("The plugin registry sent something unexpected.".into())
            })?;
        if schema > SUPPORTED_SCHEMA {
            return Err(PluginError::Invalid(
                "The plugin registry uses a newer format than this version of Questkeep \
                 understands. Update Questkeep to browse plugins."
                    .into(),
            ));
        }
        let generated = value
            .get("generated")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let mut plugins = Vec::new();
        for entry in value
            .get("plugins")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut plugin: RegistryPlugin = match serde_json::from_value(entry.clone()) {
                Ok(plugin) => plugin,
                Err(err) => {
                    tracing::warn!(%err, "skipping a registry entry that is not valid");
                    continue;
                }
            };
            if !is_valid_id(&plugin.id) {
                tracing::warn!(id = %plugin.id, "skipping a registry plugin with an invalid id");
                continue;
            }
            plugin.versions.retain(|v| {
                let ok = v.semver().is_some();
                if !ok {
                    tracing::warn!(plugin = %plugin.id, version = %v.version, "skipping a registry version that is not semver");
                }
                ok
            });
            plugin.versions.sort_by_key(|v| v.semver());
            plugins.push(plugin);
        }
        Ok(Self {
            schema,
            generated,
            plugins,
        })
    }

    pub fn plugin(&self, id: &str) -> Option<&RegistryPlugin> {
        self.plugins.iter().find(|p| p.id == id)
    }
}

impl RegistryPlugin {
    pub fn version(&self, version: &str) -> Option<&RegistryVersion> {
        self.versions.iter().find(|v| v.version == version)
    }

    /// The newest version this app can run that is not withdrawn.
    pub fn latest_compatible(&self, app: &Version) -> Option<&RegistryVersion> {
        self.versions
            .iter()
            .filter(|v| compatible(v, app, false))
            .max_by_key(|v| v.semver())
    }

    /// Why nothing of the plugin can be installed (`None` when something can).
    pub fn incompatible_reason(&self, app: &Version) -> Option<String> {
        if self.latest_compatible(app).is_some() {
            return None;
        }
        let live: Vec<&RegistryVersion> = self.versions.iter().filter(|v| !v.yanked).collect();
        if live.is_empty() {
            return Some("All versions were withdrawn by the author.".into());
        }
        let needed = live
            .iter()
            .filter(|v| v.api_version == API_VERSION)
            .filter_map(|v| parse_version_lenient(&v.min_app))
            .min();
        if let Some(needed) = needed {
            return Some(format!("Needs Questkeep {needed} or newer."));
        }
        if live.iter().any(|v| v.api_version > API_VERSION) {
            Some("Made for a newer plugin API: update Questkeep.".into())
        } else {
            Some("Made for an older plugin API that is no longer supported.".into())
        }
    }
}

/// `1`, `1.2` and `v1.2.3-beta` are accepted (for `minApp` and the app's own version).
pub fn parse_version_lenient(text: &str) -> Option<Version> {
    let text = text.trim().trim_start_matches('v');
    if let Ok(version) = Version::parse(text) {
        return Some(version);
    }
    let split = text.find(['-', '+']).unwrap_or(text.len());
    let (core, rest) = text.split_at(split);
    let parts = core.split('.').count();
    if (1..3).contains(&parts) {
        Version::parse(&format!("{core}{}{rest}", ".0".repeat(3 - parts))).ok()
    } else {
        None
    }
}

/// Whether `version` can run in an app of version `app`: the same plugin API and an app that is
/// new enough. A withdrawn version only counts with `include_yanked`.
pub fn compatible(version: &RegistryVersion, app: &Version, include_yanked: bool) -> bool {
    (include_yanked || !version.yanked)
        && version.api_version == API_VERSION
        && parse_version_lenient(&version.min_app).is_some_and(|min| min <= *app)
}

// ---------------------------------------------------------------------------------------------
// Fetching

/// Whether `url` may be requested: https, or (debug builds) http to this PC. Returns whether it
/// is the plain-http kind.
fn check_url(url: &str) -> Result<bool> {
    let parsed = url::Url::parse(url)
        .map_err(|_| PluginError::Invalid(format!("\"{url}\" is not a valid web address.")))?;
    match parsed.scheme() {
        "https" if parsed.host_str().is_some() => Ok(false),
        "http"
            if cfg!(debug_assertions)
                && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) =>
        {
            Ok(true)
        }
        _ => Err(PluginError::Invalid(format!(
            "\"{url}\" is refused: only https addresses are used for plugins."
        ))),
    }
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| url.to_string())
}

fn get(url: &str, timeout: Duration) -> Result<ureq::http::Response<ureq::Body>> {
    let plain = check_url(url)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .timeout_connect(Some(TIMEOUT))
        .timeout_recv_response(Some(TIMEOUT))
        .user_agent(concat!("Questkeep/", env!("CARGO_PKG_VERSION")))
        .https_only(!plain)
        .max_redirects(MAX_REDIRECTS)
        .build()
        .into();
    agent.get(url).call().map_err(|err| {
        let host = host_of(url);
        PluginError::Network(match err {
            ureq::Error::StatusCode(code) => format!("{host} answered with the error {code}."),
            ureq::Error::TooManyRedirects => format!("{host} redirected too many times."),
            other => format!("Could not reach {host}: {other}."),
        })
    })
}

fn read_capped(body: &mut ureq::Body, limit: usize, what: &str) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    body.as_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| PluginError::Network(format!("Could not read {what}: {e}.")))?;
    if data.len() > limit {
        return Err(PluginError::Network(format!(
            "{what} is larger than {} KB.",
            limit / 1024
        )));
    }
    Ok(data)
}

/// Downloads and reads the registry index.
pub fn fetch_index(url: &str) -> Result<RegistryIndex> {
    let fail = |e: PluginError| match e {
        PluginError::Network(msg) => {
            PluginError::Network(format!("The plugin registry could not be loaded: {msg}"))
        }
        other => other,
    };
    let mut response = get(url, TIMEOUT).map_err(fail)?;
    let bytes =
        read_capped(response.body_mut(), MAX_INDEX_BYTES, "The plugin registry").map_err(fail)?;
    let text = String::from_utf8_lossy(&bytes);
    RegistryIndex::parse(text.strip_prefix('\u{feff}').unwrap_or(&text))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).as_slice())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA-256 of a file, as lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| PluginError::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| PluginError::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(hasher.finalize().as_slice()))
}

/// Deletes a file when dropped, unless it was kept.
struct TempFile {
    path: PathBuf,
    keep: bool,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Downloads `url` to `dest`: https only (and the redirects it makes), at most 100 MB, and only
/// kept when its SHA-256 is `expected_sha256`. Nothing is left at `dest` otherwise.
pub fn download(url: &str, expected_sha256: &str, expected_size: u64, dest: &Path) -> Result<()> {
    download_capped(
        url,
        expected_sha256,
        expected_size,
        dest,
        MAX_DOWNLOAD_BYTES,
    )
}

fn download_capped(
    url: &str,
    expected_sha256: &str,
    expected_size: u64,
    dest: &Path,
    cap: u64,
) -> Result<()> {
    let expected = expected_sha256.trim().to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(PluginError::Invalid(
            "The registry lists no valid checksum for this plugin file, so it is not downloaded."
                .into(),
        ));
    }
    check_url(url)?;
    if expected_size > cap {
        return Err(PluginError::Invalid(format!(
            "The plugin file is larger than {} MB, which is not accepted.",
            cap / (1024 * 1024)
        )));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
    }
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp_path = dest.with_file_name(format!("{name}.{}.part", install::unique()));
    let mut response = get(url, DOWNLOAD_TIMEOUT)?;
    let mut temp = TempFile {
        path: temp_path,
        keep: false,
    };
    {
        let mut file =
            std::fs::File::create(&temp.path).map_err(|e| PluginError::io(&temp.path, e))?;
        let mut reader = response.body_mut().as_reader();
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let read = reader.read(&mut buffer).map_err(|e| {
                PluginError::Network(format!(
                    "The download from {} broke off: {e}.",
                    host_of(url)
                ))
            })?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > cap {
                return Err(PluginError::Invalid(format!(
                    "The plugin file is larger than {} MB, which is not accepted.",
                    cap / (1024 * 1024)
                )));
            }
            hasher.update(&buffer[..read]);
            file.write_all(&buffer[..read])
                .map_err(|e| PluginError::io(&temp.path, e))?;
        }
        file.flush().map_err(|e| PluginError::io(&temp.path, e))?;
        let actual = hex(hasher.finalize().as_slice());
        if actual != expected {
            tracing::warn!(
                url,
                expected,
                actual,
                "plugin download does not match its checksum"
            );
            return Err(PluginError::Invalid(
                "The downloaded plugin file does not match the checksum the registry lists, so \
                 it was not used."
                    .into(),
            ));
        }
    }
    install::rename(&temp.path, dest)?;
    temp.keep = true;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Cache

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryCache {
    /// The address the index came from (a cache of another registry is not used).
    url: String,
    fetched_at: String,
    index: RegistryIndex,
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!("{name}.{}.tmp", install::unique()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
    }
    std::fs::write(&temp, bytes).map_err(|e| PluginError::io(&temp, e))?;
    install::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

fn read_cache(plugins_dir: &Path, url: &str) -> Option<RegistryCache> {
    let text = std::fs::read_to_string(plugins_dir.join(CACHE_FILE)).ok()?;
    let cache: RegistryCache = serde_json::from_str(&text).ok()?;
    (cache.url == url && cache.index.schema <= SUPPORTED_SCHEMA).then_some(cache)
}

fn write_cache(plugins_dir: &Path, cache: &RegistryCache) -> Result<()> {
    let text = serde_json::to_vec(cache)
        .map_err(|e| PluginError::Invalid(format!("Could not save the registry: {e}.")))?;
    write_atomic(&plugins_dir.join(CACHE_FILE), &text)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The index to look versions up in: the cached one, or a fresh one when there is no cache yet.
fn index_for_lookup(config: &RegistryConfig, plugins_dir: &Path) -> Result<RegistryIndex> {
    if let Some(cache) = read_cache(plugins_dir, &config.url) {
        return Ok(cache.index);
    }
    let index = fetch_index(&config.url)?;
    let cache = RegistryCache {
        url: config.url.clone(),
        fetched_at: now(),
        index: index.clone(),
    };
    if let Err(err) = write_cache(plugins_dir, &cache) {
        tracing::warn!(%err, "could not cache the plugin registry");
    }
    Ok(index)
}

// ---------------------------------------------------------------------------------------------
// Icons

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IconCache {
    url: String,
    generated: Option<String>,
    data_url: String,
}

fn icon_cache_file(plugins_dir: &Path, id: &str) -> PathBuf {
    plugins_dir.join(format!("registry-icon.{id}.json"))
}

/// The absolute address of a plugin's icon (`None` without one or when it is not allowed).
pub fn icon_url(index_url: &str, plugin: &RegistryPlugin) -> Option<String> {
    let icon = plugin.icon.as_deref().filter(|i| !i.trim().is_empty())?;
    let joined = url::Url::parse(index_url)
        .ok()?
        .join(icon)
        .ok()?
        .to_string();
    check_url(&joined).ok()?;
    Some(joined)
}

fn fetch_icon(url: &str) -> Option<String> {
    let mut response = get(url, TIMEOUT).ok()?;
    let bytes = read_capped(response.body_mut(), MAX_ICON_BYTES, "The icon").ok()?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).contains("<svg") {
        "image/svg+xml"
    } else {
        return None;
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// The icons of the index as `data:` URLs (the page cannot fetch them across origins), from the
/// icon cache when it is still good and from the network otherwise. Failures are ignored.
fn resolve_icons(
    plugins_dir: &Path,
    index_url: &str,
    index: &RegistryIndex,
    online: bool,
) -> BTreeMap<String, String> {
    let mut icons = BTreeMap::new();
    let mut wanted: Vec<(&str, String)> = Vec::new();
    for plugin in &index.plugins {
        let Some(url) = icon_url(index_url, plugin) else {
            continue;
        };
        let cached: Option<IconCache> =
            std::fs::read_to_string(icon_cache_file(plugins_dir, &plugin.id))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .filter(|c: &IconCache| c.url == url);
        match cached {
            Some(c) if !online || (index.generated.is_some() && c.generated == index.generated) => {
                icons.insert(plugin.id.clone(), c.data_url);
            }
            other => {
                if let Some(c) = other {
                    // Still better than nothing if the fetch below fails.
                    icons.insert(plugin.id.clone(), c.data_url);
                }
                if online {
                    wanted.push((&plugin.id, url));
                }
            }
        }
    }
    for chunk in wanted.chunks(ICON_PARALLELISM) {
        let fetched: Vec<Option<String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|(_, url)| scope.spawn(move || fetch_icon(url)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().ok().flatten())
                .collect()
        });
        for ((id, url), data_url) in chunk.iter().zip(fetched) {
            let Some(data_url) = data_url else {
                continue;
            };
            let cache = IconCache {
                url: url.clone(),
                generated: index.generated.clone(),
                data_url: data_url.clone(),
            };
            if let Ok(bytes) = serde_json::to_vec(&cache)
                && let Err(err) = write_atomic(&icon_cache_file(plugins_dir, id), &bytes)
            {
                tracing::debug!(%err, "could not cache a registry icon");
            }
            icons.insert((*id).to_string(), data_url);
        }
    }
    icons
}

// ---------------------------------------------------------------------------------------------
// What the UI gets

/// The registry for the plugin browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RegistryListing {
    /// The registry could not be reached: this is the last copy that was loaded.
    pub offline: bool,
    /// When the shown copy was loaded (RFC 3339, UTC).
    pub fetched_at: Option<String>,
    pub plugins: Vec<RegistryPluginView>,
}

/// One registry plugin with what the app worked out about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RegistryPluginView {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub author: String,
    pub repository: String,
    pub homepage: Option<String>,
    pub license: String,
    pub tags: Vec<String>,
    /// `store`, `extension`, `theme`.
    pub kinds: Vec<String>,
    /// The icon's absolute address.
    pub icon_url: Option<String>,
    /// The icon as a `data:` URL (up to 200 KB); use this one in the page.
    pub icon_data: Option<String>,
    /// Versions that can be installed here (compatible, not withdrawn), oldest first.
    pub compatible_versions: Vec<String>,
    /// The newest of them.
    pub latest: Option<String>,
    pub installed: Option<RegistryInstalled>,
    /// Installed from the registry, not pinned, and `latest` is newer.
    pub update_available: bool,
    /// Why no version can be installed (only set when `latest` is null).
    pub incompatible_reason: Option<String>,
    /// Every version, oldest first (for "show all versions").
    pub versions: Vec<RegistryVersionView>,
}

/// The installed plugin that has the id of a registry plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RegistryInstalled {
    pub version: String,
    pub pinned: bool,
    /// `false`: installed from a folder or file, which the registry never touches.
    pub from_registry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RegistryVersionView {
    pub version: String,
    #[ts(type = "number")]
    pub size: u64,
    pub released: Option<String>,
    pub notes: Option<String>,
    pub api_version: u32,
    pub min_app: String,
    /// Withdrawn by the author: shown only for information, never installable.
    pub yanked: bool,
    /// Runs in this Questkeep (right plugin API, app new enough); ignores `yanked`.
    pub compatible: bool,
}

/// An installed registry plugin that has a newer version available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PluginUpdate {
    pub id: String,
    pub name: String,
    pub installed_version: String,
    pub latest_version: String,
    /// The release notes of `latest_version`, when the registry has some.
    pub notes: Option<String>,
}

/// The preview of a registry version: the usual install preview plus what the registry says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RegistryPluginPreview {
    pub preview: PluginPreview,
    pub released: Option<String>,
    pub notes: Option<String>,
    #[ts(type = "number")]
    pub size: u64,
    /// Installing this version would pin it (it is not the newest compatible one): the plugin
    /// then gets no update offers.
    pub pinned: bool,
    /// An installed version is newer: the plugin's data is backed up before the swap.
    pub downgrade: bool,
    /// What this version asks for beyond the installed one (empty for a plugin that is not
    /// installed).
    pub changes: PluginChanges,
}

/// What matters for trusting a plugin: the permissions its stores get, how many stores it brings
/// and whether it runs an extension.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginShape {
    pub permissions: PluginPermissions,
    pub stores: u32,
    pub has_extension: bool,
}

impl From<&PluginPreview> for PluginShape {
    fn from(preview: &PluginPreview) -> Self {
        Self {
            permissions: preview.permissions.clone(),
            stores: preview.stores,
            has_extension: preview.has_extension,
        }
    }
}

/// What a new version asks for that the installed one did not (an update that has none can be
/// installed without asking again; see "Update all").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PluginChanges {
    /// Web hosts the stores may now reach.
    pub new_http: Vec<String>,
    /// Folders the stores may now read (as written in the manifest).
    pub new_read: Vec<String>,
    /// Folders the plugin now asks the user for.
    pub new_folders: Vec<FolderRequest>,
    /// The new version runs an extension (JavaScript in the app) the installed one did not.
    pub adds_extension: bool,
    /// The new version brings more stores than the installed one.
    pub adds_store: bool,
    /// Any of the above: the user should look before this version is installed.
    pub needs_review: bool,
}

impl PluginChanges {
    /// What `new` asks for beyond `old`. Hosts and folders are compared ignoring case.
    pub fn between(old: &PluginShape, new: &PluginShape) -> Self {
        fn added<T: Clone>(old: &[T], new: &[T], key: impl Fn(&T) -> String) -> Vec<T> {
            let known: Vec<String> = old.iter().map(&key).collect();
            new.iter()
                .filter(|item| !known.contains(&key(item)))
                .cloned()
                .collect()
        }
        let lower = |text: &String| text.trim().to_lowercase();
        let new_http = added(&old.permissions.http, &new.permissions.http, lower);
        let new_read = added(&old.permissions.read, &new.permissions.read, lower);
        let new_folders = added(&old.permissions.folders, &new.permissions.folders, |f| {
            f.key.clone()
        });
        let adds_extension = new.has_extension && !old.has_extension;
        let adds_store = new.stores > old.stores;
        let needs_review = !new_http.is_empty()
            || !new_read.is_empty()
            || !new_folders.is_empty()
            || adds_extension
            || adds_store;
        Self {
            new_http,
            new_read,
            new_folders,
            adds_extension,
            adds_store,
            needs_review,
        }
    }
}

/// What the registry needs to know about an installed plugin.
#[derive(Debug, Clone)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub registry: Option<RegistryState>,
}

impl From<&PluginInfo> for Installed {
    fn from(info: &PluginInfo) -> Self {
        Self {
            id: info.id.clone(),
            version: info.version.clone(),
            registry: info.registry.clone(),
        }
    }
}

/// The newer version of an installed registry plugin, if there is one to offer.
fn update_for<'a>(
    plugin: &'a RegistryPlugin,
    installed: &Installed,
    app: &Version,
) -> Option<&'a RegistryVersion> {
    let registry = installed.registry.as_ref().filter(|r| !r.pinned)?;
    let _ = registry;
    let current = Version::parse(&installed.version).ok()?;
    let latest = plugin.latest_compatible(app)?;
    (latest.semver()? > current).then_some(latest)
}

/// The plugin browser's view of the index.
pub fn build_listing(
    index: &RegistryIndex,
    offline: bool,
    fetched_at: Option<String>,
    installed: &[Installed],
    app: &Version,
    index_url: &str,
    icons: &BTreeMap<String, String>,
) -> RegistryListing {
    let plugins = index
        .plugins
        .iter()
        .map(|plugin| {
            let latest = plugin.latest_compatible(app);
            let local = installed.iter().find(|i| i.id == plugin.id);
            RegistryPluginView {
                id: plugin.id.clone(),
                name: plugin.name.clone(),
                summary: plugin.summary.clone(),
                description: plugin.description.clone(),
                author: plugin.author.clone(),
                repository: plugin.repository.clone(),
                homepage: plugin.homepage.clone(),
                license: plugin.license.clone(),
                tags: plugin.tags.clone(),
                kinds: plugin.kinds.clone(),
                icon_url: icon_url(index_url, plugin),
                icon_data: icons.get(&plugin.id).cloned(),
                compatible_versions: plugin
                    .versions
                    .iter()
                    .filter(|v| compatible(v, app, false))
                    .map(|v| v.version.clone())
                    .collect(),
                latest: latest.map(|v| v.version.clone()),
                installed: local.map(|i| RegistryInstalled {
                    version: i.version.clone(),
                    pinned: i.registry.as_ref().is_some_and(|r| r.pinned),
                    from_registry: i.registry.is_some(),
                }),
                update_available: local.is_some_and(|i| update_for(plugin, i, app).is_some()),
                incompatible_reason: plugin.incompatible_reason(app),
                versions: plugin
                    .versions
                    .iter()
                    .map(|v| RegistryVersionView {
                        version: v.version.clone(),
                        size: v.size,
                        released: v.released.clone(),
                        notes: v.notes.clone(),
                        api_version: v.api_version,
                        min_app: v.min_app.clone(),
                        yanked: v.yanked,
                        compatible: compatible(v, app, true),
                    })
                    .collect(),
            }
        })
        .collect();
    RegistryListing {
        offline,
        fetched_at,
        plugins,
    }
}

/// Downloads the index and keeps it as the cache (which [`cached_updates`] reads). Returns the
/// index and when it was loaded. A cache that cannot be written is only logged.
pub fn refresh_cache(
    config: &RegistryConfig,
    plugins_dir: &Path,
) -> Result<(RegistryIndex, String)> {
    let index = fetch_index(&config.url)?;
    let fetched_at = now();
    let cache = RegistryCache {
        url: config.url.clone(),
        fetched_at: fetched_at.clone(),
        index: index.clone(),
    };
    if let Err(err) = write_cache(plugins_dir, &cache) {
        tracing::warn!(%err, "could not cache the plugin registry");
    }
    Ok((index, fetched_at))
}

/// Loads the registry (the network first; the cache when that fails and one exists) and
/// describes it for the browser. A registry that is too new for this app is an error either way.
pub fn fetch_listing(
    config: &RegistryConfig,
    plugins_dir: &Path,
    installed: &[Installed],
) -> Result<RegistryListing> {
    let cached = read_cache(plugins_dir, &config.url);
    let (index, fetched_at, offline) = match refresh_cache(config, plugins_dir) {
        Ok((index, fetched_at)) => (index, Some(fetched_at), false),
        Err(PluginError::Network(message)) => match cached {
            Some(cache) => {
                tracing::warn!(%message, "plugin registry not reachable: using the cached copy");
                (cache.index, Some(cache.fetched_at), true)
            }
            None => return Err(PluginError::Network(message)),
        },
        Err(other) => return Err(other),
    };
    let icons = resolve_icons(plugins_dir, &config.url, &index, !offline);
    Ok(build_listing(
        &index,
        offline,
        fetched_at,
        installed,
        &config.app_version,
        &config.url,
        &icons,
    ))
}

/// Updates of the installed registry plugins according to the cached index (no network).
pub fn cached_updates(
    config: &RegistryConfig,
    plugins_dir: &Path,
    installed: &[Installed],
    names: &BTreeMap<String, String>,
) -> Vec<PluginUpdate> {
    let Some(cache) = read_cache(plugins_dir, &config.url) else {
        return Vec::new();
    };
    updates(&cache.index, installed, &config.app_version, names)
}

fn updates(
    index: &RegistryIndex,
    installed: &[Installed],
    app: &Version,
    names: &BTreeMap<String, String>,
) -> Vec<PluginUpdate> {
    installed
        .iter()
        .filter_map(|i| {
            let plugin = index.plugin(&i.id)?;
            let latest = update_for(plugin, i, app)?;
            Some(PluginUpdate {
                id: i.id.clone(),
                name: names
                    .get(&i.id)
                    .cloned()
                    .unwrap_or_else(|| plugin.name.clone()),
                installed_version: i.version.clone(),
                latest_version: latest.version.clone(),
                notes: latest.notes.clone().filter(|n| !n.trim().is_empty()),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The zip cache and the data backups

/// Where the zip of a registry version is kept.
pub fn cached_zip_path(paths: &DataPaths, id: &str, version: &str) -> PathBuf {
    paths
        .plugin_cache_dir()
        .join(id)
        .join(format!("{version}.zip"))
}

/// The zip of `version` in the cache, downloaded (and checked against the index's hash) when it
/// is missing or does not match.
fn ensure_cached_zip(paths: &DataPaths, id: &str, version: &RegistryVersion) -> Result<PathBuf> {
    let zip = cached_zip_path(paths, id, &version.version);
    let expected = version.sha256.trim().to_ascii_lowercase();
    if zip.is_file() && sha256_file(&zip).is_ok_and(|actual| actual == expected) {
        return Ok(zip);
    }
    download(&version.url, &version.sha256, version.size, &zip)?;
    Ok(zip)
}

/// Deletes the other cached zips of a plugin (what is installed is `keep`).
pub(crate) fn prune_cache(paths: &DataPaths, id: &str, keep: &Path) {
    let dir = paths.plugin_cache_dir().join(id);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path() != keep {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Removes everything cached for a plugin.
pub(crate) fn remove_cache(paths: &DataPaths, id: &str) -> Result<()> {
    install::remove_dir(&paths.plugin_cache_dir().join(id))
}

/// Removes the data backups of a plugin.
pub(crate) fn remove_backups(paths: &DataPaths, id: &str) -> Result<()> {
    install::remove_dir(&paths.plugin_data_backup_dir().join(id))
}

fn folder_name(version: &str) -> String {
    let name: String = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.starts_with('.') {
        format!("v{name}")
    } else {
        name
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
        // Links are not followed.
    }
    Ok(())
}

/// Copies the plugin's data folder to `<data>/plugin-data-backup/<id>/<old version>/` (keeping
/// the newest [`KEEP_BACKUPS`]). Nothing to do (`Ok(None)`) without data. A failure is an error:
/// the caller must not replace the plugin then.
pub(crate) fn backup_plugin_data(
    paths: &DataPaths,
    id: &str,
    old_version: &str,
) -> Result<Option<PathBuf>> {
    let data = paths.plugin_data_dir().join(id);
    let has_data = match std::fs::read_dir(&data) {
        Ok(mut entries) => entries.next().is_some(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(backup_failed(id, &err)),
    };
    if !has_data {
        return Ok(None);
    }
    let root = paths.plugin_data_backup_dir().join(id);
    std::fs::create_dir_all(&root).map_err(|e| backup_failed(id, &e))?;
    // Leftovers of a backup that was cut short.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(".tmp-") {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let target = root.join(folder_name(old_version));
    let temp = root.join(format!(".tmp-{}", install::unique()));
    if let Err(err) = copy_tree(&data, &temp) {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(backup_failed(id, &err));
    }
    let placed = install::remove_dir(&target).and_then(|()| install::rename(&temp, &target));
    if let Err(err) = placed {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(PluginError::Invalid(format!(
            "Could not back up the data of the plugin \"{id}\" before replacing it ({err}). \
             Nothing was changed."
        )));
    }
    rotate_backups(&root);
    Ok(Some(target))
}

fn backup_failed(id: &str, err: &dyn std::fmt::Display) -> PluginError {
    PluginError::Invalid(format!(
        "Could not back up the data of the plugin \"{id}\" before replacing it ({err}). Nothing \
         was changed."
    ))
}

/// Deletes all but the newest [`KEEP_BACKUPS`] backups in `root` (a plugin's backup folder).
fn rotate_backups(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut backups: Vec<(std::time::SystemTime, Option<Version>, String, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let modified = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (modified, Version::parse(&name).ok(), name, e.path())
        })
        .collect();
    // Newest first; the version (then the name) decides between backups made in the same instant.
    backups.sort_by(|a, b| (b.0, &b.1, &b.2).cmp(&(a.0, &a.1, &a.2)));
    for (_, _, name, path) in backups.into_iter().skip(KEEP_BACKUPS) {
        if let Err(err) = install::remove_dir(&path) {
            tracing::warn!(backup = %name, %err, "could not delete an old plugin data backup");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The flows (on the plugin manager)

impl PluginManager {
    fn installed_refs(&self) -> Vec<Installed> {
        self.list().iter().map(Installed::from).collect()
    }

    /// Loads the registry for the plugin browser and notes what is installed and what can be
    /// updated. Also refreshes the cache that [`plugin_updates`](Self::plugin_updates) reads.
    pub fn registry_listing(&self, config: &RegistryConfig) -> Result<RegistryListing> {
        fetch_listing(config, &self.paths().plugins_dir(), &self.installed_refs())
    }

    /// Installed registry plugins with a newer version, from the cached index only.
    pub fn plugin_updates(&self, config: &RegistryConfig) -> Vec<PluginUpdate> {
        let list = self.list();
        let names = list
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        let installed: Vec<Installed> = list.iter().map(Installed::from).collect();
        cached_updates(config, &self.paths().plugins_dir(), &installed, &names)
    }

    /// Finds `version` of `id` in the index and checks that it may be installed.
    fn registry_version(
        &self,
        config: &RegistryConfig,
        id: &str,
        version: &str,
    ) -> Result<(RegistryVersion, bool)> {
        let index = index_for_lookup(config, &self.paths().plugins_dir())?;
        let plugin = index
            .plugin(id)
            .ok_or_else(|| PluginError::Invalid(format!("The registry has no plugin \"{id}\".")))?;
        let entry = plugin.version(version).ok_or_else(|| {
            PluginError::Invalid(format!(
                "The registry has no version {version} of {}.",
                plugin.name
            ))
        })?;
        let installed = self.list().into_iter().find(|p| p.id == id);
        let same_as_installed = installed.is_some_and(|p| p.version == entry.version);
        if !same_as_installed {
            if entry.yanked {
                return Err(PluginError::Invalid(format!(
                    "Version {version} of {} was withdrawn by its author, so it can't be \
                     installed (also not as a downgrade).",
                    plugin.name
                )));
            }
            if !compatible(entry, &config.app_version, false) {
                return Err(PluginError::Invalid(if entry.api_version != API_VERSION {
                    format!(
                        "Version {version} of {} was made for a different plugin API than this \
                         Questkeep supports.",
                        plugin.name
                    )
                } else {
                    format!(
                        "Version {version} of {} needs Questkeep {} or newer (this is {}).",
                        plugin.name, entry.min_app, config.app_version
                    )
                }));
            }
        }
        let pinned = plugin
            .latest_compatible(&config.app_version)
            .and_then(RegistryVersion::semver)
            != entry.semver();
        Ok((entry.clone(), pinned))
    }

    /// Downloads and checks a registry version into the zip cache and shows what installing it
    /// would do (the usual install preview and the registry's facts). Installs nothing.
    pub fn preview_registry_plugin(
        &self,
        config: &RegistryConfig,
        id: &str,
        version: &str,
    ) -> Result<RegistryPluginPreview> {
        let _op = self.registry_ops();
        let (entry, pinned) = self.registry_version(config, id, version)?;
        let zip = ensure_cached_zip(self.paths(), id, &entry)?;
        let preview = self.preview(&zip)?;
        if preview.id != id || preview.version != entry.version {
            let _ = std::fs::remove_file(&zip);
            return Err(mismatch(id, &entry.version, &preview.id, &preview.version));
        }
        let downgrade = match (&preview.installed_version, entry.semver()) {
            (Some(installed), Some(new)) => {
                Version::parse(installed).is_ok_and(|installed| installed > new)
            }
            _ => false,
        };
        let changes = self
            .installed_shape(id)
            .map(|old| PluginChanges::between(&old, &PluginShape::from(&preview)))
            .unwrap_or_default();
        Ok(RegistryPluginPreview {
            preview,
            released: entry.released.clone(),
            notes: entry.notes.clone(),
            size: entry.size,
            pinned,
            downgrade,
            changes,
        })
    }

    /// Installs (or updates, or switches to another version of) a registry plugin: the cached
    /// zip is checked against the index again (downloaded when missing), then installed,
    /// enabled and loaded like any plugin.
    pub fn install_registry_plugin(
        &self,
        config: &RegistryConfig,
        id: &str,
        version: &str,
    ) -> Result<PluginInfo> {
        self.install_registry_plugin_with_folders(config, id, version, &BTreeMap::new())
    }

    /// [`install_registry_plugin`](Self::install_registry_plugin) with the folders the user
    /// chose in the install dialog (see [`PluginManager::install_with_folders`]).
    pub fn install_registry_plugin_with_folders(
        &self,
        config: &RegistryConfig,
        id: &str,
        version: &str,
        folders: &BTreeMap<String, String>,
    ) -> Result<PluginInfo> {
        let _op = self.registry_ops();
        let (entry, pinned) = self.registry_version(config, id, version)?;
        let zip = ensure_cached_zip(self.paths(), id, &entry)?;
        self.install_registry_with_folders(id, &entry.version, &zip, pinned, folders)
    }
}

/// The error for a zip whose `plugin.json` is not what the registry says.
pub(crate) fn mismatch(
    id: &str,
    version: &str,
    found_id: &str,
    found_version: &str,
) -> PluginError {
    if id != found_id {
        PluginError::Invalid(format!(
            "The downloaded file is the plugin \"{found_id}\" and not \"{id}\" as the registry \
             says. Nothing was installed."
        ))
    } else {
        PluginError::Invalid(format!(
            "The downloaded file is version {found_version} of \"{id}\", but the registry says \
             {version}. Nothing was installed."
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    use questkeep_core::events::{CoreEvent, EventBus};
    use questkeep_core::notifications::{
        NewNotification, NotificationAction, NotificationSource, Notifications,
    };
    use questkeep_core::providers::ProviderRegistry;

    use super::*;
    use crate::install::testing::write_zip;
    use crate::update_check::CheckKind;

    fn version_json(version: &str, api: u32, min_app: &str, yanked: bool) -> serde_json::Value {
        serde_json::json!({
            "version": version, "url": format!("http://127.0.0.1:1/{version}.zip"),
            "sha256": "0".repeat(64), "size": 10, "apiVersion": api, "minApp": min_app,
            "released": "2026-11-20", "notes": null, "yanked": yanked,
        })
    }

    fn index_json(plugins: serde_json::Value) -> String {
        serde_json::json!({ "schema": 1, "generated": "2026-11-20T12:00:00Z", "plugins": plugins })
            .to_string()
    }

    fn plugin_json(id: &str, versions: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "id": id, "name": format!("Plugin {id}"), "summary": "s", "description": "d",
            "author": "a", "repository": "https://github.com/x/y", "homepage": null,
            "license": "MIT", "tags": [], "kinds": ["theme"], "icon": "icons/x.svg",
            "versions": versions, "somethingNew": { "later": true },
        })
    }

    fn app(version: &str) -> Version {
        parse_version_lenient(version).unwrap()
    }

    /// Answers every request on a free local port with `handler(path)` (raw HTTP response).
    fn serve(handler: impl Fn(&str) -> Vec<u8> + Send + Sync + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        serve_on(listener, handler);
        format!("http://127.0.0.1:{port}")
    }

    fn serve_on(listener: TcpListener, handler: impl Fn(&str) -> Vec<u8> + Send + Sync + 'static) {
        let handler = Arc::new(handler);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let handler = handler.clone();
                std::thread::spawn(move || {
                    let mut request = Vec::new();
                    let mut byte = [0u8; 1];
                    while !request.ends_with(b"\r\n\r\n") {
                        match stream.read(&mut byte) {
                            Ok(1) => request.push(byte[0]),
                            _ => return,
                        }
                    }
                    let text = String::from_utf8_lossy(&request).into_owned();
                    let path = text.split_whitespace().nth(1).unwrap_or("/").to_string();
                    // The client may hang up early (a download that is too big).
                    let _ = stream.write_all(&handler(&path));
                });
            }
        });
    }

    fn ok(body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    fn status(code: &str, extra: &str) -> Vec<u8> {
        format!("HTTP/1.1 {code}\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n")
            .into_bytes()
    }

    #[test]
    fn parses_the_index_and_ignores_what_it_does_not_know() {
        let text = index_json(serde_json::json!([
            plugin_json("good", vec![version_json("1.0.0", 1, "0.1.0", false)]),
            plugin_json("Bad Id", vec![]),
            { "id": "broken" },
            plugin_json("mixed", vec![
                version_json("2.0.0", 1, "0.1.0", false),
                version_json("not-a-version", 1, "0.1.0", false),
                version_json("1.0.0", 1, "0.1.0", false),
            ]),
        ]));
        let index = RegistryIndex::parse(&text).unwrap();
        assert_eq!(index.schema, 1);
        let ids: Vec<&str> = index.plugins.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["good", "mixed"]);
        // Versions are ordered ascending whatever the file says.
        let versions: Vec<&str> = index.plugins[1]
            .versions
            .iter()
            .map(|v| v.version.as_str())
            .collect();
        assert_eq!(versions, ["1.0.0", "2.0.0"]);
        assert_eq!(index.plugins[0].homepage, None);

        // Newer format: refused with a readable message.
        let err = RegistryIndex::parse(r#"{"schema":2,"plugins":[]}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Update Questkeep"), "{err}");
        assert!(RegistryIndex::parse("not json").is_err());
        assert!(RegistryIndex::parse(r#"{"plugins":[]}"#).is_err());
    }

    #[test]
    fn versions_compare_as_semver_with_pre_releases() {
        let v = |s: &str| parse_version_lenient(s).unwrap();
        assert!(v("1.0.0-beta.2") < v("1.0.0-beta.10"));
        assert!(v("1.0.0-beta.10") < v("1.0.0"));
        assert!(v("0.9.0") < v("1.0.0-alpha"));
        assert_eq!(v("0.7"), Version::new(0, 7, 0));
        assert_eq!(v("1"), Version::new(1, 0, 0));
        assert_eq!(v("v1.2.3-rc.1").pre.as_str(), "rc.1");
        assert!(parse_version_lenient("one").is_none());
        assert!(parse_version_lenient("1.2.3.4").is_none());

        let text = index_json(serde_json::json!([plugin_json(
            "pre",
            vec![
                version_json("1.0.0-beta.2", 1, "0.1.0", false),
                version_json("1.0.0", 1, "0.1.0", false),
                version_json("1.0.0-beta.10", 1, "0.1.0", false),
            ]
        )]));
        let index = RegistryIndex::parse(&text).unwrap();
        let latest = index.plugins[0].latest_compatible(&app("0.6.0")).unwrap();
        assert_eq!(latest.version, "1.0.0");
        // The pre-release is the newest when it is all there is.
        let text = index_json(serde_json::json!([plugin_json(
            "pre",
            vec![
                version_json("1.0.0-beta.2", 1, "0.1.0", false),
                version_json("1.0.0-beta.10", 1, "0.1.0", false),
            ]
        )]));
        let index = RegistryIndex::parse(&text).unwrap();
        assert_eq!(
            index.plugins[0]
                .latest_compatible(&app("0.6.0"))
                .unwrap()
                .version,
            "1.0.0-beta.10"
        );
    }

    #[test]
    fn compatibility_and_yanked_versions() {
        let text = index_json(serde_json::json!([
            plugin_json(
                "pk",
                vec![
                    version_json("1.0.0", 1, "0.5.0", false),
                    version_json("1.1.0", 1, "0.7.0", false),
                    version_json("1.2.0", 1, "0.5.0", true),
                    version_json("2.0.0", 2, "0.5.0", false),
                ]
            ),
            plugin_json("future", vec![version_json("1.0.0", 1, "0.9.0", false)]),
            plugin_json("newapi", vec![version_json("1.0.0", 2, "0.1.0", false)]),
            plugin_json("oldapi", vec![version_json("1.0.0", 0, "0.1.0", false)]),
            plugin_json("gone", vec![version_json("1.0.0", 1, "0.1.0", true)]),
        ]));
        let index = RegistryIndex::parse(&text).unwrap();
        let now = app("0.6.0");
        let p = index.plugin("pk").unwrap();
        let by = |v: &str| p.version(v).unwrap();
        assert!(compatible(by("1.0.0"), &now, false));
        assert!(!compatible(by("1.1.0"), &now, false), "app too old");
        assert!(!compatible(by("1.2.0"), &now, false), "yanked");
        assert!(compatible(by("1.2.0"), &now, true));
        assert!(!compatible(by("2.0.0"), &now, true), "other plugin API");
        // A pre-release of the app counts as its release.
        assert_eq!(
            RegistryConfig::with_url("x".into(), "0.7.0-dev.3").app_version,
            Version::new(0, 7, 0)
        );
        assert_eq!(p.latest_compatible(&now).unwrap().version, "1.0.0");
        assert_eq!(p.latest_compatible(&app("0.7.0")).unwrap().version, "1.1.0");
        assert_eq!(p.incompatible_reason(&now), None);

        let reason = |id: &str| index.plugin(id).unwrap().incompatible_reason(&now).unwrap();
        assert_eq!(reason("future"), "Needs Questkeep 0.9.0 or newer.");
        assert!(reason("newapi").contains("newer plugin API"));
        assert!(reason("oldapi").contains("older plugin API"));
        assert!(reason("gone").contains("withdrawn"));
    }

    fn installed(id: &str, version: &str, registry: Option<(&str, bool)>) -> Installed {
        Installed {
            id: id.into(),
            version: version.into(),
            registry: registry.map(|(v, pinned)| RegistryState {
                version: v.into(),
                pinned,
            }),
        }
    }

    #[test]
    fn listing_marks_installed_plugins_updates_and_pins() {
        let text = index_json(serde_json::json!([
            plugin_json("fresh", vec![version_json("1.0.0", 1, "0.1.0", false)]),
            plugin_json(
                "old",
                vec![
                    version_json("1.0.0", 1, "0.1.0", false),
                    version_json("1.1.0", 1, "0.1.0", false),
                ]
            ),
            plugin_json(
                "pinned",
                vec![
                    version_json("1.0.0", 1, "0.1.0", false),
                    version_json("1.1.0", 1, "0.1.0", false),
                ]
            ),
            plugin_json(
                "folder",
                vec![
                    version_json("1.0.0", 1, "0.1.0", false),
                    version_json("1.1.0", 1, "0.1.0", false),
                ]
            ),
            plugin_json(
                "toonew",
                vec![
                    version_json("1.0.0", 1, "0.1.0", false),
                    version_json("1.1.0", 1, "0.9.0", false),
                    version_json("1.2.0", 1, "0.1.0", true),
                ]
            ),
            plugin_json("blocked", vec![version_json("1.0.0", 1, "0.9.0", false)]),
        ]));
        let index = RegistryIndex::parse(&text).unwrap();
        let installs = [
            installed("old", "1.0.0", Some(("1.0.0", false))),
            installed("pinned", "1.0.0", Some(("1.0.0", true))),
            installed("folder", "1.0.0", None),
            installed("toonew", "1.0.0", Some(("1.0.0", false))),
        ];
        let icons = BTreeMap::from([("old".to_string(), "data:image/png;base64,AA==".to_string())]);
        let listing = build_listing(
            &index,
            true,
            Some("t".into()),
            &installs,
            &app("0.6.0"),
            "https://example.org/plugins/index.json",
            &icons,
        );
        assert!(listing.offline);
        assert_eq!(listing.fetched_at.as_deref(), Some("t"));
        let get = |id: &str| listing.plugins.iter().find(|p| p.id == id).unwrap();

        assert!(get("fresh").installed.is_none() && !get("fresh").update_available);
        assert_eq!(get("fresh").latest.as_deref(), Some("1.0.0"));
        assert_eq!(
            get("fresh").icon_url.as_deref(),
            Some("https://example.org/plugins/icons/x.svg")
        );
        assert_eq!(
            get("old").icon_data.as_deref(),
            Some("data:image/png;base64,AA==")
        );
        assert!(get("fresh").icon_data.is_none());

        let old = get("old");
        assert_eq!(old.compatible_versions, ["1.0.0", "1.1.0"]);
        assert!(old.update_available);
        assert_eq!(
            old.installed,
            Some(RegistryInstalled {
                version: "1.0.0".into(),
                pinned: false,
                from_registry: true
            })
        );
        assert!(
            !get("pinned").update_available && get("pinned").installed.as_ref().unwrap().pinned
        );
        // Installed from a folder: shown, never offered an update.
        assert!(!get("folder").update_available);
        assert!(!get("folder").installed.as_ref().unwrap().from_registry);
        // Only versions this app can run count; withdrawn ones are listed but flagged.
        let toonew = get("toonew");
        assert_eq!(toonew.latest.as_deref(), Some("1.0.0"));
        assert!(!toonew.update_available);
        assert_eq!(toonew.versions.len(), 3);
        assert!(
            !toonew.versions[1].compatible
                && toonew.versions[2].yanked
                && toonew.versions[2].compatible
        );
        assert!(get("blocked").latest.is_none());
        assert_eq!(
            get("blocked").incompatible_reason.as_deref(),
            Some("Needs Questkeep 0.9.0 or newer.")
        );

        let names = BTreeMap::new();
        let found = updates(&index, &installs, &app("0.6.0"), &names);
        assert_eq!(found.len(), 1);
        assert_eq!(
            (
                found[0].id.as_str(),
                found[0].installed_version.as_str(),
                found[0].latest_version.as_str()
            ),
            ("old", "1.0.0", "1.1.0")
        );
        // A newer app unlocks toonew's 1.1.0.
        let found = updates(&index, &installs, &app("0.9.0"), &names);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn downloads_are_checked() {
        let body = b"PK pretend this is a zip".to_vec();
        let good = sha256_hex(&body);
        let big = vec![7u8; 5000];
        let base = serve({
            let body = body.clone();
            move |path| match path {
                "/ok.zip" => ok(&body),
                "/big.zip" => ok(&vec![7u8; 5000]),
                "/missing.zip" => status("404 Not Found", ""),
                "/redirect.zip" => status("302 Found", "Location: /ok.zip\r\n"),
                "/loop.zip" => status("302 Found", "Location: /loop.zip\r\n"),
                _ => status("500 Internal Server Error", ""),
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("cache/p/1.0.0.zip");
        let leftovers = || std::fs::read_dir(dest.parent().unwrap()).unwrap().count();

        download(&format!("{base}/ok.zip"), &good, body.len() as u64, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(sha256_file(&dest).unwrap(), good);
        assert_eq!(leftovers(), 1);

        // Redirects are followed (an upper-case hash is fine too).
        std::fs::remove_file(&dest).unwrap();
        download(
            &format!("{base}/redirect.zip"),
            &good.to_uppercase(),
            0,
            &dest,
        )
        .unwrap();
        assert!(dest.is_file());
        std::fs::remove_file(&dest).unwrap();

        // The wrong hash: refused, nothing stays.
        let err = download(&format!("{base}/ok.zip"), &"a".repeat(64), 0, &dest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not match the checksum"), "{err}");
        assert!(!dest.exists() && leftovers() == 0);
        // A wrong hash does not replace a good file that is there.
        std::fs::write(&dest, "kept").unwrap();
        assert!(download(&format!("{base}/ok.zip"), &"a".repeat(64), 0, &dest).is_err());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "kept");
        std::fs::remove_file(&dest).unwrap();

        // Too big: counted while reading, or announced by the index.
        let err = download_capped(
            &format!("{base}/big.zip"),
            &sha256_hex(&big),
            0,
            &dest,
            1000,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("larger than"), "{err}");
        assert!(!dest.exists() && leftovers() == 0);
        assert!(download_capped(&format!("{base}/ok.zip"), &good, 5_000_000, &dest, 1000).is_err());

        // Errors a person can read.
        let err = download(&format!("{base}/missing.zip"), &good, 0, &dest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("404"), "{err}");
        let err = download(&format!("{base}/loop.zip"), &good, 0, &dest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("redirected too many times"), "{err}");
        let err = download("http://example.org/p.zip", &good, 0, &dest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("only https"), "{err}");
        let err = download("ftp://example.org/p.zip", &good, 0, &dest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("only https"), "{err}");
        let err = download(&format!("{base}/ok.zip"), "xyz", 0, &dest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("checksum"), "{err}");
        assert!(!dest.exists());
    }

    #[test]
    fn the_index_comes_from_the_network_or_the_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = tmp.path().join("plugins");
        let text = index_json(serde_json::json!([plugin_json(
            "pk",
            vec![version_json("1.0.0", 1, "0.1.0", false)]
        )]));
        let icon = b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec();
        let base = serve({
            let text = text.clone();
            move |path| match path {
                "/index.json" => ok(text.as_bytes()),
                "/icons/x.svg" => ok(&icon),
                "/new.json" => ok(br#"{"schema":2,"plugins":[]}"#),
                "/huge.json" => ok(&vec![b' '; 6 * 1024 * 1024]),
                _ => status("404 Not Found", ""),
            }
        });
        let config = RegistryConfig::with_url(format!("{base}/index.json"), "0.6.0");

        let listing = fetch_listing(&config, &plugins, &[]).unwrap();
        assert!(!listing.offline);
        assert!(listing.fetched_at.is_some());
        assert_eq!(listing.plugins.len(), 1);
        let icon = listing.plugins[0].icon_data.clone().unwrap();
        assert!(icon.starts_with("data:image/svg+xml;base64,"), "{icon}");
        assert!(plugins.join(CACHE_FILE).is_file());
        assert_eq!(
            std::fs::read_dir(&plugins)
                .unwrap()
                .filter(|e| e
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp"))
                .count(),
            0
        );

        // Unreachable: the cache answers, marked offline, icon included.
        let dead = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!(
                "http://127.0.0.1:{}/index.json",
                listener.local_addr().unwrap().port()
            )
        };
        let dead_config = RegistryConfig::with_url(dead, "0.6.0");
        assert!(
            fetch_listing(&dead_config, &plugins, &[]).is_err(),
            "another registry's cache is not used"
        );
        // The same registry address, but the server is gone.
        let cache = read_cache(&plugins, &config.url).unwrap();
        let moved = RegistryCache {
            url: dead_config.url.clone(),
            ..cache
        };
        write_cache(&plugins, &moved).unwrap();
        let offline = fetch_listing(&dead_config, &plugins, &[]).unwrap();
        assert!(offline.offline);
        assert_eq!(offline.fetched_at, listing.fetched_at);
        assert_eq!(offline.plugins.len(), 1);

        // Too new: an error, not the cache.
        let new_config = RegistryConfig::with_url(format!("{base}/new.json"), "0.6.0");
        write_cache(
            &plugins,
            &RegistryCache {
                url: new_config.url.clone(),
                ..moved.clone()
            },
        )
        .unwrap();
        let err = fetch_listing(&new_config, &plugins, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("Update Questkeep"), "{err}");
        // Too large and not found count as unreachable.
        let huge = RegistryConfig::with_url(format!("{base}/huge.json"), "0.6.0");
        assert!(
            fetch_index(&huge.url)
                .unwrap_err()
                .to_string()
                .contains("larger than")
        );
        assert!(
            fetch_index(&format!("{base}/nope.json"))
                .unwrap_err()
                .to_string()
                .contains("404")
        );
        assert!(fetch_index("http://example.org/index.json").is_err());
    }

    /// A registry on a local port that tests can publish plugin versions to.
    struct Registry {
        config: RegistryConfig,
        state: Arc<std::sync::Mutex<ServerState>>,
    }

    #[derive(Default)]
    struct ServerState {
        base: String,
        files: BTreeMap<String, Vec<u8>>,
        plugins: BTreeMap<String, Vec<serde_json::Value>>,
        /// How often `index.json` was requested.
        index_hits: usize,
    }

    impl Registry {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
            let state = Arc::new(std::sync::Mutex::new(ServerState {
                base: base.clone(),
                ..Default::default()
            }));
            let shared = state.clone();
            serve_on(listener, move |path| {
                let mut state = shared.lock().unwrap();
                if path == "/index.json" {
                    state.index_hits += 1;
                    let plugins: Vec<serde_json::Value> = state
                        .plugins
                        .iter()
                        .map(|(id, versions)| plugin_json(id, versions.clone()))
                        .collect();
                    return ok(index_json(serde_json::Value::Array(plugins)).as_bytes());
                }
                match state.files.get(path) {
                    Some(bytes) => ok(bytes),
                    None => status("404 Not Found", ""),
                }
            });
            Self {
                config: RegistryConfig::with_url(format!("{base}/index.json"), "0.6.0"),
                state,
            }
        }

        /// Publishes `version` of `id`; its zip contains the plugin `inside` (normally `id`).
        fn publish(&self, id: &str, version: &str, inside: &str) {
            self.publish_with(id, version, inside, serde_json::json!({}));
        }

        /// [`publish`](Self::publish) with more `plugin.json` fields (a script for an
        /// `extension` is added to the zip).
        fn publish_with(&self, id: &str, version: &str, inside: &str, extra: serde_json::Value) {
            let tmp = tempfile::tempdir().unwrap();
            let mut manifest = serde_json::json!({
                "id": inside, "name": "P", "version": version, "apiVersion": 1,
                "themes": [{ "id": "t", "name": "T", "css": "theme.css" }],
            });
            for (key, value) in extra.as_object().unwrap() {
                manifest[key] = value.clone();
            }
            let manifest = manifest.to_string();
            let zip = tmp.path().join("p.zip");
            write_zip(
                &zip,
                &[
                    ("plugin.json", manifest.as_bytes()),
                    ("theme.css", version.as_bytes()),
                    ("ext.js", &b"// extension"[..]),
                ],
            );
            let bytes = std::fs::read(&zip).unwrap();
            let mut state = self.state.lock().unwrap();
            let mut entry = version_json(version, 1, "0.1.0", false);
            entry["url"] = format!("{}/{id}-{version}.zip", state.base).into();
            entry["sha256"] = sha256_hex(&bytes).into();
            entry["size"] = bytes.len().into();
            state.plugins.entry(id.into()).or_default().push(entry);
            state.files.insert(format!("/{id}-{version}.zip"), bytes);
        }

        fn yank(&self, id: &str, version: &str) {
            let mut state = self.state.lock().unwrap();
            for entry in state.plugins.get_mut(id).unwrap() {
                if entry["version"] == version {
                    entry["yanked"] = true.into();
                }
            }
        }

        fn refresh(&self, manager: &PluginManager) -> RegistryListing {
            manager.registry_listing(&self.config).unwrap()
        }
    }

    fn manager(root: &Path) -> PluginManager {
        PluginManager::new(
            DataPaths::new(root.join("data")),
            ProviderRegistry::new(),
            EventBus::new(),
        )
    }

    #[test]
    fn browses_previews_installs_updates_and_downgrades() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = manager(tmp.path());
        let paths = manager.paths().clone();
        let registry = Registry::new();
        let config = &registry.config;
        registry.publish("pk", "1.0.0", "pk");

        let listing = registry.refresh(&manager);
        assert_eq!(listing.plugins[0].latest.as_deref(), Some("1.0.0"));
        assert!(listing.plugins[0].installed.is_none());

        // Previewing downloads into the cache and installs nothing.
        let preview = manager
            .preview_registry_plugin(config, "pk", "1.0.0")
            .unwrap();
        assert_eq!(preview.preview.id, "pk");
        assert!(!preview.pinned && !preview.downgrade);
        assert!(cached_zip_path(&paths, "pk", "1.0.0").is_file());
        assert!(manager.list().is_empty());

        let info = manager
            .install_registry_plugin(config, "pk", "1.0.0")
            .unwrap();
        assert_eq!(
            info.registry,
            Some(RegistryState {
                version: "1.0.0".into(),
                pinned: false
            })
        );
        assert_eq!(
            Path::new(&info.source),
            cached_zip_path(&paths, "pk", "1.0.0")
        );
        assert!(info.loaded && info.enabled);
        // Reload works from the cached zip.
        assert_eq!(manager.reload("pk").unwrap().version, "1.0.0");

        // An update: the data is backed up first, old zips are dropped.
        std::fs::create_dir_all(paths.plugin_data_dir().join("pk")).unwrap();
        std::fs::write(paths.plugin_data_dir().join("pk/x.txt"), "mine").unwrap();
        registry.publish("pk", "1.1.0", "pk");
        assert!(
            manager.plugin_updates(config).is_empty(),
            "the cache is not refreshed yet"
        );
        let listing = registry.refresh(&manager);
        assert!(listing.plugins[0].update_available);
        let updates = manager.plugin_updates(config);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].latest_version, "1.1.0");
        let info = manager
            .install_registry_plugin(config, "pk", "1.1.0")
            .unwrap();
        assert_eq!(info.version, "1.1.0");
        assert!(!info.registry.unwrap().pinned);
        assert_eq!(
            std::fs::read_to_string(paths.plugin_data_backup_dir().join("pk/1.0.0/x.txt")).unwrap(),
            "mine"
        );
        let cached = std::fs::read_dir(paths.plugin_cache_dir().join("pk"))
            .unwrap()
            .count();
        assert_eq!(cached, 1);
        assert!(manager.plugin_updates(config).is_empty());

        // Going back is a downgrade and pins the plugin.
        let preview = manager
            .preview_registry_plugin(config, "pk", "1.0.0")
            .unwrap();
        assert!(preview.downgrade && preview.pinned);
        let info = manager
            .install_registry_plugin(config, "pk", "1.0.0")
            .unwrap();
        assert_eq!(info.version, "1.0.0");
        assert!(info.registry.as_ref().unwrap().pinned);
        assert!(paths.plugin_data_backup_dir().join("pk/1.1.0").is_dir());
        let listing = registry.refresh(&manager);
        assert!(!listing.plugins[0].update_available);
        assert!(listing.plugins[0].installed.as_ref().unwrap().pinned);
        assert!(manager.plugin_updates(config).is_empty());

        // A withdrawn version: fine while it is the installed one, refused as a target.
        manager
            .install_registry_plugin(config, "pk", "1.1.0")
            .unwrap();
        registry.yank("pk", "1.1.0");
        registry.publish("pk", "1.2.0", "pk");
        registry.refresh(&manager);
        assert!(
            manager
                .preview_registry_plugin(config, "pk", "1.1.0")
                .is_ok()
        );
        manager
            .install_registry_plugin(config, "pk", "1.0.0")
            .unwrap();
        let err = manager
            .preview_registry_plugin(config, "pk", "1.1.0")
            .unwrap_err()
            .to_string();
        assert!(err.contains("withdrawn"), "{err}");
        let err = manager
            .install_registry_plugin(config, "pk", "9.9.9")
            .unwrap_err()
            .to_string();
        assert!(err.contains("no version 9.9.9"), "{err}");
        let err = manager
            .install_registry_plugin(config, "nope", "1.0.0")
            .unwrap_err()
            .to_string();
        assert!(err.contains("no plugin \"nope\""), "{err}");

        // Removing it cleans the cache; its data and backups go with `delete_data`.
        manager.remove("pk", false).unwrap();
        assert!(!paths.plugin_cache_dir().join("pk").exists());
        assert!(paths.plugin_data_backup_dir().join("pk").is_dir());
        manager
            .install_registry_plugin(config, "pk", "1.2.0")
            .unwrap();
        manager.remove("pk", true).unwrap();
        assert!(!paths.plugin_data_backup_dir().join("pk").exists());
        assert!(!paths.plugin_data_dir().join("pk").exists());
    }

    #[test]
    fn a_zip_that_is_not_what_the_registry_says_is_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = manager(tmp.path());
        let registry = Registry::new();
        registry.publish("claimed", "1.0.0", "other");
        registry.refresh(&manager);
        let err = manager
            .install_registry_plugin(&registry.config, "claimed", "1.0.0")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("\"other\"") && err.contains("not \"claimed\""),
            "{err}"
        );
        let err = manager
            .preview_registry_plugin(&registry.config, "claimed", "1.0.0")
            .unwrap_err()
            .to_string();
        assert!(err.contains("\"other\""), "{err}");
        assert!(manager.list().is_empty());
        assert!(!manager.plugin_dir("other").exists() && !manager.plugin_dir("claimed").exists());
    }

    #[test]
    fn registry_installs_of_one_plugin_do_not_interleave() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = manager(tmp.path());
        let registry = Registry::new();
        for version in ["1.0.0", "1.1.0", "1.2.0"] {
            registry.publish("pk", version, "pk");
        }
        registry.refresh(&manager);
        let handles: Vec<_> = ["1.0.0", "1.1.0", "1.2.0", "1.1.0"]
            .into_iter()
            .map(|version| {
                let manager = manager.clone();
                let config = registry.config.clone();
                std::thread::spawn(move || manager.install_registry_plugin(&config, "pk", version))
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        // Whatever order they ran in, one complete copy is installed and listed once.
        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert!(list[0].loaded && list[0].error.is_none());
        assert_eq!(list[0].registry.as_ref().unwrap().version, list[0].version);
    }

    fn shape(
        http: &[&str],
        read: &[&str],
        folders: &[&str],
        stores: u32,
        ext: bool,
    ) -> PluginShape {
        PluginShape {
            permissions: PluginPermissions {
                read: read.iter().map(ToString::to_string).collect(),
                http: http.iter().map(ToString::to_string).collect(),
                folders: folders
                    .iter()
                    .map(|key| FolderRequest {
                        key: key.to_string(),
                        label: format!("Label {key}"),
                        hint: None,
                        default: None,
                    })
                    .collect(),
            },
            stores,
            has_extension: ext,
        }
    }

    #[test]
    fn changes_list_what_a_new_version_asks_for_beyond_the_installed_one() {
        let old = shape(&["api.example.com"], &["~/a"], &["inst"], 1, false);
        // The same again, or less: nothing to review.
        assert_eq!(PluginChanges::between(&old, &old), PluginChanges::default());
        let less = shape(&[], &[], &[], 1, false);
        assert!(!PluginChanges::between(&old, &less).needs_review);

        let host = PluginChanges::between(
            &old,
            &shape(
                &["api.example.com", "*.cdn.com"],
                &["~/a"],
                &["inst"],
                1,
                false,
            ),
        );
        assert_eq!(host.new_http, ["*.cdn.com"]);
        assert!(host.needs_review && host.new_read.is_empty() && !host.adds_extension);
        // Case and spaces do not make a host new.
        let same_host = PluginChanges::between(
            &old,
            &shape(&[" API.example.com "], &["~/A"], &["inst"], 1, false),
        );
        assert!(!same_host.needs_review);

        let read = PluginChanges::between(
            &old,
            &shape(
                &["api.example.com"],
                &["~/a", "C:/games"],
                &["inst"],
                1,
                false,
            ),
        );
        assert_eq!(read.new_read, ["C:/games"]);
        assert!(read.needs_review);

        let folder = PluginChanges::between(
            &old,
            &shape(&["api.example.com"], &["~/a"], &["inst", "saves"], 1, false),
        );
        assert_eq!(folder.new_folders.len(), 1);
        assert_eq!(folder.new_folders[0].key, "saves");
        assert!(folder.needs_review);

        let ext = PluginChanges::between(
            &old,
            &shape(&["api.example.com"], &["~/a"], &["inst"], 1, true),
        );
        assert!(ext.adds_extension && ext.needs_review);
        // An extension that was there already is nothing new.
        let had = shape(&[], &[], &[], 0, true);
        assert!(!PluginChanges::between(&had, &had).needs_review);

        let store = PluginChanges::between(
            &old,
            &shape(&["api.example.com"], &["~/a"], &["inst"], 2, false),
        );
        assert!(store.adds_store && store.needs_review);
    }

    #[test]
    fn the_registry_preview_compares_with_the_installed_version() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = manager(tmp.path());
        let registry = Registry::new();
        let config = &registry.config;
        registry.publish("pk", "1.0.0", "pk");
        // Not installed: nothing to compare with.
        let fresh = manager
            .preview_registry_plugin(config, "pk", "1.0.0")
            .unwrap();
        assert_eq!(fresh.changes, PluginChanges::default());
        manager
            .install_registry_plugin(config, "pk", "1.0.0")
            .unwrap();

        // 1.1.0 asks for a host and brings an extension: it needs review.
        registry.publish_with(
            "pk",
            "1.1.0",
            "pk",
            serde_json::json!({
                "permissions": { "http": ["api.example.com"] },
                "extension": { "main": "ext.js" },
            }),
        );
        registry.refresh(&manager);
        let preview = manager
            .preview_registry_plugin(config, "pk", "1.1.0")
            .unwrap();
        assert_eq!(preview.changes.new_http, ["api.example.com"]);
        assert!(preview.changes.adds_extension && preview.changes.needs_review);
        manager
            .install_registry_plugin(config, "pk", "1.1.0")
            .unwrap();

        // 1.2.0 asks for no more than 1.1.0 does: no review.
        registry.publish_with(
            "pk",
            "1.2.0",
            "pk",
            serde_json::json!({
                "permissions": { "http": ["api.example.com"] },
                "extension": { "main": "ext.js" },
            }),
        );
        registry.refresh(&manager);
        let preview = manager
            .preview_registry_plugin(config, "pk", "1.2.0")
            .unwrap();
        assert_eq!(preview.changes, PluginChanges::default());
    }

    fn hits(registry: &Registry) -> usize {
        registry.state.lock().unwrap().index_hits
    }

    #[test]
    fn the_check_asks_the_registry_only_when_it_has_a_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = manager(tmp.path());
        let registry = Registry::new();
        let config = &registry.config;
        registry.publish("pk", "1.0.0", "pk");

        // No plugin from the registry: no request, whatever the setting says.
        for kind in [CheckKind::Automatic, CheckKind::Manual] {
            let check = manager.check_for_updates(config, kind, true).unwrap();
            assert!(!check.requested && check.updates.is_empty());
        }
        assert_eq!(hits(&registry), 0);

        // A plugin from a folder is no reason either.
        let dir = tmp.path().join("folder-plugin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"id":"local","name":"L","version":"1.0.0","apiVersion":1}"#,
        )
        .unwrap();
        manager.install(&dir).unwrap();
        assert!(
            !manager
                .check_for_updates(config, CheckKind::Manual, true)
                .unwrap()
                .requested
        );
        assert_eq!(hits(&registry), 0);

        manager
            .install_registry_plugin(config, "pk", "1.0.0")
            .unwrap();
        let installed_hits = hits(&registry);
        registry.publish("pk", "1.1.0", "pk");

        // The automatic check is off: no request. The button still asks.
        let off = manager
            .check_for_updates(config, CheckKind::Automatic, false)
            .unwrap();
        assert!(!off.requested && off.updates.is_empty());
        assert_eq!(hits(&registry), installed_hits);
        let manual = manager
            .check_for_updates(config, CheckKind::Manual, false)
            .unwrap();
        assert!(manual.requested && !manual.failed);
        assert_eq!(manual.updates.len(), 1);
        assert_eq!(manual.updates[0].latest_version, "1.1.0");
        assert_eq!(hits(&registry), installed_hits + 1);

        // On: it asks and finds the same.
        let on = manager
            .check_for_updates(config, CheckKind::Automatic, true)
            .unwrap();
        assert!(on.requested && on.updates.len() == 1);
        assert_eq!(hits(&registry), installed_hits + 2);
    }

    #[test]
    fn an_unreachable_registry_is_silent_for_the_schedule_and_reported_for_the_button() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = manager(tmp.path());
        let registry = Registry::new();
        registry.publish("pk", "1.0.0", "pk");
        manager
            .install_registry_plugin(&registry.config, "pk", "1.0.0")
            .unwrap();
        let down = RegistryConfig::with_url("http://127.0.0.1:1/index.json".into(), "0.6.0");

        let auto = manager
            .check_for_updates(&down, CheckKind::Automatic, true)
            .unwrap();
        assert!(auto.requested && auto.failed && auto.updates.is_empty());
        let bus = EventBus::new();
        let notes = Notifications::in_memory(&bus);
        let scheduled = manager.run_scheduled_check(&down, &notes, true);
        assert!(scheduled.failed);
        assert!(notes.list().is_empty(), "offline is no news");
        let err = manager
            .check_for_updates(&down, CheckKind::Manual, true)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("could not be loaded") || err.contains("Could not reach"),
            "{err}"
        );
    }

    /// Two registry plugins installed at 1.0.0, a registry that offers 1.0.0 of both, and a
    /// bell. `Fixture::update` publishes a new version.
    struct NoticeFixture {
        tmp: tempfile::TempDir,
        manager: PluginManager,
        registry: Registry,
        bus: EventBus,
    }

    impl NoticeFixture {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let bus = EventBus::new();
            let manager = PluginManager::new(
                DataPaths::new(tmp.path().join("data")),
                ProviderRegistry::new(),
                bus.clone(),
            );
            let registry = Registry::new();
            registry.publish("pk", "1.0.0", "pk");
            registry.publish("pq", "1.0.0", "pq");
            for id in ["pk", "pq"] {
                manager
                    .install_registry_plugin(&registry.config, id, "1.0.0")
                    .unwrap();
            }
            Self {
                tmp,
                manager,
                registry,
                bus,
            }
        }

        fn bell(&self) -> Notifications {
            Notifications::open(self.tmp.path().join("notifications.json"), &self.bus)
        }

        fn config(&self) -> &RegistryConfig {
            &self.registry.config
        }

        /// The scheduled check.
        fn check(&self, bell: &Notifications) {
            self.manager.run_scheduled_check(self.config(), bell, true);
        }

        /// The bell entries for updates.
        fn entries(bell: &Notifications) -> Vec<questkeep_core::notifications::Notification> {
            bell.with_action(&NotificationAction::PluginUpdates)
        }

        fn notices_file(&self) -> PathBuf {
            self.tmp
                .path()
                .join("data")
                .join("plugins")
                .join("plugin-update-notices.json")
        }
    }

    #[test]
    fn the_entry_lists_the_pending_updates_and_is_created_unread() {
        let fx = NoticeFixture::new();
        let bell = fx.bell();
        fx.check(&bell);
        assert!(NoticeFixture::entries(&bell).is_empty(), "nothing pending");

        fx.registry.publish("pk", "1.1.0", "pk");
        fx.check(&bell);
        let list = bell.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].source, NotificationSource::Plugins);
        assert_eq!(list[0].action, Some(NotificationAction::PluginUpdates));
        assert!(!list[0].read);
        assert!(
            list[0].message.starts_with("1 plugin update available: "),
            "{}",
            list[0].message
        );
        assert!(
            list[0].message.contains("1.0.0 \u{2192} 1.1.0"),
            "{}",
            list[0].message
        );
    }

    #[test]
    fn the_entry_appears_at_start_from_the_cache() {
        let fx = NoticeFixture::new();
        fx.registry.publish("pk", "1.1.0", "pk");
        // The check of an earlier run refreshed the cache.
        fx.manager
            .check_for_updates(fx.config(), CheckKind::Manual, true)
            .unwrap();
        let hits_before = hits(&fx.registry);

        // A start: a bell without the entry, no network.
        let bell = fx.bell();
        fx.manager.sync_update_notice(fx.config(), &bell, true);
        assert_eq!(NoticeFixture::entries(&bell).len(), 1);
        assert_eq!(hits(&fx.registry), hits_before, "the cache is enough");

        // The setting is off: nothing is created from the cache.
        let quiet = Notifications::in_memory(&fx.bus);
        fx.manager.sync_update_notice(fx.config(), &quiet, false);
        assert!(quiet.list().is_empty());
    }

    #[test]
    fn the_entry_is_updated_in_place_and_unread_only_for_new_pairs() {
        let fx = NoticeFixture::new();
        let bell = fx.bell();
        fx.registry.publish("pk", "1.1.0", "pk");
        fx.check(&bell);
        let first = NoticeFixture::entries(&bell).remove(0);
        bell.mark_all_read();

        // Nothing changed: the entry stays as it is, read.
        fx.check(&bell);
        let same = NoticeFixture::entries(&bell);
        assert_eq!(same.len(), 1);
        assert_eq!(
            same[0],
            bell.with_action(&NotificationAction::PluginUpdates)[0]
        );
        assert!(same[0].read);

        // A second plugin has an update: same entry, new text, unread again.
        fx.registry.publish("pq", "1.0.1", "pq");
        fx.check(&bell);
        let list = NoticeFixture::entries(&bell);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, first.id);
        assert!(!list[0].read);
        assert!(list[0].message.starts_with("2 plugin updates available: "));
        assert!(list[0].message.contains("1.0.0 \u{2192} 1.0.1"));
        bell.mark_all_read();

        // A newer version of one of them is a new pair too.
        fx.registry.publish("pk", "1.2.0", "pk");
        fx.check(&bell);
        let list = NoticeFixture::entries(&bell);
        assert!(!list[0].read);
        assert!(list[0].message.contains("1.0.0 \u{2192} 1.2.0"));
        assert!(!list[0].message.contains("1.1.0"));
        bell.mark_all_read();

        // One is installed: the text shrinks, and that is no reason to turn unread.
        fx.manager
            .install_registry_plugin(fx.config(), "pk", "1.2.0")
            .unwrap();
        fx.manager.sync_update_notice(fx.config(), &bell, true);
        let list = NoticeFixture::entries(&bell);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, first.id);
        assert!(list[0].read);
        assert!(list[0].message.starts_with("1 plugin update available: "));
        assert!(!list[0].message.contains("1.2.0"));

        // The last one is installed: the entry goes by itself.
        fx.manager
            .install_registry_plugin(fx.config(), "pq", "1.0.1")
            .unwrap();
        fx.manager.sync_update_notice(fx.config(), &bell, true);
        assert!(bell.list().is_empty());
    }

    #[test]
    fn a_removed_plugin_takes_its_update_along() {
        let fx = NoticeFixture::new();
        let bell = fx.bell();
        fx.registry.publish("pk", "1.1.0", "pk");
        fx.check(&bell);
        assert_eq!(NoticeFixture::entries(&bell).len(), 1);
        fx.manager.remove("pk", false).unwrap();
        fx.manager.sync_update_notice(fx.config(), &bell, true);
        assert!(bell.list().is_empty());
    }

    #[test]
    fn a_dismissed_entry_stays_gone_until_a_new_pair_appears() {
        let fx = NoticeFixture::new();
        let bell = fx.bell();
        fx.registry.publish("pk", "1.1.0", "pk");
        fx.check(&bell);
        let id = NoticeFixture::entries(&bell)[0].id;
        bell.dismiss(id);

        // Later checks, a start, "Clear all": the same pair stays quiet.
        fx.check(&bell);
        assert!(bell.list().is_empty());
        let restarted = fx.bell();
        fx.manager.sync_update_notice(fx.config(), &restarted, true);
        assert!(restarted.list().is_empty());
        fx.manager
            .run_manual_check(fx.config(), &restarted)
            .unwrap();
        assert!(restarted.list().is_empty());

        // A new version is a new pair: the entry comes back, unread, and lists everything pending.
        fx.registry.publish("pk", "1.2.0", "pk");
        fx.check(&restarted);
        let list = restarted.list();
        assert_eq!(list.len(), 1);
        assert!(!list[0].read);
        assert!(list[0].message.contains("1.0.0 \u{2192} 1.2.0"));

        // Clearing everything counts as dismissing, too.
        restarted.clear();
        fx.check(&restarted);
        assert!(restarted.list().is_empty());
        fx.registry.publish("pq", "1.0.1", "pq");
        fx.check(&restarted);
        let list = restarted.list();
        assert_eq!(list.len(), 1);
        assert!(list[0].message.starts_with("2 plugin updates available: "));
    }

    #[test]
    fn dismissing_then_installing_some_does_not_bring_it_back() {
        let fx = NoticeFixture::new();
        let bell = fx.bell();
        fx.registry.publish("pk", "1.1.0", "pk");
        fx.registry.publish("pq", "1.0.1", "pq");
        fx.check(&bell);
        bell.dismiss(NoticeFixture::entries(&bell)[0].id);
        fx.manager
            .install_registry_plugin(fx.config(), "pk", "1.1.0")
            .unwrap();
        fx.manager.sync_update_notice(fx.config(), &bell, true);
        assert!(bell.list().is_empty());
    }

    #[test]
    fn the_entry_and_the_dismissed_pairs_survive_a_restart() {
        let fx = NoticeFixture::new();
        let bell = fx.bell();
        fx.registry.publish("pk", "1.1.0", "pk");
        fx.check(&bell);
        bell.mark_all_read();

        // The bell is read from its file again, and checks again: still the one read entry.
        let restarted = fx.bell();
        let before = NoticeFixture::entries(&restarted);
        assert_eq!(before.len(), 1);
        assert!(before[0].read);
        fx.manager.sync_update_notice(fx.config(), &restarted, true);
        fx.check(&restarted);
        assert_eq!(NoticeFixture::entries(&restarted), before);

        // The memory of what was dismissed is read from its file, too.
        restarted.dismiss(before[0].id);
        fx.check(&restarted);
        let again = fx.bell();
        fx.manager.sync_update_notice(fx.config(), &again, true);
        assert!(again.list().is_empty());
        let saved = std::fs::read_to_string(fx.notices_file()).unwrap();
        assert!(saved.contains("pk@1.1.0"), "{saved}");
    }

    #[test]
    fn the_first_notices_file_still_loads() {
        let fx = NoticeFixture::new();
        fx.registry.publish("pk", "1.1.0", "pk");
        // A plain list of announced pairs, as the first version wrote it.
        std::fs::write(fx.notices_file(), r#"["pk@1.1.0"]"#).unwrap();

        // No entry in the bell: it carries no word about dismissing, so the entry comes.
        let bell = fx.bell();
        fx.check(&bell);
        assert_eq!(NoticeFixture::entries(&bell).len(), 1);

        // An entry that is in the bell from that time is taken over as it is (read stays read).
        let old = Notifications::in_memory(&fx.bus);
        old.post(
            NewNotification::info(NotificationSource::Plugins, "old", "old text")
                .action(NotificationAction::PluginUpdates),
        );
        old.mark_all_read();
        std::fs::write(fx.notices_file(), r#"["pk@1.1.0"]"#).unwrap();
        fx.check(&old);
        let list = NoticeFixture::entries(&old);
        assert_eq!(list.len(), 1);
        assert!(list[0].read);
        assert!(list[0].message.starts_with("1 plugin update available: "));

        // A broken file is an empty memory.
        std::fs::write(fx.notices_file(), "{nope").unwrap();
        fx.check(&old);
        assert_eq!(NoticeFixture::entries(&old).len(), 1);
    }

    #[test]
    fn check_now_creates_and_refreshes_the_entry() {
        let fx = NoticeFixture::new();
        let bell = fx.bell();
        fx.registry.publish("pk", "1.1.0", "pk");
        let check = fx.manager.run_manual_check(fx.config(), &bell).unwrap();
        assert_eq!(check.updates.len(), 1);
        assert_eq!(NoticeFixture::entries(&bell).len(), 1);

        fx.registry.publish("pq", "1.0.1", "pq");
        fx.manager.run_manual_check(fx.config(), &bell).unwrap();
        let list = NoticeFixture::entries(&bell);
        assert_eq!(list.len(), 1);
        assert!(list[0].message.starts_with("2 plugin updates available: "));
    }

    #[test]
    fn the_check_publishes_the_change_of_updates() {
        let fx = NoticeFixture::new();
        let changed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = changed.clone();
        fx.bus.subscribe(move |event| {
            if matches!(event, CoreEvent::PluginUpdatesChanged) {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let bell = fx.bell();
        fx.check(&bell);
        assert_eq!(changed.load(std::sync::atomic::Ordering::SeqCst), 0);
        fx.registry.publish("pk", "1.1.0", "pk");
        fx.check(&bell);
        assert_eq!(changed.load(std::sync::atomic::Ordering::SeqCst), 1);
        fx.check(&bell);
        assert_eq!(changed.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn the_notice_names_the_plugins() {
        let update = |id: &str, from: &str, to: &str| PluginUpdate {
            id: id.into(),
            name: id.to_uppercase(),
            installed_version: from.into(),
            latest_version: to.into(),
            notes: None,
        };
        let (title, message) = crate::update_check::notice_text(&[
            update("a", "0.1.0", "0.2.0"),
            update("b", "1.0.0", "1.0.1"),
        ]);
        assert_eq!(title, "Plugin updates available");
        assert_eq!(
            message,
            "2 plugin updates available: A 0.1.0 \u{2192} 0.2.0, B 1.0.0 \u{2192} 1.0.1"
        );
    }
}
