//! Shared Linux support for the Epic and GOG providers: both read the data of the Heroic Games
//! Launcher (and, for Epic, plain Legendary) instead of the Windows launchers.
//!
//! Everything here is plain functions over injected directories, so it runs and is tested on all
//! platforms; only [`HeroicPaths::detect`] looks at the real environment.
//!
//! Sources (verified against the public Heroic and Legendary code, see `docs/DECISIONS.md`):
//! - Legendary's `installed.json`: a JSON object keyed by app name; each entry has `app_name`,
//!   `title`, `install_path`, `is_dlc`, `platform` (`Windows` or `Mac`), ...
//! - Heroic's `gog_store/installed.json`: `{"installed": [{"appName", "install_path", "platform",
//!   "is_dlc", ...}]}`; titles come from Heroic's cached GOG library (`{"games": [{"app_name",
//!   "title"}]}`).
//! - Launching: `heroic://launch/<runner>/<appName>` (Heroic's protocol handler also accepts
//!   `heroic://launch?appName=..&runner=..`; the path form works in old and new versions).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::ProviderError;
use crate::model::{ImportedGame, PlayAction, TrackingMode};

const HEROIC_FLATPAK: &str = ".var/app/com.heroicgameslauncher.hgl/config/heroic";

/// Where one Legendary data directory (containing `installed.json`) came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegendarySource {
    pub dir: PathBuf,
    /// `true` for the copy bundled with Heroic: its games are known to Heroic and launch through
    /// it. A standalone Legendary directory launches through the `legendary` command.
    pub via_heroic: bool,
}

/// The existing Heroic/Legendary directories of a machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeroicPaths {
    /// Heroic config directories (`~/.config/heroic`, the Flatpak equivalent).
    pub config_dirs: Vec<PathBuf>,
    /// Legendary directories, Heroic's bundled ones first.
    pub legendary: Vec<LegendarySource>,
    /// The `legendary` executable, when found on `PATH`.
    pub legendary_bin: Option<PathBuf>,
}

impl HeroicPaths {
    /// Looks at the real environment (`HOME`, `XDG_CONFIG_HOME`, `PATH`).
    pub fn detect() -> Self {
        let Some(home) = dirs::home_dir() else {
            return Self::default();
        };
        let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        let path = std::env::var_os("PATH");
        Self::discover(&home, xdg.as_deref(), path.as_deref())
    }

    /// Finds the existing directories below `home`, `xdg_config` (`$XDG_CONFIG_HOME`) and the
    /// executables on `path_var`. Nothing outside these arguments is read.
    pub fn discover(home: &Path, xdg_config: Option<&Path>, path_var: Option<&OsStr>) -> Self {
        let mut config_roots: Vec<PathBuf> = Vec::new();
        if let Some(x) = xdg_config.filter(|x| x.is_absolute()) {
            config_roots.push(x.to_path_buf());
        }
        config_roots.push(home.join(".config"));

        let mut config_dirs: Vec<PathBuf> = Vec::new();
        let candidates = config_roots
            .iter()
            .map(|r| r.join("heroic"))
            .chain(std::iter::once(home.join(HEROIC_FLATPAK)));
        for dir in candidates {
            if dir.is_dir() && !config_dirs.contains(&dir) {
                config_dirs.push(dir);
            }
        }

        let mut legendary: Vec<LegendarySource> = Vec::new();
        let mut push = |dir: PathBuf, via_heroic: bool| {
            if dir.is_dir() && !legendary.iter().any(|s| s.dir == dir) {
                legendary.push(LegendarySource { dir, via_heroic });
            }
        };
        for c in &config_dirs {
            push(c.join("legendaryConfig").join("legendary"), true);
        }
        for r in &config_roots {
            push(r.join("legendary"), false);
        }

        Self {
            config_dirs,
            legendary,
            legendary_bin: find_in_path("legendary", path_var),
        }
    }

    fn has_any(&self) -> bool {
        !self.config_dirs.is_empty() || !self.legendary.is_empty()
    }
}

/// First executable file called `name` in the `PATH`-style list.
fn find_in_path(name: &str, path_var: Option<&OsStr>) -> Option<PathBuf> {
    let path_var = path_var?;
    std::env::split_paths(path_var)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// Percent-encodes everything except unreserved URI characters.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `heroic://launch/<runner>/<appName>`.
pub fn launch_uri(runner: &str, app_name: &str) -> String {
    format!(
        "heroic://launch/{}/{}",
        percent_encode(runner),
        percent_encode(app_name)
    )
}

/// A play action that starts the game through Heroic and then watches its install folder (the
/// Wine/Proton process is matched through its working directory and `Z:\` arguments).
fn heroic_action(runner: &str, app_name: &str, install_dir: &str) -> PlayAction {
    let mut action = PlayAction::uri(launch_uri(runner, app_name));
    action.tracking = TrackingMode::Directory;
    action.tracking_path = Some(install_dir.to_string());
    action
}

/// Plain `legendary launch <app>`, used for standalone Legendary installs.
fn legendary_action(bin: &Path, app_name: &str, install_dir: &str) -> PlayAction {
    let mut action = PlayAction::file(bin.display().to_string());
    action.args = Some(format!("launch {app_name}"));
    action.working_dir = Some(install_dir.to_string());
    action.tracking = TrackingMode::Directory;
    action.tracking_path = Some(install_dir.to_string());
    action
}

fn str_field<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// An entry of Legendary's `installed.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegendaryInstall {
    pub app_name: String,
    pub title: Option<String>,
    pub install_path: String,
    pub is_dlc: bool,
    pub platform: Option<String>,
}

