//! User settings stored in `settings.json`. A missing or unreadable file yields defaults so a
//! broken settings file never prevents the app from starting.
//!
//! API keys are not part of the file: [`SettingsStore`] keeps them in the system keyring (the
//! same store as sign-in tokens) and fills them into [`Settings`] on load, so the rest of the app
//! and the UI see one settings object.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::auth::TokenStore;
use crate::backup::BackupSettings;
use crate::error::{CoreError, Result};
use crate::model::{DEFAULT_PREFIXES, MetadataPriority};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    /// Scan all store providers when the app starts.
    pub scan_on_startup: bool,
    /// Download missing covers from store CDNs.
    pub download_covers: bool,
    /// Download descriptions, genres, release dates and backgrounds from stores.
    pub fetch_metadata: bool,
    /// Per-provider configuration keyed by provider id, e.g. `steam: { apiKey, steamId }`.
    /// Passed to the provider as-is. Secret fields a store declares (`steam.apiKey`) live in the
    /// keyring, not in the file.
    #[ts(type = "Record<string, Record<string, unknown>>")]
    pub providers: BTreeMap<String, serde_json::Value>,
    /// Optional SteamGridDB API key. When set, SteamGridDB supplies covers and backgrounds that
    /// the stores don't have. Lives in the keyring, not in the file.
    #[serde(rename = "steamGridDbApiKey")]
    pub steamgriddb_api_key: Option<String>,
    /// Optional IGDB (Twitch application) Client ID. With the secret, IGDB supplies descriptions,
    /// genres and the like for games whose store has none. Lives in the keyring, not in the file.
    pub igdb_client_id: Option<String>,
    /// Optional IGDB (Twitch application) Client Secret. Lives in the keyring, not in the file.
    pub igdb_client_secret: Option<String>,
    /// The close button hides Questkeep to the system tray instead of quitting.
    pub close_to_tray: bool,
    /// Minimizing the window hides Questkeep to the system tray.
    pub minimize_to_tray: bool,
    /// Start Questkeep when the user signs in to the computer.
    pub start_with_system: bool,
    /// When started by the system (`--minimized`), stay in the tray without opening the window.
    pub start_minimized: bool,
    /// Notice installed games started outside Questkeep (store clients, mod managers, shortcuts)
    /// and record their play sessions.
    pub detect_external_launches: bool,
    /// Show the start page (the Home entry above the views).
    pub show_start_page: bool,
    /// The page Questkeep opens on.
    pub startup_page: StartupPage,
    /// Which metadata source wins per field.
    pub metadata_priority: MetadataPriority,
    /// Words ignored at the start of names when sorting.
    pub sort_prefixes: SortPrefixes,
    /// Automatic backups (kept from the current install when a backup is restored).
    pub backups: BackupSettings,
    /// UI language (`en`, `de`, `fr`, `it`, `es`, `pt`); `None` follows the system.
    pub language: Option<String>,
    /// Price comparison websites, in the order the + menu offers them.
    pub price_sites: Vec<crate::price_sites::PriceSite>,
}

/// Words like "The" that sorting skips at the start of a name ("The Witcher" sorts under W).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct SortPrefixes {
    pub enabled: bool,
    pub words: Vec<String>,
}

impl Default for SortPrefixes {
    fn default() -> Self {
        Self {
            enabled: true,
            words: DEFAULT_PREFIXES.iter().map(ToString::to_string).collect(),
        }
    }
}

impl SortPrefixes {
    /// The words sorting skips right now: none while turned off; trimmed, without blanks or
    /// repeats (case doesn't matter).
    pub fn active(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        let mut words: Vec<String> = Vec::new();
        for word in self
            .words
            .iter()
            .map(|w| w.trim())
            .filter(|w| !w.is_empty())
        {
            if !words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
                words.push(word.to_string());
            }
        }
        words
    }
}

/// The page Questkeep shows when it opens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum StartupPage {
    /// The start page; the first view while the start page is turned off.
    #[default]
    Start,
    /// The start page or view that was open when Questkeep last closed.
    Last,
    /// A saved view; the first view if it no longer exists.
    View { id: String },
}

impl Settings {
    /// The configuration section of one provider (`null` when absent).
    pub fn provider_config(&self, id: &str) -> &serde_json::Value {
        self.providers.get(id).unwrap_or(&serde_json::Value::Null)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_on_startup: true,
            download_covers: true,
            fetch_metadata: true,
            providers: BTreeMap::new(),
            steamgriddb_api_key: None,
            igdb_client_id: None,
            igdb_client_secret: None,
            close_to_tray: true,
            minimize_to_tray: false,
            start_with_system: false,
            start_minimized: true,
            detect_external_launches: true,
            show_start_page: true,
            startup_page: StartupPage::Start,
            metadata_priority: MetadataPriority::default(),
            sort_prefixes: SortPrefixes::default(),
            backups: BackupSettings::default(),
            language: None,
            price_sites: Vec::new(),
        }
    }
}

