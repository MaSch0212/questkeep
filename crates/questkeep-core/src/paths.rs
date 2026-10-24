//! Data directory layout and resolution (app-data or portable mode).

use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

/// Name of the marker file that enables portable mode when placed next to the executable.
pub const PORTABLE_MARKER: &str = "portable.txt";
const APP_DIR_NAME: &str = "questkeep";

/// All on-disk locations used by Questkeep, derived from one root directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    root: PathBuf,
    portable: bool,
}

impl DataPaths {
    /// Uses `root` directly (tests, custom locations).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            portable: false,
        }
    }

    /// Resolves the data root: `<exe dir>/data` when `portable.txt` sits next to the executable,
    /// otherwise the OS app-data directory (`%APPDATA%\questkeep`, `~/.local/share/questkeep`).
    pub fn resolve() -> Result<Self> {
        if let Some(exe_dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            && exe_dir.join(PORTABLE_MARKER).is_file()
        {
            return Ok(Self {
                root: exe_dir.join("data"),
                portable: true,
            });
        }
        let base = dirs::data_dir().ok_or_else(|| {
            CoreError::Invalid("Could not determine the user's app-data directory".into())
        })?;
        Ok(Self::new(base.join(APP_DIR_NAME)))
    }

    /// Creates the root, images and logs directories if missing.
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.images_dir())?;
        std::fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_portable(&self) -> bool {
        self.portable
    }

    pub fn database_file(&self) -> PathBuf {
        self.root.join("library.sqlite")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.root.join("images")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// Store answers kept between runs (e.g. the Epic catalog); safe to delete.
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Where the main window was last placed.
    pub fn window_file(&self) -> PathBuf {
        self.root.join("window.json")
    }

    /// Turns a path stored relative to the data root into an absolute path.
    pub fn absolute(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_relative_to_root() {
        let paths = DataPaths::new("/tmp/qk");
        assert_eq!(paths.database_file(), Path::new("/tmp/qk/library.sqlite"));
        assert_eq!(paths.images_dir(), Path::new("/tmp/qk/images"));
        assert_eq!(
            paths.absolute("images/a.jpg"),
            Path::new("/tmp/qk/images/a.jpg")
        );
        assert!(!paths.is_portable());
    }

    #[test]
    fn ensure_dirs_creates_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        assert!(paths.images_dir().is_dir());
        assert!(paths.logs_dir().is_dir());
    }
}