/// Parses Legendary's `installed.json` (an object keyed by app name). Entries without an app
/// name or install path are dropped.
pub fn parse_legendary_installed(json: &str) -> Result<Vec<LegendaryInstall>, String> {
    let value: Value =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let Some(map) = value.as_object() else {
        return Err("expected a JSON object".into());
    };
    let mut out = Vec::new();
    for (key, entry) in map {
        let app_name = str_field(entry, "app_name").unwrap_or(key.as_str());
        let Some(install_path) = str_field(entry, "install_path") else {
            continue;
        };
        out.push(LegendaryInstall {
            app_name: app_name.to_string(),
            title: str_field(entry, "title").map(str::to_string),
            install_path: install_path.to_string(),
            is_dlc: entry.get("is_dlc").and_then(Value::as_bool) == Some(true),
            platform: str_field(entry, "platform").map(str::to_string),
        });
    }
    Ok(out)
}

fn dir_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

/// Installed Epic games from Legendary/Heroic data. Same ids (`AppName`) as the Windows launcher.
pub fn legendary_games(paths: &HeroicPaths) -> Result<Vec<ImportedGame>, ProviderError> {
    if !paths.has_any() {
        return Err(ProviderError::NotFound(
            "Neither Heroic Games Launcher nor Legendary was found".into(),
        ));
    }
    let mut games: Vec<ImportedGame> = Vec::new();
    for source in &paths.legendary {
        let file = source.dir.join("installed.json");
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(ProviderError::Io {
                    path: file.display().to_string(),
                    source,
                });
            }
        };
        let installs = match parse_legendary_installed(&text) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("ignoring malformed {}: {e}", file.display());
                continue;
            }
        };
        for install in installs {
            if install.is_dlc
                || install
                    .platform
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case("mac"))
            {
                continue;
            }
            if games
                .iter()
                .any(|g| g.source_game_id.eq_ignore_ascii_case(&install.app_name))
            {
                continue;
            }
            if !Path::new(&install.install_path).is_dir() {
                tracing::warn!(
                    "Epic game {} has no existing install folder, skipped",
                    install.app_name
                );
                continue;
            }
            let name = install
                .title
                .clone()
                .or_else(|| dir_name(&install.install_path))
                .unwrap_or_else(|| install.app_name.clone());
            let mut game = ImportedGame::installed(&install.app_name, name);
            game.install_dir = Some(install.install_path.clone());
            game.play_action = if source.via_heroic {
                Some(heroic_action(
                    "legendary",
                    &install.app_name,
                    &install.install_path,
                ))
            } else if let Some(bin) = &paths.legendary_bin {
                Some(legendary_action(
                    bin,
                    &install.app_name,
                    &install.install_path,
                ))
            } else {
                tracing::warn!(
                    "Epic game {} is installed with Legendary, but `legendary` is not on PATH",
                    install.app_name
                );
                None
            };
            games.push(game);
        }
    }
    games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
    Ok(games)
}

/// Product id to title from Heroic's cached GOG library (`{"games": [...]}` or a bare array).
pub fn parse_gog_library_titles(json: &str) -> HashMap<String, String> {
    let Ok(value) = serde_json::from_str::<Value>(json.trim_start_matches('\u{feff}')) else {
        return HashMap::new();
    };
    let list = value.get("games").unwrap_or(&value);
    let mut titles = HashMap::new();
    for entry in list.as_array().into_iter().flatten() {
        if let (Some(id), Some(title)) = (str_field(entry, "app_name"), str_field(entry, "title")) {
            titles.insert(id.to_string(), title.to_string());
        }
    }
    titles
}

/// An entry of Heroic's `gog_store/installed.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroicGogInstall {
    pub app_name: String,
    pub install_path: String,
    pub is_dlc: bool,
    pub platform: Option<String>,
}

/// Parses `{"installed": [...]}`. Entries without `appName` are dropped (Heroic does the same).
pub fn parse_gog_installed(json: &str) -> Result<Vec<HeroicGogInstall>, String> {
    let value: Value =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let Some(list) = value.get("installed").and_then(Value::as_array) else {
        return Err("missing \"installed\" list".into());
    };
    let mut out = Vec::new();
    for entry in list {
        let (Some(app_name), Some(install_path)) = (
            str_field(entry, "appName"),
            str_field(entry, "install_path"),
        ) else {
            continue;
        };
        out.push(HeroicGogInstall {
            app_name: app_name.to_string(),
            install_path: install_path.to_string(),
            is_dlc: entry.get("is_dlc").and_then(Value::as_bool) == Some(true),
            platform: str_field(entry, "platform").map(str::to_string),
        });
    }
    Ok(out)
}