/// A secret field of [`Settings`] and its keyring entry name.
struct SecretField {
    keyring_user: &'static str,
    get: fn(&Settings) -> Option<String>,
    set: fn(&mut Settings, Option<String>),
}

/// The app's own secrets; stores declare theirs ([`Secret::Provider`]).
const APP_SECRETS: [SecretField; 3] = [
    SecretField {
        keyring_user: "setting:steamGridDbApiKey",
        get: |s| s.steamgriddb_api_key.clone(),
        set: |s, value| s.steamgriddb_api_key = value,
    },
    SecretField {
        keyring_user: "setting:igdbClientId",
        get: |s| s.igdb_client_id.clone(),
        set: |s, value| s.igdb_client_id = value,
    },
    SecretField {
        keyring_user: "setting:igdbClientSecret",
        get: |s| s.igdb_client_secret.clone(),
        set: |s, value| s.igdb_client_secret = value,
    },
];

/// A value kept in the keyring instead of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Secret {
    /// One of [`APP_SECRETS`].
    App(usize),
    /// `providers.<provider>.<key>`, a secret field a store declared.
    Provider { provider: String, key: String },
}

impl Secret {
    fn keyring_user(&self) -> String {
        match self {
            Secret::App(i) => APP_SECRETS[*i].keyring_user.to_string(),
            Secret::Provider { provider, key } => format!("setting:{provider}.{key}"),
        }
    }

    fn get(&self, s: &Settings) -> Option<String> {
        match self {
            Secret::App(i) => (APP_SECRETS[*i].get)(s),
            Secret::Provider { provider, key } => s
                .providers
                .get(provider)
                .and_then(|c| c.get(key))
                .and_then(|v| v.as_str())
                .map(str::to_string),
        }
    }

