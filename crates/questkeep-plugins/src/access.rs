//! What a plugin's stores may reach beyond their own folders, as the host provides it: the
//! whole file system (read-only), the registry, secrets in the keyring and the website session
//! windows. The manifest declares each part (`permissions.filesystem`, `registry`, `login`,
//! `launcher`); everything here only runs when it did.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use questkeep_store_api::auth::{
    CAPTURE_PREFIX, CaptureRule, KeyringTokenStore, LoginWindow, TokenStore, WebSession,
    WebSessionPage,
};
use questkeep_store_api::model::{ActionKind, LauncherClose, LauncherProcess};

use crate::error::{PluginError, Result};
use crate::manifest::{LoginPermission, PluginManifest};

/// Where the file system of this PC is mounted for stores that declared `permissions.filesystem`
/// (see the SDK's `HOST_ROOT`).
pub use questkeep_plugin_sdk::HOST_ROOT;

/// The longest secret a plugin may keep: the Windows credential store accepts 2560 bytes (stored
/// as UTF-16).
pub const MAX_SECRET_CHARS: usize = 1200;

/// The largest script file of a login window or website session.
const MAX_SCRIPT_BYTES: u64 = 256 * 1024;

/// The parts of the app that plugins use through host functions: the keyring and the hidden
/// windows of website sessions. The shell provides the real ones; the defaults are the system
/// keyring and no web session.
#[derive(Clone)]
pub struct PluginServices {
    pub secrets: Arc<dyn TokenStore>,
    pub web: Option<Arc<dyn WebSession>>,
}

impl Default for PluginServices {
    fn default() -> Self {
        Self {
            secrets: Arc::new(KeyringTokenStore::new()),
            web: None,
        }
    }
}

impl std::fmt::Debug for PluginServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginServices")
            .field("web", &self.web.is_some())
            .finish()
    }
}

/// The keyring entry of a plugin's secret: the keyring user is `plugin/<plugin id>/<key>`, so
/// plugins cannot reach each other's secrets (ids have no `/`) and cannot clash with the
/// built-in stores, whose entries are named by store id.
pub fn secret_entry(plugin_id: &str, key: &str) -> String {
    format!("plugin/{plugin_id}/{key}")
}

/// Checks a secret key and value before they reach the keyring.
pub fn check_secret(key: &str, value: Option<&str>) -> std::result::Result<(), String> {
    if key.trim().is_empty() || key.chars().count() > 128 || key.chars().any(char::is_control) {
        return Err("a secret's key must be 1 to 128 characters without control characters".into());
    }
    if let Some(value) = value
        && value.chars().count() > MAX_SECRET_CHARS
    {
        return Err(format!(
            "a secret is limited to {MAX_SECRET_CHARS} characters (the system keyring's limit)"
        ));
    }
    Ok(())
}

/// A plugin's sign-in as the app needs it: the window the shell opens, built from the manifest.
#[derive(Debug, Clone)]
pub struct LoginSetup {
    /// The page the sign-in window opens.
    pub url: String,
    pub window: LoginWindow,
}

impl LoginSetup {
    /// Builds the window from `permissions.login`, reading its scripts from the plugin folder.
    /// The strings live as long as the app: a plugin is loaded a few times per run, and
    /// [`LoginWindow`] holds `&'static str`.
    pub fn new(manifest: &PluginManifest, dir: &Path) -> Result<Option<Self>> {
        let Some(login) = &manifest.permissions.login else {
            return Ok(None);
        };
        let script = |name: &str| -> Result<&'static str> {
            let file = dir.join(name);
            let meta = std::fs::metadata(&file).map_err(|e| PluginError::io(&file, e))?;
            if !meta.is_file() || meta.len() > MAX_SCRIPT_BYTES {
                return Err(PluginError::Invalid(format!(
                    "The script \"{name}\" must be a file of at most {} KB.",
                    MAX_SCRIPT_BYTES / 1024
                )));
            }
            let text = std::fs::read_to_string(&file).map_err(|e| PluginError::io(&file, e))?;
            Ok(leak(text))
        };
        Ok(Some(Self {
            url: login.url.clone(),
            window: window_for(login, &manifest.name, script)?,
        }))
    }
}