/// Installed GOG games from Heroic. Same ids (GOG product ids) as the Windows registry.
pub fn gog_games(paths: &HeroicPaths) -> Result<Vec<ImportedGame>, ProviderError> {
    if paths.config_dirs.is_empty() {
        return Err(ProviderError::NotFound(
            "Heroic Games Launcher was not found; on Linux GOG games are read from Heroic".into(),
        ));
    }
    let mut games: Vec<ImportedGame> = Vec::new();
    for dir in &paths.config_dirs {
        let file = dir.join("gog_store").join("installed.json");
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!("ignoring unreadable {}: {e}", file.display());
                continue;
            }
        };
        let installs = match parse_gog_installed(&text) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("ignoring malformed {}: {e}", file.display());
                continue;
            }
        };
        let mut titles: HashMap<String, String> = HashMap::new();
        for lib in [
            dir.join("store_cache").join("gog_library.json"),
            dir.join("gog_store").join("library.json"),
        ] {
            if let Ok(text) = std::fs::read_to_string(&lib) {
                for (k, v) in parse_gog_library_titles(&text) {
                    titles.entry(k).or_insert(v);
                }
            }
        }
        for install in installs {
            if install.is_dlc
                || install
                    .platform
                    .as_deref()
                    .is_some_and(|p| matches!(p.to_ascii_lowercase().as_str(), "osx" | "mac"))
            {
                continue;
            }
            if games.iter().any(|g| g.source_game_id == install.app_name) {
                continue;
            }
            // Sideloaded/imported entries can point at the executable instead of the folder.
            let mut path = PathBuf::from(&install.install_path);
            if path.is_file()
                && let Some(parent) = path.parent()
            {
                path = parent.to_path_buf();
            }
            if !path.is_dir() {
                tracing::warn!(
                    "GOG game {} has no existing install folder, skipped",
                    install.app_name
                );
                continue;
            }
            let dir_str = path.display().to_string();
            let name = titles
                .get(&install.app_name)
                .cloned()
                .or_else(|| dir_name(&dir_str))
                .unwrap_or_else(|| install.app_name.clone());
            let mut game = ImportedGame::installed(&install.app_name, name);
            game.install_dir = Some(dir_str.clone());
            game.play_action = Some(heroic_action("gog", &install.app_name, &dir_str));
            games.push(game);
        }
    }
    games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
    Ok(games)
}

// ---- Heroic's cached store libraries (owned games) -------------------------------------------
//
// Heroic caches each store's library as `<config>/store_cache/<name>.json` (a flat key-value
// file; the list is under one key): `legendary_library.json` -> `"library"`, `gog_library.json` ->
// `"games"`, `nile_library.json` -> `"library"`. The entries are Heroic's unified `GameInfo`
// (`app_name`, `title`, `art_cover`, `art_square`, `is_installed`, ...).

/// A game of one of Heroic's cached store libraries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    pub id: String,
    pub title: String,
    /// Cover image URL (Heroic's `art_cover`, else `art_square`).
    pub cover: Option<String>,
}

/// Parses a cache file's `GameInfo` list: `{"<key>": [...]}` or a bare array. Entries without
/// `app_name` or `title` are dropped.
pub fn parse_library_entries(json: &str, key: &str) -> Vec<LibraryEntry> {
    let Ok(value) = serde_json::from_str::<Value>(json.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    let list = value.get(key).unwrap_or(&value);
    let mut out = Vec::new();
    for entry in list.as_array().into_iter().flatten() {
        let (Some(id), Some(title)) = (str_field(entry, "app_name"), str_field(entry, "title"))
        else {
            continue;
        };
        out.push(LibraryEntry {
            id: id.to_string(),
            title: title.to_string(),
            cover: str_field(entry, "art_cover")
                .or_else(|| str_field(entry, "art_square"))
                .map(str::to_string),
        });
    }
    out
}

/// The entries of `store_cache/<name>.json` over all Heroic config dirs (first one wins per id).
fn cached_library(paths: &HeroicPaths, name: &str, key: &str) -> Vec<LibraryEntry> {
    let mut out: Vec<LibraryEntry> = Vec::new();
    for dir in &paths.config_dirs {
        let Ok(text) = std::fs::read_to_string(dir.join("store_cache").join(name)) else {
            continue;
        };
        for e in parse_library_entries(&text, key) {
            if !out.iter().any(|o| o.id == e.id) {
                out.push(e);
            }
        }
    }
    out
}

/// Owned games from `entries` that are not in `installed`, as `installed: false` games.
fn merge_owned(mut installed: Vec<ImportedGame>, entries: Vec<LibraryEntry>) -> Vec<ImportedGame> {
    for e in entries {
        if installed
            .iter()
            .any(|g| g.source_game_id.eq_ignore_ascii_case(&e.id))
        {
            continue;
        }
        let mut game = ImportedGame::installed(&e.id, &e.title);
        game.installed = false;
        installed.push(game);
    }
    installed.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
    installed
}

/// Whether Heroic has a cached Epic library.
pub fn has_legendary_library(paths: &HeroicPaths) -> bool {
    !cached_library(paths, "legendary_library.json", "library").is_empty()
}

/// Whether Heroic has a cached GOG library.
pub fn has_gog_library(paths: &HeroicPaths) -> bool {
    !cached_library(paths, "gog_library.json", "games").is_empty()
}

/// All Epic games Heroic knows (its cached library): installed ones come from
/// [`legendary_games`] (so they keep their play action), the rest have `installed: false`. Ids
/// are Epic `AppName`s, like the installed games and the Epic API scan.
pub fn legendary_owned_games(paths: &HeroicPaths) -> Result<Vec<ImportedGame>, ProviderError> {
    let installed = legendary_games(paths)?;
    let entries = cached_library(paths, "legendary_library.json", "library");
    Ok(merge_owned(installed, entries))
}

/// All GOG games Heroic knows, see [`legendary_owned_games`]. Ids are GOG product ids. Heroic's
/// pseudo entry for the GOG redistributables (`gog-redist`) and other non-numeric ids are skipped.
pub fn gog_owned_games(paths: &HeroicPaths) -> Result<Vec<ImportedGame>, ProviderError> {
    let installed = gog_games(paths)?;
    let mut entries = cached_library(paths, "gog_library.json", "games");
    entries.retain(|e| !e.id.is_empty() && e.id.bytes().all(|b| b.is_ascii_digit()));
    Ok(merge_owned(installed, entries))
}

// ---- Amazon Games (Nile) ---------------------------------------------------------------------
//
// Heroic's Nile runner keeps `nile_config/nile/installed.json` (a list of `{id, version, path}`),
// `nile_config/nile/library.json` (Nile's raw library: a list of `{id, product: {id, title,
// productDetail: {iconUrl, details: {backgroundUrl1, backgroundUrl2, logoUrl}}}}`) and the unified
// cache `store_cache/nile_library.json` (`{"library": [GameInfo]}`). The game id everywhere is the
// Amazon product id (`product.id`, Heroic's `app_name`).

/// An entry of Nile's `installed.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NileInstall {
    pub id: String,
    pub path: String,
}

