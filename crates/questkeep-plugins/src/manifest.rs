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
            check_mount(&read.mount)?;
            if mounts.contains(&read.mount.as_str()) {
                return Err(invalid(format!(
                    "Two folder permissions use the same name \"{}\".",
                    read.mount
                )));
            }
            mounts.push(&read.mount);
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
