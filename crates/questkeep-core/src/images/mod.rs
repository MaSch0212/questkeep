//! Cover image cache under `<data>/images`. Paths stored in the database are relative to the
//! data directory, e.g. `images/covers/<game id>.jpg`.

mod fetcher;

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::paths::DataPaths;

pub use fetcher::{CoverFetcher, CoverJob, image_extension};

const COVERS_DIR: &str = "covers";
const ALLOWED_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];
/// Upper bound for user-picked and downloaded images.
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// File-system side of the image cache.
#[derive(Debug, Clone)]
pub struct ImageCache {
    paths: DataPaths,
}

impl ImageCache {
    pub fn new(paths: DataPaths) -> Self {
        Self { paths }
    }

    fn covers_dir(&self) -> PathBuf {
        self.paths.images_dir().join(COVERS_DIR)
    }

    /// Relative path (as stored in the DB) for a game's cover with the given extension.
    fn cover_relative(id: Uuid, ext: &str) -> String {
        format!("images/{COVERS_DIR}/{id}.{ext}")
    }

    /// Copies a user-picked image into the cache and returns the relative path.
    pub fn import_cover(&self, id: Uuid, source: &Path) -> Result<String> {
        let ext = source
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .filter(|e| ALLOWED_EXTENSIONS.contains(&e.as_str()))
            .ok_or_else(|| {
                CoreError::Invalid(
                    "Unsupported image type. Use a JPG, PNG, WebP or GIF file.".into(),
                )
            })?;
        let size = std::fs::metadata(source)?.len();
        if size > MAX_IMAGE_BYTES {
            return Err(CoreError::Invalid("The image is larger than 20 MB.".into()));
        }
        std::fs::create_dir_all(self.covers_dir())?;
        self.remove_covers(id)?;
        let relative = Self::cover_relative(id, &ext);
        std::fs::copy(source, self.paths.absolute(&relative))?;
        Ok(relative)
    }

    /// Writes downloaded bytes as the game's cover (atomically) and returns the relative path.
    pub fn store_cover_bytes(&self, id: Uuid, ext: &str, bytes: &[u8]) -> Result<String> {
        std::fs::create_dir_all(self.covers_dir())?;
        let relative = Self::cover_relative(id, ext);
        let target = self.paths.absolute(&relative);
        let tmp = target.with_extension(format!("{ext}.part"));
        std::fs::write(&tmp, bytes)?;
        self.remove_covers(id)?;
        std::fs::rename(&tmp, &target)?;
        Ok(relative)
    }

    /// Deletes every cached cover of a game (any extension).
    pub fn remove_covers(&self, id: Uuid) -> Result<()> {
        for ext in ALLOWED_EXTENSIONS {
            let path = self.paths.absolute(&Self::cover_relative(id, ext));
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    /// Absolute path for a stored relative path, if the file exists.
    pub fn resolve(&self, relative: &str) -> Option<PathBuf> {
        let path = self.paths.absolute(relative);
        path.is_file().then_some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_replaces_and_removes_covers() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let cache = ImageCache::new(paths);
        let id = Uuid::new_v4();

        let png = tmp.path().join("cover.PNG");
        std::fs::write(&png, b"png").unwrap();
        let rel = cache.import_cover(id, &png).unwrap();
        assert_eq!(rel, format!("images/covers/{id}.png"));
        assert!(cache.resolve(&rel).is_some());

        let rel2 = cache.store_cover_bytes(id, "jpg", b"jpg").unwrap();
        assert!(cache.resolve(&rel).is_none(), "old extension removed");
        assert!(cache.resolve(&rel2).is_some());

        cache.remove_covers(id).unwrap();
        assert!(cache.resolve(&rel2).is_none());
    }

    #[test]
    fn rejects_non_images() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ImageCache::new(DataPaths::new(tmp.path()));
        let txt = tmp.path().join("x.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(matches!(
            cache.import_cover(Uuid::new_v4(), &txt),
            Err(CoreError::Invalid(_))
        ));
    }
}