/// Parses Nile's `installed.json` (a list, or `{"installed": [...]}`). Entries without id or path
/// are dropped.
pub fn parse_nile_installed(json: &str) -> Result<Vec<NileInstall>, String> {
    let value: Value =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let list = value.get("installed").unwrap_or(&value);
    let Some(list) = list.as_array() else {
        return Err("expected a JSON list".into());
    };
    Ok(list
        .iter()
        .filter_map(|e| {
            Some(NileInstall {
                id: str_field(e, "id")?.to_string(),
                path: str_field(e, "path")?.to_string(),
            })
        })
        .collect())
}

/// Parses Nile's raw `library.json`. The cover mirrors Heroic's `art_cover`
/// (`backgroundUrl2`, else the icon).
pub fn parse_nile_library(json: &str) -> Vec<LibraryEntry> {
    let Ok(value) = serde_json::from_str::<Value>(json.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in value.as_array().into_iter().flatten() {
        let product = entry.get("product").unwrap_or(entry);
        let (Some(id), Some(title)) = (str_field(product, "id"), str_field(product, "title"))
        else {
            continue;
        };
        let detail = product.get("productDetail");
        let cover = detail
            .and_then(|d| d.get("details"))
            .and_then(|d| str_field(d, "backgroundUrl2"))
            .or_else(|| detail.and_then(|d| str_field(d, "iconUrl")));
        out.push(LibraryEntry {
            id: id.to_string(),
            title: title.to_string(),
            cover: cover.map(str::to_string),
        });
    }
    out
}

/// Heroic's cached Amazon library, falling back to Nile's raw `library.json`.
pub fn nile_library(paths: &HeroicPaths) -> Vec<LibraryEntry> {
    let mut out = cached_library(paths, "nile_library.json", "library");
    for dir in &paths.config_dirs {
        let file = dir.join("nile_config").join("nile").join("library.json");
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for e in parse_nile_library(&text) {
            if !out.iter().any(|o| o.id == e.id) {
                out.push(e);
            }
        }
    }
    out
}

/// Installed Amazon games from Heroic (Nile). With `owned`, the library's other games follow with
/// `installed: false`.
pub fn nile_games(paths: &HeroicPaths, owned: bool) -> Result<Vec<ImportedGame>, ProviderError> {
    if paths.config_dirs.is_empty() {
        return Err(ProviderError::NotFound(
            "Heroic Games Launcher was not found; Amazon games are read from Heroic".into(),
        ));
    }
    let library = nile_library(paths);
    let mut games: Vec<ImportedGame> = Vec::new();
    for dir in &paths.config_dirs {
        let file = dir.join("nile_config").join("nile").join("installed.json");
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!("ignoring unreadable {}: {e}", file.display());
                continue;
            }
        };
        let installs = match parse_nile_installed(&text) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("ignoring malformed {}: {e}", file.display());
                continue;
            }
        };
        for install in installs {
            if games.iter().any(|g| g.source_game_id == install.id) {
                continue;
            }
            if !Path::new(&install.path).is_dir() {
                tracing::warn!(
                    "Amazon game {} has no existing install folder, skipped",
                    install.id
                );
                continue;
            }
            let name = library
                .iter()
                .find(|e| e.id == install.id)
                .map(|e| e.title.clone())
                .or_else(|| dir_name(&install.path))
                .unwrap_or_else(|| install.id.clone());
            let mut game = ImportedGame::installed(&install.id, name);
            game.install_dir = Some(install.path.clone());
            game.play_action = Some(heroic_action("nile", &install.id, &install.path));
            games.push(game);
        }
    }
    if owned {
        return Ok(merge_owned(games, library));
    }
    games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
    Ok(games)
}

// ---- Sideloaded apps -------------------------------------------------------------------------
//
// Heroic's "Add a game" entries live in `sideload_apps/library.json` as `{"games": [{"runner":
// "sideload", "app_name", "title", "install": {"executable", "platform"}, "folder_name",
// "art_cover", "art_square", "is_installed", "browserUrl", ...}]}`. `platform` is `Windows`,
// `linux`, `Mac` or `Browser`.