    fn set(&self, s: &mut Settings, value: Option<String>) {
        match self {
            Secret::App(i) => (APP_SECRETS[*i].set)(s, value),
            Secret::Provider { provider, key } => match value {
                Some(v) => {
                    let section = s
                        .providers
                        .entry(provider.clone())
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(map) = section.as_object_mut() {
                        map.insert(key.clone(), serde_json::Value::String(v));
                    }
                }
                None => {
                    if let Some(map) = s
                        .providers
                        .get_mut(provider)
                        .and_then(|c| c.as_object_mut())
                    {
                        map.remove(key);
                    }
                }
            },
        }
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// A copy without any secret values (what goes into the file).
fn strip_secrets(settings: &Settings, secrets: &[Secret]) -> Settings {
    let mut copy = settings.clone();
    for secret in secrets {
        secret.set(&mut copy, None);
    }
    copy
}

/// Loads and saves [`Settings`]; secrets go to the keyring.
#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
    secrets: Arc<dyn TokenStore>,
    /// What is kept in `secrets` instead of the file.
    fields: Arc<[Secret]>,
    /// Secret values as last loaded or saved, so frequent loads don't hit the keyring.
    cache: Arc<Mutex<Option<Vec<Option<String>>>>>,
}

impl std::fmt::Debug for SettingsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SettingsStore {
    /// `provider_secrets` are the secret fields stores declare, as (provider id, key).
    pub fn new(
        path: impl Into<PathBuf>,
        secrets: Arc<dyn TokenStore>,
        provider_secrets: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        let fields = (0..APP_SECRETS.len())
            .map(Secret::App)
            .chain(
                provider_secrets
                    .into_iter()
                    .map(|(provider, key)| Secret::Provider { provider, key }),
            )
            .collect();
        Self {
            path: path.into(),
            secrets,
            fields,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Reads the file, moves any plain-text secrets left by older versions into the keyring, and
    /// fills the secrets in from the keyring.
    pub fn load(&self) -> Settings {
        let mut settings = self.read_file();
        let plain: Vec<Option<String>> = self
            .fields
            .iter()
            .map(|f| non_empty(f.get(&settings)))
            .collect();
        if plain.iter().any(Option::is_some) && !self.migrate_plain_secrets(&settings, &plain) {
            // No keyring: keep using the values from the file.
            return settings;
        }
        settings = strip_secrets(&settings, &self.fields);
        for (field, value) in self.fields.iter().zip(self.secret_values()) {
            if value.is_some() {
                field.set(&mut settings, value);
            }
        }
        settings
    }

    /// Writes the secrets to the keyring and the rest to the file (atomically: temp file +
    /// rename). Fails instead of writing a secret to the file when the keyring is unavailable.
    pub fn save(&self, settings: &Settings) -> Result<()> {
        let values: Vec<Option<String>> = self
            .fields
            .iter()
            .map(|f| non_empty(f.get(settings)))
            .collect();
        for (field, value) in self.fields.iter().zip(&values) {
            let user = field.keyring_user();
            let result = match value {
                Some(v) => self.secrets.save(&user, v),
                None => self.secrets.delete(&user),
            };
            result.map_err(|err| {
                CoreError::Invalid(format!(
                    "Could not store the API key in the system keyring: {err}"
                ))
            })?;
        }
        *self.cache.lock().unwrap_or_else(PoisonError::into_inner) = Some(values);
        self.write_file(&strip_secrets(settings, &self.fields))
    }

    fn read_file(&self) -> Settings {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                tracing::warn!(%err, path = %self.path.display(), "invalid settings file, using defaults");
                Settings::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(err) => {
                tracing::warn!(%err, path = %self.path.display(), "cannot read settings file, using defaults");
                Settings::default()
            }
        }
    }

    fn write_file(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string_pretty(settings)?;
        let tmp = tmp_path(&self.path);
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Secret values from the cache, or the keyring on first use. Keyring errors count as "no
    /// value" (logged), so a missing keyring never breaks loading settings.
    fn secret_values(&self) -> Vec<Option<String>> {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(values) = cache.as_ref() {
            return values.clone();
        }
        let values: Vec<Option<String>> = self
            .fields
            .iter()
            .map(|field| match self.secrets.load(&field.keyring_user()) {
                Ok(value) => non_empty(value),
                Err(err) => {
                    tracing::warn!(%err, "could not read an API key from the keyring");
                    None
                }
            })
            .collect();
        *cache = Some(values.clone());
        values
    }

    /// Moves plain-text secrets from the file (written by older versions) into the keyring and
    /// rewrites the file without them. Returns `false` if the keyring is unavailable; the file is
    /// then left as it is, so nothing is lost.
    fn migrate_plain_secrets(&self, settings: &Settings, plain: &[Option<String>]) -> bool {
        for (field, value) in self.fields.iter().zip(plain) {
            if let Some(v) = value
                && let Err(err) = self.secrets.save(&field.keyring_user(), v)
            {
                tracing::warn!(%err, "could not move an API key into the keyring; keeping it in settings.json");
                return false;
            }
        }
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match self.write_file(&strip_secrets(settings, &self.fields)) {
            Ok(()) => tracing::info!("moved API keys from settings.json into the system keyring"),
            Err(err) => tracing::warn!(%err, "could not rewrite settings.json without API keys"),
        }
        true
    }
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthError, MemoryTokenStore};

    fn steam_key() -> [(String, String); 1] {
        [("steam".into(), "apiKey".into())]
    }

    fn store_at(path: PathBuf) -> (SettingsStore, Arc<MemoryTokenStore>) {
        let secrets = Arc::new(MemoryTokenStore::default());
        (
            SettingsStore::new(path, secrets.clone(), steam_key()),
            secrets,
        )
    }

    #[test]
    fn missing_or_corrupt_file_gives_defaults_and_save_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let (store, secrets) = store_at(path.clone());
        assert_eq!(store.load(), Settings::default());

        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(store.load(), Settings::default());

        let custom = Settings {
            scan_on_startup: false,
            download_covers: false,
            fetch_metadata: false,
            providers: BTreeMap::from([(
                "steam".to_string(),
                serde_json::json!({ "apiKey": "k", "steamId": "1" }),
            )]),
            steamgriddb_api_key: Some("sgdb".into()),
            igdb_client_id: Some("twitch-id".into()),
            igdb_client_secret: Some("twitch-secret".into()),
            close_to_tray: false,
            minimize_to_tray: true,
            start_with_system: true,
            start_minimized: false,
            detect_external_launches: false,
            show_start_page: false,
            startup_page: StartupPage::View { id: "fav".into() },
            metadata_priority: crate::model::MetadataPriority {
                order: vec!["steam".into()],
                ..Default::default()
            },
            sort_prefixes: SortPrefixes {
                enabled: false,
                words: vec!["Der".into()],
            },
            backups: BackupSettings {
                enabled: false,
                folder: Some("D:/Backups".into()),
                frequency: crate::backup::BackupFrequency::Daily,
                keep: 3,
            },
            language: Some("de".into()),
            price_sites: vec![crate::price_sites::PriceSite {
                name: "GG.deals".into(),
                url: "https://gg.deals/".into(),
            }],
        };
        store.save(&custom).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            !raw.contains("sgdb")
                && !raw.contains("apiKey")
                && !raw.contains("twitch-id")
                && !raw.contains("twitch-secret"),
            "no secrets in the file: {raw}"
        );
        assert!(raw.contains("steamId"));
        assert_eq!(
            secrets
                .load("setting:steamGridDbApiKey")
                .unwrap()
                .as_deref(),
            Some("sgdb")
        );
        for (user, expected) in [
            ("setting:igdbClientId", "twitch-id"),
            ("setting:igdbClientSecret", "twitch-secret"),
        ] {
            assert_eq!(secrets.load(user).unwrap().as_deref(), Some(expected));
        }
        assert_eq!(store.load(), custom);
        // A fresh store (a new process) reads the secrets back from the keyring.
        let reopened = SettingsStore::new(path, secrets.clone(), steam_key());
        assert_eq!(reopened.load(), custom);
        assert_eq!(custom.provider_config("steam")["steamId"], "1");
        assert!(custom.provider_config("gog").is_null());
    }

