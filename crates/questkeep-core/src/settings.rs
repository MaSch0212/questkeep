//! User settings stored in `settings.json`. A missing or unreadable file yields defaults so a
//! broken settings file never prevents the app from starting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    /// Scan all store providers when the app starts.
    pub scan_on_startup: bool,
    /// Download missing covers from store CDNs.
    pub download_covers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_on_startup: true,
            download_covers: true,
        }
    }
}

/// Loads and saves [`Settings`].
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Settings {
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

    /// Writes atomically (temp file + rename) so a crash never leaves a truncated file.
    pub fn save(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string_pretty(settings)?;
        let tmp = tmp_path(&self.path);
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
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

    #[test]
    fn missing_or_corrupt_file_gives_defaults_and_save_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(tmp.path().join("settings.json"));
        assert_eq!(store.load(), Settings::default());

        std::fs::write(tmp.path().join("settings.json"), "{not json").unwrap();
        assert_eq!(store.load(), Settings::default());

        let custom = Settings {
            scan_on_startup: false,
            download_covers: false,
        };
        store.save(&custom).unwrap();
        assert_eq!(store.load(), custom);
    }

    #[test]
    fn unknown_and_missing_keys_are_tolerated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"scanOnStartup": false, "future": 1}"#).unwrap();
        let s = SettingsStore::new(path).load();
        assert!(!s.scan_on_startup);
        assert!(s.download_covers);
    }
}