/// A sideloaded Heroic app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideloadApp {
    pub app_name: String,
    pub title: String,
    pub executable: Option<String>,
    pub platform: Option<String>,
    pub installed: bool,
    pub is_browser: bool,
    /// Heroic's `art_cover`: a URL or a local path (optionally `file://`).
    pub cover: Option<String>,
}

pub fn parse_sideload_library(json: &str) -> Result<Vec<SideloadApp>, String> {
    let value: Value =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let Some(list) = value.get("games").and_then(Value::as_array) else {
        return Err("missing \"games\" list".into());
    };
    let mut out = Vec::new();
    for entry in list {
        let (Some(app_name), Some(title)) =
            (str_field(entry, "app_name"), str_field(entry, "title"))
        else {
            continue;
        };
        let install = entry.get("install");
        let platform = install
            .and_then(|i| str_field(i, "platform"))
            .map(str::to_string);
        out.push(SideloadApp {
            app_name: app_name.to_string(),
            title: title.to_string(),
            executable: install
                .and_then(|i| str_field(i, "executable"))
                .map(str::to_string),
            installed: entry.get("is_installed").and_then(Value::as_bool) != Some(false),
            is_browser: str_field(entry, "browserUrl").is_some()
                || platform
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case("browser")),
            platform,
            cover: str_field(entry, "art_cover")
                .or_else(|| str_field(entry, "art_square"))
                .map(str::to_string),
        });
    }
    Ok(out)
}

/// All sideloaded apps over the Heroic config dirs (first one wins per app name).
pub fn sideload_apps(paths: &HeroicPaths) -> Vec<SideloadApp> {
    let mut out: Vec<SideloadApp> = Vec::new();
    for dir in &paths.config_dirs {
        let file = dir.join("sideload_apps").join("library.json");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        match parse_sideload_library(&text) {
            Ok(list) => {
                for app in list {
                    if !out.iter().any(|o| o.app_name == app.app_name) {
                        out.push(app);
                    }
                }
            }
            Err(e) => tracing::warn!("ignoring malformed {}: {e}", file.display()),
        }
    }
    out
}

