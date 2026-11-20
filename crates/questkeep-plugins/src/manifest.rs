//! `plugin.json`: what a plugin is and what it may reach.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{PluginError, Result};

/// Name of the manifest file in a plugin folder.
pub const MANIFEST_FILE: &str = "plugin.json";
/// The plugin API this version of Questkeep speaks.
pub const API_VERSION: u32 = 1;
/// Where the installed plugin folder is mounted for its stores.
pub const PLUGIN_MOUNT: &str = "/plugin";
/// Where the plugin's own data folder is mounted for its stores.
pub const DATA_MOUNT: &str = "/data";

/// A parsed and validated `plugin.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    /// Unique id: lowercase letters, digits and `-`, 2 to 64 characters.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// The plugin's icon: an `.svg` or `.png` inside the plugin folder (see [`read_icon`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub api_version: u32,
    /// The plugin's stores (WASM components).
    #[serde(default)]
    pub providers: Vec<ProviderEntry>,
    #[serde(default)]
    pub permissions: Permissions,
    /// The plugin's extension; kept as written for the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<serde_json::Value>,
    /// The plugin's themes; kept as written for the UI.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    /// The component, relative to the plugin folder.
    pub wasm: String,
}

/// What the plugin's stores may reach besides their own folders.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    /// Folders the stores may read.
    #[serde(default)]
    pub read: Vec<ReadMount>,
    /// Hosts the stores may send HTTP requests to: `api.example.com`, `*.example.com` (any
    /// subdomain) or `http://localhost` (plain http, otherwise only https is allowed).
    #[serde(default)]
    pub http: Vec<String>,
    /// Folders the user chooses: the plugin declares what it wants, the user decides where each
    /// points to, the stores see it read-only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<FolderPermission>,
}

/// One entry of `permissions.folders`: a folder the plugin asks the user for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPermission {
    /// Names the folder in `plugins.json`: 1 to 32 lowercase letters, digits and `-`.
    pub key: String,
    /// What the folder is, shown to the user.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Where the stores see it, e.g. `/instances`.
    #[serde(rename = "as")]
    pub mount: String,
    /// Used until the user chooses a folder; `%VAR%` and a leading `~` are expanded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadMount {
    /// A folder on this PC; `%VAR%` and a leading `~` are expanded.
    pub path: String,
    /// Where the stores see it, e.g. `/store`.
    #[serde(rename = "as")]
    pub mount: String,
}