fn window_for(
    login: &LoginPermission,
    store_name: &str,
    script: impl Fn(&str) -> Result<&'static str>,
) -> Result<LoginWindow> {
    let domains: Vec<&'static str> = login
        .domains
        .iter()
        .map(|d| leak(d.to_ascii_lowercase()))
        .collect();
    let session = login
        .session
        .as_ref()
        .map(|session| -> Result<WebSessionPage> {
            Ok(WebSessionPage {
                store_name: leak(store_name.to_string()),
                url: leak(session.url.clone()),
                script: script(&session.script)?,
                renew_url: session.renew_url.clone().map(leak),
            })
        })
        .transpose()?;
    let rule = login.capture.as_ref().map(|capture| {
        &*Box::leak(Box::new(CaptureRule {
            prefix: leak(capture.url_prefix.clone()),
            param: leak(capture.param.clone()),
        }))
    });
    Ok(LoginWindow {
        title: leak(login.title.clone()),
        domains: Box::leak(domains.into_boxed_slice()),
        rule,
        user_agent: login.user_agent.clone().map(leak),
        script: login.script.as_deref().map(script).transpose()?,
        web_session: session,
        // Page scripts report by navigating to an address below `questkeep.invalid`.
        capture: |url| url.starts_with(CAPTURE_PREFIX).then(|| url.to_string()),
    })
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// The store's id once the plugin has said it: browser profiles and the website session are
/// kept per store, like the built-in stores', and the plugin's call can come before that.
pub type StoreId = Arc<OnceLock<String>>;

// ---- the file system -------------------------------------------------------------------

/// The roots that make up the plugin's view of the PC: host path and the path the plugin sees.
/// Windows: each local drive (`C:\` as `/host/c`); elsewhere the root (`/` as `/host`).
pub fn filesystem_mounts() -> Vec<(PathBuf, String)> {
    #[cfg(windows)]
    {
        questkeep_store_api::platform::local_drives()
            .into_iter()
            .filter_map(|root| {
                let letter = root.to_string_lossy().chars().next()?.to_ascii_lowercase();
                Some((root, format!("{HOST_ROOT}/{letter}")))
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec![(PathBuf::from("/"), HOST_ROOT.to_string())]
    }
}

/// A folder of this PC by name (see the `known-folder` host function), only when it exists.
pub fn known_folder_path(name: &str, home: Option<&Path>) -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let windows = cfg!(windows);
    let macos = cfg!(target_os = "macos");
    let path = match name {
        "home" => home.map(Path::to_path_buf),
        "local-app-data" if windows => var("LOCALAPPDATA"),
        "app-data" if windows => var("APPDATA"),
        "program-data" if windows => var("ProgramData"),
        "program-files" if windows => var("ProgramFiles"),
        "program-files-x86" if windows => var("ProgramFiles(x86)"),
        "config" if windows => var("APPDATA"),
        "config" if macos => home.map(|h| h.join("Library/Application Support")),
        "config" => var("XDG_CONFIG_HOME").or_else(|| home.map(|h| h.join(".config"))),
        "data" if windows => var("LOCALAPPDATA"),
        "data" if macos => home.map(|h| h.join("Library/Application Support")),
        "data" => var("XDG_DATA_HOME").or_else(|| home.map(|h| h.join(".local/share"))),
        _ => None,
    };
    path.filter(|p| p.is_dir())
}

/// Whether `path` is inside one of the `protected` folders (compared without case on Windows).
pub fn is_protected(path: &Path, protected: &[PathBuf]) -> bool {
    let normal = |p: &Path| {
        let text = p.to_string_lossy().replace('\\', "/");
        let text = text.trim_end_matches('/').to_string();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let path = normal(path);
    protected.iter().any(|folder| {
        let folder = normal(folder);
        path == folder
            || path
                .strip_prefix(&folder)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

// ---- the launcher ----------------------------------------------------------------------

/// What a plugin store may tell Questkeep to close after a game: only processes whose names the
/// manifest declares (`permissions.launcher.processNames`), and only a clean-exit command whose
/// program is one of them. Without a declaration the store has no launcher process.
pub fn gate_launcher(process: LauncherProcess, declared: &[String]) -> Option<LauncherProcess> {
    let is_declared = |name: &str| declared.iter().any(|d| d.eq_ignore_ascii_case(name));
    let process_names: Vec<String> = process
        .process_names
        .into_iter()
        .filter(|name| is_declared(name))
        .collect();
    if process_names.is_empty() {
        return None;
    }
    let close = match process.close {
        LauncherClose::Command(action) => {
            let program = file_name(&action.target);
            if action.kind == ActionKind::File && is_declared(program) {
                LauncherClose::Command(action)
            } else {
                return None;
            }
        }
        LauncherClose::CloseWindows {
            process_names: only,
        } => LauncherClose::CloseWindows {
            process_names: only.into_iter().filter(|name| is_declared(name)).collect(),
        },
        LauncherClose::ForceQuit => LauncherClose::ForceQuit,
    };
    Some(LauncherProcess {
        process_names,
        close,
    })
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use questkeep_store_api::model::PlayAction;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn process(names: &[&str], close: LauncherClose) -> LauncherProcess {
        LauncherProcess {
            process_names: self::names(names),
            close,
        }
    }

    #[test]
    fn secrets_are_named_per_plugin() {
        assert_eq!(secret_entry("my-store", "token"), "plugin/my-store/token");
        // A key cannot make the entry look like another plugin's.
        assert_ne!(secret_entry("a", "b/c"), secret_entry("a-b", "c"));
        assert_ne!(secret_entry("a", "../b/c"), secret_entry("b", "c"));
    }

    #[test]
    fn secret_keys_and_values_are_checked() {
        assert!(check_secret("token", Some("x")).is_ok());
        assert!(check_secret("", None).is_err());
        assert!(check_secret("a\nb", None).is_err());
        assert!(check_secret(&"k".repeat(129), None).is_err());
        assert!(check_secret("k", Some(&"v".repeat(MAX_SECRET_CHARS))).is_ok());
        assert!(check_secret("k", Some(&"v".repeat(MAX_SECRET_CHARS + 1))).is_err());
    }

    #[test]
    fn protected_folders_cover_everything_below_them() {
        let protected = vec![
            PathBuf::from("/data/q/plugins"),
            PathBuf::from("/data/q/plugin-data/"),
        ];
        assert!(is_protected(Path::new("/data/q/plugins"), &protected));
        assert!(is_protected(
            Path::new("/data/q/plugins/a/b.exe"),
            &protected
        ));
        assert!(is_protected(
            Path::new("/data/q/plugin-data/x/y"),
            &protected
        ));
        assert!(!is_protected(
            Path::new("/data/q/plugins-other/a"),
            &protected
        ));
        assert!(!is_protected(Path::new("/data/q/library.db"), &protected));
        assert!(!is_protected(Path::new("/data/q"), &protected));
    }

    #[test]
    fn the_launcher_only_closes_declared_processes() {
        let declared = names(&["Steam.exe", "steamwebhelper.exe"]);
        // Undeclared names are dropped; none left means no launcher process at all.
        let kept = gate_launcher(
            process(&["steam.exe", "antivirus.exe"], LauncherClose::ForceQuit),
            &declared,
        )
        .unwrap();
        assert_eq!(kept.process_names, ["steam.exe"]);
        assert!(gate_launcher(process(&["x.exe"], LauncherClose::ForceQuit), &declared).is_none());
        assert!(gate_launcher(process(&["steam.exe"], LauncherClose::ForceQuit), &[]).is_none());
        // Close-windows names are filtered too.
        let kept = gate_launcher(
            process(
                &["steam.exe"],
                LauncherClose::CloseWindows {
                    process_names: names(&["steam.exe", "other.exe"]),
                },
            ),
            &declared,
        )
        .unwrap();
        assert_eq!(
            kept.close,
            LauncherClose::CloseWindows {
                process_names: names(&["steam.exe"])
            }
        );
    }

    #[test]
    fn a_clean_exit_command_must_be_a_declared_program() {
        let declared = names(&["steam.exe"]);
        let command = |target: &str| LauncherClose::Command(PlayAction::file(target));
        let ok = gate_launcher(
            process(&["steam.exe"], command(r"C:\Steam\steam.exe")),
            &declared,
        );
        assert!(ok.is_some());
        let ok = gate_launcher(process(&["steam.exe"], command("/x/STEAM.EXE")), &declared);
        assert!(ok.is_some());
        for target in [
            r"C:\Windows\System32\cmd.exe",
            "powershell.exe",
            "steam.exe.bat",
        ] {
            assert!(
                gate_launcher(process(&["steam.exe"], command(target)), &declared).is_none(),
                "{target}"
            );
        }
        // A URI is not a program.
        let uri = LauncherClose::Command(PlayAction::uri("steam.exe"));
        assert!(gate_launcher(process(&["steam.exe"], uri), &declared).is_none());
    }

    #[test]
    fn known_folders_need_to_exist_and_know_their_name() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            known_folder_path("home", Some(tmp.path())),
            Some(tmp.path().to_path_buf())
        );
        assert_eq!(known_folder_path("home", None), None);
        assert_eq!(known_folder_path("nonsense", Some(tmp.path())), None);
        let missing = tmp.path().join("missing");
        assert_eq!(known_folder_path("home", Some(&missing)), None);
    }

    #[test]
    fn the_file_system_is_mounted_below_host() {
        let mounts = filesystem_mounts();
        assert!(!mounts.is_empty());
        for (dir, guest) in &mounts {
            assert!(guest.starts_with("/host"), "{guest}");
            assert!(dir.is_absolute(), "{}", dir.display());
        }
    }

    #[test]
    fn a_login_window_is_built_from_the_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("login.js"), "var a = 1;").unwrap();
        std::fs::write(
            tmp.path().join("session.js"),
            "var reportSignedOut = REPORT_SIGNED_OUT;",
        )
        .unwrap();
        let manifest = PluginManifest::parse(
            &serde_json::json!({
                "id": "ex", "name": "Example", "version": "1", "apiVersion": 1,
                "permissions": { "login": {
                    "title": "Sign in", "url": "https://auth.example.com/",
                    "domains": ["Example.com"],
                    "capture": { "param": "code", "urlPrefix": "https://embed.example.com/ok" },
                    "script": "login.js",
                    "session": { "url": "https://www.example.com/a", "script": "session.js" }
                } }
            })
            .to_string(),
        )
        .unwrap();
        let setup = LoginSetup::new(&manifest, tmp.path()).unwrap().unwrap();
        assert_eq!(setup.url, "https://auth.example.com/");
        let window = setup.window;
        assert_eq!(window.title, "Sign in");
        assert_eq!(window.domains, ["example.com"]);
        assert_eq!(window.script, Some("var a = 1;"));
        assert_eq!(window.web_session.unwrap().store_name, "Example");
        // Domain lock, parameter capture and the questkeep.invalid report address.
        use questkeep_store_api::auth::Navigation;
        assert_eq!(
            window.navigation("https://auth.example.com/x"),
            Navigation::Allow
        );
        assert_eq!(window.navigation("https://evil.net/"), Navigation::Block);
        assert_eq!(
            window.navigation("https://embed.example.com/ok?code=42"),
            Navigation::Capture("42".into())
        );
        let report = "https://questkeep.invalid/session?a=1";
        assert_eq!(
            window.navigation(report),
            Navigation::Capture(report.into())
        );
        // No login permission, no window.
        let plain = PluginManifest::parse(r#"{"id":"ex","name":"E","version":"1","apiVersion":1}"#)
            .unwrap();
        assert!(LoginSetup::new(&plain, tmp.path()).unwrap().is_none());
    }

    #[test]
    fn a_missing_or_huge_script_is_an_invalid_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = PluginManifest::parse(
            &serde_json::json!({
                "id": "ex", "name": "Example", "version": "1", "apiVersion": 1,
                "permissions": { "login": {
                    "title": "Sign in", "url": "https://auth.example.com/",
                    "domains": ["example.com"], "script": "login.js"
                } }
            })
            .to_string(),
        )
        .unwrap();
        assert!(LoginSetup::new(&manifest, tmp.path()).is_err());
        std::fs::write(
            tmp.path().join("login.js"),
            vec![b' '; MAX_SCRIPT_BYTES as usize + 1],
        )
        .unwrap();
        let err = LoginSetup::new(&manifest, tmp.path()).unwrap_err();
        assert!(err.to_string().contains("at most"));
    }
}