/// Sideloaded games that exist on disk. Browser apps and macOS apps have nothing to track or run
/// here and are skipped. A folder that would match every process (`/`, the home folder) is not
/// used for tracking.
pub fn sideload_games(paths: &HeroicPaths) -> Result<Vec<ImportedGame>, ProviderError> {
    if paths.config_dirs.is_empty() {
        return Err(ProviderError::NotFound(
            "Heroic Games Launcher was not found".into(),
        ));
    }
    let home = dirs::home_dir();
    let mut games = Vec::new();
    for app in sideload_apps(paths) {
        let is_mac = app
            .platform
            .as_deref()
            .is_some_and(|p| p.eq_ignore_ascii_case("mac"));
        if !app.installed || app.is_browser || is_mac {
            continue;
        }
        let Some(exe) = app.executable.as_deref().filter(|e| Path::new(e).is_file()) else {
            tracing::warn!(
                "sideloaded game {} has no existing executable, skipped",
                app.app_name
            );
            continue;
        };
        let dir = Path::new(exe)
            .parent()
            .filter(|p| p.parent().is_some() && home.as_deref() != Some(*p))
            .map(|p| p.display().to_string());
        let mut game = ImportedGame::installed(&app.app_name, &app.title);
        game.install_dir = dir.clone();
        let mut action = PlayAction::uri(launch_uri("sideload", &app.app_name));
        if let Some(dir) = dir {
            action.tracking = TrackingMode::Directory;
            action.tracking_path = Some(dir);
        }
        game.play_action = Some(action);
        games.push(game);
    }
    games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
    Ok(games)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::ActionKind;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn legendary_json(entries: &[(&str, &str, &Path, bool, &str)]) -> String {
        let mut map = serde_json::Map::new();
        for (app, title, dir, dlc, platform) in entries {
            map.insert(
                (*app).into(),
                serde_json::json!({
                    "app_name": app, "title": title, "install_path": dir,
                    "is_dlc": dlc, "platform": platform, "version": "1.0",
                    "executable": "Game.exe", "install_size": 5
                }),
            );
        }
        Value::Object(map).to_string()
    }

    #[test]
    fn discovers_existing_dirs_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let xdg = home.join("xdgcfg");
        fs::create_dir_all(xdg.join("heroic/legendaryConfig/legendary")).unwrap();
        fs::create_dir_all(home.join(".config/heroic")).unwrap();
        fs::create_dir_all(home.join(".config/legendary")).unwrap();
        fs::create_dir_all(home.join(HEROIC_FLATPAK)).unwrap();
        let bin_dir = home.join("bin");
        write(&bin_dir.join("legendary"), "");
        let path = std::env::join_paths([home.join("nope"), bin_dir.clone()]).unwrap();

        let p = HeroicPaths::discover(home, Some(&xdg), Some(&path));
        assert_eq!(
            p.config_dirs,
            vec![
                xdg.join("heroic"),
                home.join(".config/heroic"),
                home.join(HEROIC_FLATPAK)
            ]
        );
        assert_eq!(
            p.legendary,
            vec![
                LegendarySource {
                    dir: xdg.join("heroic/legendaryConfig/legendary"),
                    via_heroic: true
                },
                LegendarySource {
                    dir: home.join(".config/legendary"),
                    via_heroic: false
                },
            ]
        );
        assert_eq!(p.legendary_bin, Some(bin_dir.join("legendary")));
    }

    #[test]
    fn discovery_in_empty_home_finds_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let p = HeroicPaths::discover(tmp.path(), None, None);
        assert_eq!(p, HeroicPaths::default());
        assert!(matches!(
            legendary_games(&p),
            Err(ProviderError::NotFound(_))
        ));
        assert!(matches!(gog_games(&p), Err(ProviderError::NotFound(_))));
    }

    #[test]
    fn launch_uri_format() {
        assert_eq!(
            launch_uri("legendary", "Sugar"),
            "heroic://launch/legendary/Sugar"
        );
        assert_eq!(launch_uri("gog", "12 3"), "heroic://launch/gog/12%203");
    }

    #[test]
    fn parses_legendary_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let json = legendary_json(&[("Sugar", "Sugar Game", tmp.path(), false, "Windows")]);
        let list = parse_legendary_installed(&json).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].app_name, "Sugar");
        assert_eq!(list[0].title.as_deref(), Some("Sugar Game"));
        assert!(!list[0].is_dlc);
        // Key is used when app_name is missing; entries without a path are dropped.
        let list = parse_legendary_installed(
            r#"{"K": {"install_path": "/x"}, "NoPath": {"app_name": "NoPath"}, "Bad": 5}"#,
        )
        .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].app_name, "K");
        assert!(parse_legendary_installed("[]").is_err());
        assert!(parse_legendary_installed("{ nope").is_err());
    }

    #[test]
    fn legendary_games_via_heroic_and_standalone() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let a = home.join("games/A");
        let b = home.join("games/B");
        let c = home.join("games/C");
        let dup = home.join("games/Dup");
        for d in [&a, &b, &c, &dup] {
            fs::create_dir_all(d).unwrap();
        }
        write(
            &home.join(".config/heroic/legendaryConfig/legendary/installed.json"),
            &legendary_json(&[
                ("A", "Alpha", &a, false, "Windows"),
                ("Dlc", "Some DLC", &a, true, "Windows"),
                ("MacOnly", "Mac", &b, false, "Mac"),
                ("Gone", "Gone", &home.join("missing"), false, "Windows"),
                ("Dup", "Dup Heroic", &dup, false, "Windows"),
            ]),
        );
        write(
            &home.join(".config/legendary/installed.json"),
            &legendary_json(&[
                ("C", "charlie", &c, false, "Windows"),
                ("Dup", "Dup Standalone", &dup, false, "Windows"),
            ]),
        );
        write(&home.join("bin/legendary"), "");
        let path = std::env::join_paths([home.join("bin")]).unwrap();
        let paths = HeroicPaths::discover(home, None, Some(&path));

        let games = legendary_games(&paths).unwrap();
        let ids: Vec<_> = games.iter().map(|g| g.source_game_id.as_str()).collect();
        assert_eq!(ids, ["A", "C", "Dup"]);
        let dup_game = games.iter().find(|g| g.source_game_id == "Dup").unwrap();
        assert_eq!(dup_game.name, "Dup Heroic");

        let alpha = &games[0];
        assert_eq!(alpha.name, "Alpha");
        let action = alpha.play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(action.target, "heroic://launch/legendary/A");
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(
            action.tracking_path.as_deref(),
            alpha.install_dir.as_deref()
        );

        let charlie = &games[1];
        let action = charlie.play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::File);
        assert_eq!(action.args.as_deref(), Some("launch C"));
        assert_eq!(
            PathBuf::from(&action.target),
            home.join("bin").join("legendary")
        );
        assert_eq!(action.tracking, TrackingMode::Directory);
    }

    #[test]
    fn standalone_legendary_without_binary_has_no_action() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let c = home.join("C");
        fs::create_dir_all(&c).unwrap();
        write(
            &home.join(".config/legendary/installed.json"),
            &legendary_json(&[("C", "C", &c, false, "Windows")]),
        );
        let games = legendary_games(&HeroicPaths::discover(home, None, None)).unwrap();
        assert_eq!(games.len(), 1);
        assert!(games[0].play_action.is_none());
    }

    #[test]
    fn malformed_legendary_files_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        write(
            &home.join(".config/heroic/legendaryConfig/legendary/installed.json"),
            "not json",
        );
        let paths = HeroicPaths::discover(home, None, None);
        assert!(legendary_games(&paths).unwrap().is_empty());
        // Heroic present but no Legendary data at all: empty, not an error.
        let tmp2 = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp2.path().join(".config/heroic")).unwrap();
        let paths = HeroicPaths::discover(tmp2.path(), None, None);
        assert!(legendary_games(&paths).unwrap().is_empty());
        assert!(gog_games(&paths).unwrap().is_empty());
    }

    #[test]
    fn parses_gog_files() {
        let list = parse_gog_installed(
            r#"{"installed": [
                {"appName": "1207658924", "install_path": "/g/Game", "platform": "windows", "is_dlc": false},
                {"appName": "", "install_path": "/x"},
                {"install_path": "/y"},
                {"appName": "7", "install_path": "/z", "is_dlc": true}
            ]}"#,
        )
        .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].app_name, "1207658924");
        assert!(list[1].is_dlc);
        assert!(parse_gog_installed("{}").is_err());
        assert!(parse_gog_installed("garbage").is_err());

        let titles = parse_gog_library_titles(
            r#"{"games": [{"app_name": "1", "title": "One", "runner": "gog"}, {"app_name": "2"}, 3]}"#,
        );
        assert_eq!(titles.len(), 1);
        assert_eq!(titles["1"], "One");
        assert_eq!(
            parse_gog_library_titles(r#"[{"app_name": "9", "title": "Nine"}]"#)["9"],
            "Nine"
        );
        assert!(parse_gog_library_titles("nope").is_empty());
    }

    #[test]
    fn gog_games_from_heroic() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let hero = home.join(".config/heroic");
        let g1 = home.join("Games/Witcher");
        let g2 = home.join("Games/Untitled");
        let g3 = home.join("Games/Native");
        let mac = home.join("Games/Mac");
        for d in [&g1, &g2, &g3, &mac] {
            fs::create_dir_all(d).unwrap();
        }
        let exe = g2.join("start.exe");
        fs::write(&exe, "").unwrap();
        write(
            &hero.join("gog_store/installed.json"),
            &serde_json::json!({"installed": [
                {"appName": "1", "install_path": g1, "platform": "windows", "is_dlc": false},
                {"appName": "2", "install_path": exe, "platform": "windows"},
                {"appName": "3", "install_path": g3, "platform": "linux"},
                {"appName": "4", "install_path": mac, "platform": "osx"},
                {"appName": "5", "install_path": g1, "is_dlc": true},
                {"appName": "6", "install_path": home.join("gone")}
            ]})
            .to_string(),
        );
        write(
            &hero.join("store_cache/gog_library.json"),
            r#"{"games": [{"app_name": "1", "title": "The Witcher"}]}"#,
        );
        let games = gog_games(&HeroicPaths::discover(home, None, None)).unwrap();
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Native", "The Witcher", "Untitled"]);
        let w = games.iter().find(|g| g.source_game_id == "1").unwrap();
        let action = w.play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(action.target, "heroic://launch/gog/1");
        assert_eq!(action.tracking, TrackingMode::Directory);
        let u = games.iter().find(|g| g.source_game_id == "2").unwrap();
        assert_eq!(
            u.install_dir.as_deref(),
            Some(g2.display().to_string().as_str())
        );
    }

    #[test]
    fn owned_epic_and_gog_merge_with_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let hero = home.join(".config/heroic");
        let a = home.join("Games/A");
        let w = home.join("Games/W");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&w).unwrap();
        write(
            &hero.join("legendaryConfig/legendary/installed.json"),
            &legendary_json(&[("Aaa", "Alpha", &a, false, "Windows")]),
        );
        write(
            &hero.join("store_cache/legendary_library.json"),
            r#"{"library": [
                {"app_name": "Aaa", "title": "Alpha (cached)", "runner": "legendary", "is_installed": true},
                {"app_name": "Bbb", "title": "Beta", "art_cover": "https://x/b.jpg", "runner": "legendary"},
                {"app_name": "NoTitle"}
            ], "__timestamp.library": "Mon"}"#,
        );
        write(
            &hero.join("gog_store/installed.json"),
            &serde_json::json!({"installed": [{"appName": "1", "install_path": w}]}).to_string(),
        );
        write(
            &hero.join("store_cache/gog_library.json"),
            r#"{"games": [
                {"app_name": "1", "title": "Witcher"},
                {"app_name": "2", "title": "Cyberpunk", "art_square": "https://x/c.jpg"},
                {"app_name": "gog-redist", "title": "Redistributables", "is_installed": true}
            ]}"#,
        );
        let paths = HeroicPaths::discover(home, None, None);
        assert!(has_legendary_library(&paths) && has_gog_library(&paths));

        let epic = legendary_owned_games(&paths).unwrap();
        let view: Vec<_> = epic
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str(), g.installed))
            .collect();
        assert_eq!(view, [("Aaa", "Alpha", true), ("Bbb", "Beta", false)]);
        assert!(epic[0].play_action.is_some() && epic[1].play_action.is_none());

        let gog = gog_owned_games(&paths).unwrap();
        let view: Vec<_> = gog
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.installed))
            .collect();
        assert_eq!(view, [("2", false), ("1", true)]);
        assert_eq!(gog[1].name, "Witcher");

        let empty = tempfile::tempdir().unwrap();
        let none = HeroicPaths::discover(empty.path(), None, None);
        assert!(!has_gog_library(&none));
        assert!(matches!(
            gog_owned_games(&none),
            Err(ProviderError::NotFound(_))
        ));
    }

    #[test]
    fn parses_nile_files() {
        let list = parse_nile_installed(
            r#"[{"id": "amzn1.adg.product.1", "version": "v1", "path": "/g/A"},
                {"id": "x"}, {"path": "/y"}]"#,
        )
        .unwrap();
        assert_eq!(
            list,
            vec![NileInstall {
                id: "amzn1.adg.product.1".into(),
                path: "/g/A".into()
            }]
        );
        assert_eq!(
            parse_nile_installed(r#"{"installed": []}"#).unwrap(),
            vec![]
        );
        assert!(parse_nile_installed("{}").is_err());
        assert!(parse_nile_installed("nope").is_err());

        let lib = parse_nile_library(
            r#"[{"id": "e1", "product": {"id": "amzn1.adg.product.1", "title": "Fez",
                  "productDetail": {"iconUrl": "https://i/icon.png",
                  "details": {"backgroundUrl2": "https://i/bg2.jpg", "logoUrl": "https://i/l.png"}}}},
                {"id": "e2", "product": {"id": "amzn1.adg.product.2", "title": "Icon only",
                  "productDetail": {"iconUrl": "https://i/i2.png"}}},
                {"id": "e3", "product": {"title": "no product id"}},
                {"product": {"id": "p4"}}]"#,
        );
        assert_eq!(lib.len(), 2);
        assert_eq!(lib[0].cover.as_deref(), Some("https://i/bg2.jpg"));
        assert_eq!(lib[1].cover.as_deref(), Some("https://i/i2.png"));
        assert_eq!(lib[1].title, "Icon only");
    }

    #[test]
    fn nile_installed_and_owned_games() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let hero = home.join(".config/heroic");
        let fez = home.join("Games/Fez");
        fs::create_dir_all(&fez).unwrap();
        write(
            &hero.join("nile_config/nile/installed.json"),
            &serde_json::json!([
                {"id": "amzn1.adg.product.1", "version": "v1", "path": fez},
                {"id": "amzn1.adg.product.gone", "version": "v1", "path": home.join("gone")}
            ])
            .to_string(),
        );
        write(
            &hero.join("store_cache/nile_library.json"),
            r#"{"library": [
                {"app_name": "amzn1.adg.product.1", "title": "Fez", "art_cover": "https://i/fez.jpg"},
                {"app_name": "amzn1.adg.product.2", "title": "Ball", "art_square": "https://i/ball.png"}
            ]}"#,
        );
        // Nile's own library only adds games the cache lacks.
        write(
            &hero.join("nile_config/nile/library.json"),
            r#"[{"id": "e", "product": {"id": "amzn1.adg.product.3", "title": "Raw"}},
                {"id": "e", "product": {"id": "amzn1.adg.product.2", "title": "Ball (raw)"}}]"#,
        );
        let paths = HeroicPaths::discover(home, None, None);

        let installed = nile_games(&paths, false).unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].name, "Fez");
        let action = installed[0].play_action.as_ref().unwrap();
        assert_eq!(action.target, "heroic://launch/nile/amzn1.adg.product.1");
        assert_eq!(action.tracking, TrackingMode::Directory);

        let owned = nile_games(&paths, true).unwrap();
        let view: Vec<_> = owned
            .iter()
            .map(|g| (g.name.as_str(), g.installed))
            .collect();
        assert_eq!(view, [("Ball", false), ("Fez", true), ("Raw", false)]);
        assert_eq!(
            nile_library(&paths)
                .iter()
                .find(|e| e.id.ends_with(".2"))
                .and_then(|e| e.cover.as_deref()),
            Some("https://i/ball.png")
        );

        let empty = tempfile::tempdir().unwrap();
        assert!(matches!(
            nile_games(&HeroicPaths::discover(empty.path(), None, None), false),
            Err(ProviderError::NotFound(_))
        ));
    }

    #[test]
    fn sideloaded_games() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let hero = home.join(".config/heroic");
        let dir = home.join("Games/Indie");
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("indie.exe");
        fs::write(&exe, "").unwrap();
        write(
            &hero.join("sideload_apps/library.json"),
            &serde_json::json!({"games": [
                {"runner": "sideload", "app_name": "abc123", "title": "Indie",
                 "install": {"executable": exe, "platform": "Windows"}, "is_installed": true,
                 "art_cover": "file:///covers/indie.png"},
                {"runner": "sideload", "app_name": "web", "title": "Web",
                 "install": {"platform": "Browser"}, "browserUrl": "https://x", "is_installed": true},
                {"runner": "sideload", "app_name": "mac", "title": "Mac",
                 "install": {"executable": exe, "platform": "Mac"}},
                {"runner": "sideload", "app_name": "gone", "title": "Gone",
                 "install": {"executable": home.join("nope.exe"), "platform": "linux"}},
                {"runner": "sideload", "app_name": "off", "title": "Off",
                 "install": {"executable": exe}, "is_installed": false},
                {"title": "no id"}
            ]})
            .to_string(),
        );
        let paths = HeroicPaths::discover(home, None, None);
        let games = sideload_games(&paths).unwrap();
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert_eq!(
            (g.source_game_id.as_str(), g.name.as_str()),
            ("abc123", "Indie")
        );
        assert_eq!(
            g.install_dir.as_deref(),
            Some(dir.display().to_string().as_str())
        );
        let action = g.play_action.as_ref().unwrap();
        assert_eq!(action.target, "heroic://launch/sideload/abc123");
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(
            sideload_apps(&paths)[0].cover.as_deref(),
            Some("file:///covers/indie.png")
        );
        assert!(parse_sideload_library("{}").is_err());
        assert!(parse_sideload_library("x").is_err());
        let empty = tempfile::tempdir().unwrap();
        assert!(matches!(
            sideload_games(&HeroicPaths::discover(empty.path(), None, None)),
            Err(ProviderError::NotFound(_))
        ));
    }
}