impl PluginManifest {
    /// Reads and checks `plugin.json` in `dir`.
    pub fn load(dir: &Path) -> Result<Self> {
        let file = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&file).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                PluginError::Invalid(format!(
                    "This is not a plugin: there is no {MANIFEST_FILE} in {}.",
                    dir.display()
                ))
            } else {
                PluginError::io(&file, err)
            }
        })?;
        Self::parse(&text)
    }

    /// Parses and checks the text of a `plugin.json`.
    pub fn parse(text: &str) -> Result<Self> {
        // A UTF-8 byte order mark is what Windows editors like to add.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let manifest: PluginManifest = serde_json::from_str(text)
            .map_err(|e| PluginError::Invalid(format!("{MANIFEST_FILE} is not valid: {e}.")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        if !is_valid_id(&self.id) {
            return Err(invalid(format!(
                "The plugin id \"{}\" is not valid: use 2 to 64 lowercase letters, digits and \
                 dashes, starting with a letter or digit.",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(invalid("The plugin has no name."));
        }
        if self.version.trim().is_empty() {
            return Err(invalid("The plugin has no version."));
        }
        if self.api_version != API_VERSION {
            return Err(invalid(format!(
                "This plugin was made for plugin API version {}, but this version of Questkeep \
                 supports version {API_VERSION}.",
                self.api_version
            )));
        }
        for provider in &self.providers {
            check_relative(&provider.wasm, "The store file")?;
        }
        if let Some(icon) = &self.icon {
            check_icon_path(icon)?;
        }
        if let Some(main) = self
            .extension
            .as_ref()
            .and_then(|e| e.get("main"))
            .and_then(|m| m.as_str())
        {
            check_relative(main, "The extension file")?;
        }
        for css in self
            .themes
            .iter()
            .filter_map(|t| t.get("css").and_then(|c| c.as_str()))
        {
            check_relative(css, "The theme file")?;
        }
        let mut mounts: Vec<&str> = Vec::new();
        for read in &self.permissions.read {
            if read.path.trim().is_empty() {
                return Err(invalid("A folder permission has no path."));
            }
            take_mount(&mut mounts, &read.mount)?;
        }
        let mut keys: Vec<&str> = Vec::new();
        for folder in &self.permissions.folders {
            if !is_valid_folder_key(&folder.key) {
                return Err(invalid(format!(
                    "The folder key \"{}\" is not valid: use 1 to 32 lowercase letters, digits \
                     and dashes.",
                    folder.key
                )));
            }
            if keys.contains(&folder.key.as_str()) {
                return Err(invalid(format!(
                    "Two folders use the same key \"{}\".",
                    folder.key
                )));
            }
            keys.push(&folder.key);
            if folder.label.trim().is_empty() {
                return Err(invalid(format!(
                    "The folder \"{}\" has no label.",
                    folder.key
                )));
            }
            if folder
                .default
                .as_deref()
                .is_some_and(|d| d.trim().is_empty())
            {
                return Err(invalid(format!(
                    "The default of the folder \"{}\" is empty.",
                    folder.key
                )));
            }
            take_mount(&mut mounts, &folder.mount)?;
        }
        for host in &self.permissions.http {
            if HostRule::parse(host).is_none() {
                return Err(invalid(format!(
                    "The web permission \"{host}\" is not valid: use a host name such as \
                     api.example.com or *.example.com."
                )));
            }
        }
        Ok(())
    }

    /// The parsed web permissions.
    pub fn host_rules(&self) -> Vec<HostRule> {
        self.permissions
            .http
            .iter()
            .filter_map(|h| HostRule::parse(h))
            .collect()
    }
}

fn invalid(message: impl Into<String>) -> PluginError {
    PluginError::Invalid(message.into())
}

/// `^[a-z0-9][a-z0-9-]{1,63}$`
pub fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (2..=64).contains(&bytes.len())
        && bytes[0] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// Checks a guest path and notes it as used; two folders may not share one.
fn take_mount<'a>(taken: &mut Vec<&'a str>, mount: &'a str) -> Result<()> {
    check_mount(mount)?;
    if taken.contains(&mount) {
        return Err(invalid(format!(
            "Two folder permissions use the same name \"{mount}\"."
        )));
    }
    taken.push(mount);
    Ok(())
}

/// `^[a-z0-9-]{1,32}$`
pub fn is_valid_folder_key(key: &str) -> bool {
    (1..=32).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A path inside the plugin folder: relative, without `..`, drive or root.
fn check_relative(path: &str, what: &str) -> Result<()> {
    let inside = !path.trim().is_empty()
        && !path.contains(':')
        && !path.starts_with(['/', '\\'])
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        && !path.split(['/', '\\']).any(|part| part == "..");
    if inside {
        Ok(())
    } else {
        Err(invalid(format!(
            "{what} \"{path}\" must be a path inside the plugin folder."
        )))
    }
}

/// The largest icon file a plugin may have.
pub const MAX_ICON_BYTES: u64 = 200 * 1024;

/// The icon's path is inside the plugin folder and names an `.svg` or `.png`.
fn check_icon_path(icon: &str) -> Result<()> {
    check_relative(icon, "The icon file")?;
    let extension = icon_extension(icon);
    if extension != "svg" && extension != "png" {
        return Err(invalid(format!(
            "The icon file \"{icon}\" must be an .svg or .png image."
        )));
    }
    Ok(())
}

fn icon_extension(icon: &str) -> String {
    icon.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// A checked icon file: its media type and bytes.
pub type IconFile = (&'static str, Vec<u8>);

/// Reads the plugin's icon (`None` when the manifest names none) from the plugin folder `dir`
/// and checks it: the file exists, is at most [`MAX_ICON_BYTES`], a PNG really is one, and an SVG
/// is UTF-8 text without scripts, event handler attributes or `javascript:` links. The UI shows
/// it as an image only (never inlined), so the SVG check is a second line of defence.
pub fn read_icon(dir: &Path, manifest: &PluginManifest) -> Result<Option<IconFile>> {
    let Some(icon) = manifest.icon.as_deref() else {
        return Ok(None);
    };
    check_icon_path(icon)?;
    let file = dir.join(icon);
    let too_large = || {
        invalid(format!(
            "The plugin's icon file \"{icon}\" is larger than {} KB.",
            MAX_ICON_BYTES / 1024
        ))
    };
    let Some(meta) = std::fs::metadata(&file).ok().filter(|m| m.is_file()) else {
        return Err(invalid(format!(
            "The plugin's icon file \"{icon}\" is missing."
        )));
    };
    if meta.len() > MAX_ICON_BYTES {
        return Err(too_large());
    }
    let bytes = std::fs::read(&file).map_err(|e| PluginError::io(&file, e))?;
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return Err(too_large());
    }
    if icon_extension(icon) == "png" {
        if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
            return Err(invalid(format!(
                "The plugin's icon file \"{icon}\" is not a PNG image."
            )));
        }
        return Ok(Some(("image/png", bytes)));
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Err(invalid(format!(
            "The plugin's icon file \"{icon}\" is not UTF-8 text, so it is not a usable SVG."
        )));
    };
    if let Some(found) = unsafe_svg_part(text) {
        return Err(invalid(format!(
            "The plugin's icon file \"{icon}\" is not allowed: an SVG icon may not contain \
             {found}."
        )));
    }
    Ok(Some(("image/svg+xml", bytes)))
}

/// What in an SVG could run code, if anything: `<script`, `javascript:` or an `on<event>=`
/// attribute.
fn unsafe_svg_part(svg: &str) -> Option<&'static str> {
    let lower = svg.to_ascii_lowercase();
    if lower.contains("<script") {
        return Some("a script");
    }
    if lower.contains("javascript:") {
        return Some("a javascript: link");
    }
    let bytes = lower.as_bytes();
    for (i, _) in lower.match_indices("on") {
        let starts_name = i > 0
            && matches!(
                bytes[i - 1],
                b' ' | b'\t' | b'\r' | b'\n' | b'"' | b'\'' | b'/'
            );
        if !starts_name {
            continue;
        }
        let mut end = i + 2;
        while bytes.get(end).is_some_and(u8::is_ascii_lowercase) {
            end += 1;
        }
        if end == i + 2 {
            continue;
        }
        while bytes.get(end).is_some_and(u8::is_ascii_whitespace) {
            end += 1;
        }
        if bytes.get(end) == Some(&b'=') {
            return Some("an event handler attribute (on...=)");
        }
    }
    None
}

/// A guest path for a read mount: absolute, plain, and not one of the plugin's own folders.
fn check_mount(mount: &str) -> Result<()> {
    let parts: Vec<&str> = mount.split('/').collect();
    let plain = mount.starts_with('/')
        && parts.len() >= 2
        && parts[1..].iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && !p.contains(['\\', ':'])
                && !p.chars().any(char::is_control)
        });
    if !plain {
        return Err(invalid(format!(
            "The folder name \"{mount}\" is not valid: use an absolute path such as /store."
        )));
    }
    let reserved = [PLUGIN_MOUNT, DATA_MOUNT]
        .iter()
        .any(|r| mount == *r || mount.starts_with(&format!("{r}/")));
    if reserved {
        return Err(invalid(format!(
            "The folder name \"{mount}\" is taken: {PLUGIN_MOUNT} and {DATA_MOUNT} belong to \
             the plugin itself."
        )));
    }
    Ok(())
}

/// One entry of `permissions.http`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRule {
    /// Lowercase host name, without the `*.` of a subdomain rule.
    host: String,
    /// `*.host`: any subdomain (not the host itself).
    subdomains: bool,
    /// Declared as `http://host`: plain http is allowed too.
    plain_http: bool,
}