    #[test]
    fn unknown_and_missing_keys_are_tolerated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"scanOnStartup": false, "future": 1}"#).unwrap();
        let s = store_at(path).0.load();
        assert!(!s.scan_on_startup);
        assert!(s.download_covers);
        assert!(s.close_to_tray && s.start_minimized);
        assert!(!s.minimize_to_tray && !s.start_with_system);
        assert!(s.show_start_page);
        assert_eq!(s.startup_page, StartupPage::Start);
    }

    #[test]
    fn startup_page_roundtrips_as_tagged_json() {
        let page = StartupPage::View { id: "fav".into() };
        let json = serde_json::to_string(&page).unwrap();
        assert_eq!(json, r#"{"kind":"view","id":"fav"}"#);
        assert_eq!(serde_json::from_str::<StartupPage>(&json).unwrap(), page);
        assert_eq!(
            serde_json::from_str::<StartupPage>(r#"{"kind":"last"}"#).unwrap(),
            StartupPage::Last
        );
    }

    #[test]
    fn plain_text_keys_from_older_versions_move_into_the_keyring() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"providers":{"steam":{"apiKey":"OLD","steamId":"7"}},"steamGridDbApiKey":"GRID","igdbClientSecret":"TWITCH"}"#,
        )
        .unwrap();
        let (store, secrets) = store_at(path.clone());
        let s = store.load();
        assert_eq!(s.provider_config("steam")["apiKey"], "OLD");
        assert_eq!(s.provider_config("steam")["steamId"], "7");
        assert_eq!(s.steamgriddb_api_key.as_deref(), Some("GRID"));
        assert_eq!(s.igdb_client_secret.as_deref(), Some("TWITCH"));
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            !raw.contains("OLD") && !raw.contains("GRID") && !raw.contains("TWITCH"),
            "{raw}"
        );
        assert!(raw.contains("steamId"));
        assert_eq!(
            secrets.load("setting:steam.apiKey").unwrap().as_deref(),
            Some("OLD")
        );
    }

    struct NoKeyring;
    impl TokenStore for NoKeyring {
        fn load(&self, _: &str) -> std::result::Result<Option<String>, AuthError> {
            Err(AuthError::NoKeyring("test".into()))
        }
        fn save(&self, _: &str, _: &str) -> std::result::Result<(), AuthError> {
            Err(AuthError::NoKeyring("test".into()))
        }
        fn delete(&self, _: &str) -> std::result::Result<(), AuthError> {
            Err(AuthError::NoKeyring("test".into()))
        }
    }

    #[test]
    fn clearing_a_key_deletes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, secrets) = store_at(tmp.path().join("settings.json"));
        let mut s = Settings {
            steamgriddb_api_key: Some("GRID".into()),
            ..Settings::default()
        };
        store.save(&s).unwrap();
        s.steamgriddb_api_key = Some("  ".into());
        store.save(&s).unwrap();
        assert!(secrets.load("setting:steamGridDbApiKey").unwrap().is_none());
        assert!(store.load().steamgriddb_api_key.is_none());

        s.igdb_client_id = Some("id".into());
        s.igdb_client_secret = Some("secret".into());
        store.save(&s).unwrap();
        s.igdb_client_secret = None;
        store.save(&s).unwrap();
        assert!(secrets.load("setting:igdbClientSecret").unwrap().is_none());
        assert_eq!(store.load().igdb_client_id.as_deref(), Some("id"));
    }

    #[test]
    fn without_a_keyring_secrets_are_never_written_to_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let store = SettingsStore::new(path.clone(), Arc::new(NoKeyring), steam_key());
        let with_key = Settings {
            steamgriddb_api_key: Some("SECRET".into()),
            ..Settings::default()
        };
        assert!(store.save(&with_key).is_err());
        assert!(!path.exists() || !std::fs::read_to_string(&path).unwrap().contains("SECRET"));
        // Loading still works, and an old plain-text file is left untouched.
        std::fs::write(&path, r#"{"steamGridDbApiKey":"LEGACY"}"#).unwrap();
        assert_eq!(store.load().steamgriddb_api_key.as_deref(), Some("LEGACY"));
        assert!(std::fs::read_to_string(&path).unwrap().contains("LEGACY"));
    }
}