impl HostRule {
    pub fn parse(entry: &str) -> Option<Self> {
        let entry = entry.trim().to_ascii_lowercase();
        let (plain_http, rest) = match entry.strip_prefix("http://") {
            Some(rest) => (true, rest),
            None => (false, entry.strip_prefix("https://").unwrap_or(&entry)),
        };
        let (subdomains, host) = match rest.strip_prefix("*.") {
            Some(host) => (true, host),
            None => (false, rest),
        };
        let valid = !host.is_empty()
            && host.len() <= 253
            && host.split('.').all(|label| {
                !label.is_empty()
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            });
        valid.then(|| Self {
            host: host.to_string(),
            subdomains,
            plain_http,
        })
    }

    fn matches_host(&self, host: &str) -> bool {
        if self.subdomains {
            host.strip_suffix(&self.host)
                .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with('.'))
        } else {
            host == self.host
        }
    }
}

/// Whether a request with this scheme to this host is covered by the rules.
pub fn host_allowed(rules: &[HostRule], scheme: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    rules.iter().any(|rule| {
        rule.matches_host(host)
            && (scheme.eq_ignore_ascii_case("https")
                || (rule.plain_http && scheme.eq_ignore_ascii_case("http")))
    })
}

/// Expands `%VAR%` (any platform) and a leading `~` in a path from the manifest. `None` when a
/// variable is not set: the folder can't exist then.
pub fn expand_path(
    path: &str,
    var: impl Fn(&str) -> Option<String>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let mut out = String::new();
    let mut rest = path;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        if name.is_empty() {
            // `%%` is a literal percent sign.
            out.push('%');
        } else {
            out.push_str(&var(name)?);
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    if out == "~" {
        return home.map(Path::to_path_buf);
    }
    if let Some(below) = out.strip_prefix("~/").or_else(|| out.strip_prefix("~\\")) {
        return home.map(|h| h.join(below));
    }
    Some(PathBuf::from(out))
}

/// The folder a `permissions.folders` entry points to: the user's `chosen` path, else the
/// expanded default. `None` when there is neither (or the default can't be expanded).
pub fn folder_path(
    folder: &FolderPermission,
    chosen: Option<&str>,
    var: impl Fn(&str) -> Option<String>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    match chosen {
        Some(path) => Some(PathBuf::from(path)),
        // Rebuilt from its parts so the separators match the platform's (`~/x` becomes
        // `C:\Users\me\x`, not `C:\Users\me/x`).
        None => {
            expand_path(folder.default.as_deref()?, var, home).map(|p| p.components().collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(patch: impl Fn(&mut serde_json::Value)) -> Result<PluginManifest> {
        let mut json = serde_json::json!({
            "id": "example-store",
            "name": "Example Store",
            "version": "0.1.0",
            "apiVersion": 1,
            "providers": [{ "wasm": "provider.wasm" }],
            "permissions": {
                "read": [{ "path": "%LOCALAPPDATA%/Example", "as": "/store" }],
                "http": ["api.example.com", "*.example.org", "http://localhost"]
            }
        });
        patch(&mut json);
        PluginManifest::parse(&json.to_string())
    }

    fn message(result: Result<PluginManifest>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn parses_a_full_manifest_and_keeps_later_fields() {
        let parsed = manifest(|json| {
            json["description"] = "About".into();
            json["extension"] = serde_json::json!({ "main": "main.js", "settings": [1, 2] });
            json["themes"] = serde_json::json!([{ "id": "t", "css": "theme.css", "x": 1 }]);
        })
        .unwrap();
        assert_eq!(parsed.id, "example-store");
        assert_eq!(parsed.description.as_deref(), Some("About"));
        assert_eq!(parsed.providers[0].wasm, "provider.wasm");
        assert_eq!(parsed.permissions.read[0].mount, "/store");
        assert_eq!(parsed.extension.as_ref().unwrap()["settings"][1], 2);
        assert_eq!(parsed.themes[0]["x"], 1);
        // What was not understood survives a round trip.
        let again = PluginManifest::parse(&serde_json::to_string(&parsed).unwrap()).unwrap();
        assert_eq!(again, parsed);
    }

    #[test]
    fn minimal_manifest_and_bom() {
        let text = "\u{feff}{\"id\":\"ab\",\"name\":\"A\",\"version\":\"1\",\"apiVersion\":1}";
        let parsed = PluginManifest::parse(text).unwrap();
        assert!(parsed.providers.is_empty() && parsed.themes.is_empty());
        assert!(parsed.extension.is_none());
    }

    #[test]
    fn rejects_bad_ids() {
        for id in [
            "",
            "a",
            "-ab",
            "Abc",
            "a b",
            "a_b",
            "ä-store",
            "../x",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_id(id), "{id}");
            assert!(message(manifest(|j| j["id"] = id.into())).contains("plugin id"));
        }
        for id in ["ab", "0-store", "my-store-2", &"a".repeat(64)] {
            assert!(is_valid_id(id), "{id}");
        }
    }

    #[test]
    fn rejects_other_api_versions_readably() {
        let text = message(manifest(|j| j["apiVersion"] = 2.into()));
        assert!(text.contains("API version 2") && text.contains("supports version 1"));
        assert!(message(manifest(|j| j["name"] = " ".into())).contains("no name"));
        assert!(PluginManifest::parse("{ // comment\n}").is_err());
        assert!(message(PluginManifest::parse("[]")).contains("plugin.json is not valid"));
    }

    #[test]
    fn rejects_paths_that_leave_the_plugin_folder() {
        for path in [
            "../provider.wasm",
            "a/../../b.wasm",
            "a\\..\\..\\b.wasm",
            "/etc/provider.wasm",
            "\\\\server\\share\\p.wasm",
            "C:/p.wasm",
            "C:p.wasm",
            "",
        ] {
            let text = message(manifest(|j| j["providers"][0]["wasm"] = path.into()));
            assert!(text.contains("inside the plugin folder"), "{path}: {text}");
        }
        assert!(manifest(|j| j["providers"][0]["wasm"] = "bin/./store.wasm".into()).is_ok());
        let text = message(manifest(|j| {
            j["extension"] = serde_json::json!({ "main": "../main.js" });
        }));
        assert!(text.contains("extension file"));
        let text = message(manifest(|j| {
            j["themes"] = serde_json::json!([{ "css": "/theme.css" }]);
        }));
        assert!(text.contains("theme file"));
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];

    fn with_icon(icon: &str, files: &[(&str, &[u8])]) -> Result<Option<IconFile>> {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let parsed = manifest(|j| j["icon"] = icon.into())?;
        read_icon(tmp.path(), &parsed)
    }

    fn icon_error(icon: &str, files: &[(&str, &[u8])]) -> String {
        with_icon(icon, files).unwrap_err().to_string()
    }

    #[test]
    fn icon_is_optional_and_a_path_to_an_svg_or_png() {
        assert_eq!(manifest(|_| {}).unwrap().icon, None);
        let parsed = manifest(|j| j["icon"] = "img/Icon.SVG".into()).unwrap();
        assert_eq!(parsed.icon.as_deref(), Some("img/Icon.SVG"));
        assert!(manifest(|j| j["icon"] = "logo.png".into()).is_ok());
        let again = PluginManifest::parse(&serde_json::to_string(&parsed).unwrap()).unwrap();
        assert_eq!(again, parsed);
        let plain = serde_json::to_string(&manifest(|_| {}).unwrap()).unwrap();
        assert!(!plain.contains("icon"));
        for bad in ["logo.jpg", "logo", "logo.svg.exe", "icon.gif"] {
            let text = message(manifest(|j| j["icon"] = bad.into()));
            assert!(text.contains(".svg or .png"), "{bad}: {text}");
        }
        for bad in [
            "../icon.svg",
            "/icon.svg",
            "C:/icon.svg",
            "a/../../i.png",
            "",
        ] {
            let text = message(manifest(|j| j["icon"] = bad.into()));
            assert!(
                text.contains("icon file") && text.contains("inside the plugin folder"),
                "{bad}: {text}"
            );
        }
    }

    #[test]
    fn reads_and_checks_the_icon_file() {
        let svg =
            br#"<svg xmlns="http://www.w3.org/2000/svg"><rect data-one="1" width="1"/></svg>"#;
        let (kind, bytes) = with_icon("i.svg", &[("i.svg", svg)]).unwrap().unwrap();
        assert_eq!((kind, bytes.len()), ("image/svg+xml", svg.len()));
        let (kind, _) = with_icon("img/I.PNG", &[("img/I.PNG", PNG)])
            .unwrap()
            .unwrap();
        assert_eq!(kind, "image/png");
        // No icon, nothing to read.
        let tmp = tempfile::tempdir().unwrap();
        let plain = manifest(|_| {}).unwrap();
        assert!(read_icon(tmp.path(), &plain).unwrap().is_none());

        for files in [&[][..], &[("sub/i.svg", &svg[..])][..]] {
            assert!(icon_error("i.svg", files).contains("is missing"));
        }
        // A folder is not a file.
        assert!(icon_error("sub.png", &[("sub.png/x", b"1")]).contains("is missing"));
        let big = vec![b' '; MAX_ICON_BYTES as usize + 1];
        assert!(icon_error("i.png", &[("i.png", &big)]).contains("larger than 200 KB"));
        let mut exact = PNG.to_vec();
        exact.resize(MAX_ICON_BYTES as usize, 0);
        assert!(with_icon("i.png", &[("i.png", &exact)]).is_ok());
        assert!(icon_error("i.png", &[("i.png", b"GIF89a")]).contains("not a PNG"));
        assert!(icon_error("i.svg", &[("i.svg", &[0xff, 0xfe, b'<'])]).contains("not UTF-8"));
        assert!(icon_error("i.jpg", &[("i.jpg", b"x")]).contains(".svg or .png"));
    }

    #[test]
    fn an_svg_icon_may_not_run_code() {
        for (svg, what) in [
            ("<svg><script>alert(1)</script></svg>", "script"),
            ("<svg><SCRIPT src=x></SCRIPT></svg>", "script"),
            (
                "<svg><a href=\"javascript:alert(1)\"/></svg>",
                "javascript:",
            ),
            ("<svg><a href=\"JaVaScRiPt:x\"/></svg>", "javascript:"),
            ("<svg onload=\"x()\"></svg>", "event handler"),
            (
                "<svg><rect width='1' onClick = 'x()'/></svg>",
                "event handler",
            ),
            ("<svg\nonmouseover=\"x()\"></svg>", "event handler"),
            ("<svg><rect/onerror=x()></svg>", "event handler"),
        ] {
            let text = icon_error("i.svg", &[("i.svg", svg.as_bytes())]);
            assert!(
                text.contains("not allowed") && text.contains(what),
                "{svg}: {text}"
            );
        }
        // Words and attributes that only look similar are fine.
        for svg in [
            "<svg><text>one two on</text><rect data-one=\"1\" fill=\"none\"/></svg>",
            "<svg><text>turn on, then off</text></svg>",
        ] {
            assert!(
                with_icon("i.svg", &[("i.svg", svg.as_bytes())]).is_ok(),
                "{svg}"
            );
        }
    }

    #[test]
    fn rejects_bad_mounts() {
        for mount in [
            "store",
            "/",
            "",
            "/plugin",
            "/data",
            "/plugin/x",
            "/data/x",
            "/a/../b",
            "/a//b",
            "/a/",
            "/a\\b",
            "/c:",
        ] {
            let result = manifest(|j| j["permissions"]["read"][0]["as"] = mount.into());
            assert!(result.is_err(), "{mount}");
        }
        assert!(manifest(|j| j["permissions"]["read"][0]["as"] = "/stores/main".into()).is_ok());
        assert!(manifest(|j| j["permissions"]["read"][0]["as"] = "/plugins".into()).is_ok());
        let twice = manifest(|j| {
            j["permissions"]["read"] = serde_json::json!([
                { "path": "a", "as": "/x" }, { "path": "b", "as": "/x" }
            ]);
        });
        assert!(message(twice).contains("same name"));
    }

    fn folders(entries: serde_json::Value) -> Result<PluginManifest> {
        manifest(|j| j["permissions"]["folders"] = entries.clone())
    }

    #[test]
    fn parses_folders_the_user_chooses() {
        let parsed = folders(serde_json::json!([
            { "key": "instances", "label": "Instances", "hint": "Where they are",
              "as": "/instances", "default": "~/BSManager/BSInstances" },
            { "key": "a-1", "label": "Other", "as": "/other" }
        ]))
        .unwrap();
        let first = &parsed.permissions.folders[0];
        assert_eq!(first.key, "instances");
        assert_eq!(first.hint.as_deref(), Some("Where they are"));
        assert_eq!(first.mount, "/instances");
        assert_eq!(parsed.permissions.folders[1].default, None);
        let again = PluginManifest::parse(&serde_json::to_string(&parsed).unwrap()).unwrap();
        assert_eq!(again, parsed);
        // Without folders nothing is written.
        let plain = manifest(|_| {}).unwrap();
        assert!(plain.permissions.folders.is_empty());
        assert!(!serde_json::to_string(&plain).unwrap().contains("folders"));
    }

    #[test]
    fn rejects_bad_folders() {
        let one = |patch: serde_json::Value| {
            let mut base = serde_json::json!({ "key": "k", "label": "L", "as": "/k" });
            for (name, value) in patch.as_object().unwrap() {
                base[name] = value.clone();
            }
            folders(serde_json::json!([base]))
        };
        for key in ["", "Key", "a_b", "a b", "../x", &"a".repeat(33)] {
            let text = message(one(serde_json::json!({ "key": key })));
            assert!(text.contains("folder key"), "{key}: {text}");
        }
        assert!(one(serde_json::json!({ "key": "-" })).is_ok());
        assert!(one(serde_json::json!({ "key": "a".repeat(32) })).is_ok());
        assert!(message(one(serde_json::json!({ "label": " " }))).contains("no label"));
        assert!(message(one(serde_json::json!({ "default": " " }))).contains("default"));
        for mount in ["k", "/", "/plugin", "/data/x", "/a/../b"] {
            assert!(one(serde_json::json!({ "as": mount })).is_err(), "{mount}");
        }
        let twice = folders(serde_json::json!([
            { "key": "k", "label": "A", "as": "/a" },
            { "key": "k", "label": "B", "as": "/b" }
        ]));
        assert!(message(twice).contains("same key"));
        // The mount names are shared with `read`.
        let clash = folders(serde_json::json!([{ "key": "k", "label": "A", "as": "/store" }]));
        assert!(message(clash).contains("same name"));
        let clash = folders(serde_json::json!([
            { "key": "k", "label": "A", "as": "/a" },
            { "key": "j", "label": "B", "as": "/a" }
        ]));
        assert!(message(clash).contains("same name"));
    }

    #[test]
    fn a_folder_is_the_choice_or_the_expanded_default() {
        let parsed = folders(serde_json::json!([
            { "key": "k", "label": "L", "as": "/k", "default": "~/Games" },
            { "key": "n", "label": "N", "as": "/n" }
        ]))
        .unwrap();
        let var = |_: &str| None;
        let home = Path::new("/home/me");
        let [with_default, without] = &parsed.permissions.folders[..] else {
            panic!("two folders");
        };
        assert_eq!(
            folder_path(with_default, None, var, Some(home)),
            Some(home.join("Games"))
        );
        assert_eq!(
            folder_path(with_default, Some("/mine"), var, Some(home)),
            Some(PathBuf::from("/mine"))
        );
        // No `/` left in the middle of a Windows path.
        #[cfg(windows)]
        assert_eq!(
            folder_path(with_default, None, var, Some(home))
                .unwrap()
                .display()
                .to_string(),
            r"\home\me\Games"
        );
        assert_eq!(folder_path(with_default, None, var, None), None);
        assert_eq!(folder_path(without, None, var, Some(home)), None);
        assert_eq!(
            folder_path(without, Some("/x"), var, None),
            Some(PathBuf::from("/x"))
        );
    }

    #[test]
    fn web_permissions_match_hosts() {
        let rules = manifest(|_| {}).unwrap().host_rules();
        assert!(host_allowed(&rules, "https", "api.example.com"));
        assert!(host_allowed(&rules, "HTTPS", "API.Example.com."));
        assert!(!host_allowed(&rules, "http", "api.example.com"));
        assert!(!host_allowed(&rules, "https", "example.com"));
        assert!(!host_allowed(&rules, "https", "evil-api.example.com"));
        assert!(!host_allowed(&rules, "https", "api.example.com.evil.net"));
        // *.example.org: subdomains only.
        assert!(host_allowed(&rules, "https", "cdn.example.org"));
        assert!(host_allowed(&rules, "https", "a.b.example.org"));
        assert!(!host_allowed(&rules, "https", "example.org"));
        assert!(!host_allowed(&rules, "https", "badexample.org"));
        assert!(!host_allowed(&rules, "http", "cdn.example.org"));
        // http://localhost: both schemes.
        assert!(host_allowed(&rules, "http", "localhost"));
        assert!(host_allowed(&rules, "https", "localhost"));
        assert!(!host_allowed(&rules, "ftp", "localhost"));
        assert!(!host_allowed(&[], "https", "api.example.com"));
    }

    #[test]
    fn rejects_bad_web_permissions() {
        for host in [
            "",
            "*",
            "*.",
            "a b.com",
            "example.com/path",
            "example.com:8080",
            "-a.com",
            "a..b",
        ] {
            let result = manifest(|j| j["permissions"]["http"] = serde_json::json!([host]));
            assert!(result.is_err(), "{host}");
        }
    }

    #[test]
    fn expands_variables_and_home() {
        let var = |name: &str| match name {
            "LOCALAPPDATA" => Some("C:\\Users\\me\\AppData\\Local".to_string()),
            "X" => Some("x".to_string()),
            _ => None,
        };
        let home = Path::new("/home/me");
        assert_eq!(
            expand_path("%LOCALAPPDATA%/Store", var, Some(home)),
            Some(PathBuf::from("C:\\Users\\me\\AppData\\Local/Store"))
        );
        assert_eq!(
            expand_path("a/%X%/%X%.txt", var, None),
            Some(PathBuf::from("a/x/x.txt"))
        );
        assert_eq!(expand_path("%MISSING%/a", var, Some(home)), None);
        assert_eq!(
            expand_path("~/games", var, Some(home)),
            Some(home.join("games"))
        );
        assert_eq!(expand_path("~", var, Some(home)), Some(home.to_path_buf()));
        assert_eq!(expand_path("~/games", var, None), None);
        // Not a home reference, a lone percent sign, an escaped one.
        assert_eq!(
            expand_path("a/~/b", var, None),
            Some(PathBuf::from("a/~/b"))
        );
        assert_eq!(
            expand_path("50% off", var, None),
            Some(PathBuf::from("50% off"))
        );
        assert_eq!(expand_path("100%%", var, None), Some(PathBuf::from("100%")));
    }
}
