//! Image cache under `<data>/images`. Paths stored in the database are relative to the data
//! directory, e.g. `images/covers/<game id>.jpg` or `images/backgrounds/<game id>.jpg`.

mod fetcher;
pub mod hltb;
mod igdb;
mod steamgriddb;

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::paths::DataPaths;

pub use fetcher::{CoverFetcher, CoverJob, MetadataFetcher, MetadataJob, image_extension};
pub(crate) use fetcher::{download, http_agent};
pub(crate) use igdb::CHECKED_KEY as IGDB_CHECKED_KEY;
pub use igdb::{Igdb, IgdbLookup};
pub use questkeep_imaging::{
    ICON_SIZE, IconRgba, SquareFrom, icon_png_from_bytes, icon_png_from_rgba, load_icon_rgba,
    load_image_filled, scale_icon,
};
pub use steamgriddb::{Lookup, SteamGridDb};
pub(crate) use steamgriddb::{checked_key, lookup_markers, platform_name};

/// `games.extra` key set when the game's icon was cropped from its cover, so a new cover can
/// replace the icon too (store and executable icons stay).
pub(crate) const ICON_FROM_COVER_KEY: &str = "iconFromCover";

/// `games.extra` key set when the user picked this image themselves; fetchers then leave it alone.
pub(crate) fn user_image_key(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::Cover => "userCover",
        ImageKind::Background => "userBackground",
        ImageKind::Icon => "userIcon",
    }
}

/// Whether the user picked this image of the game themselves.
pub(crate) fn is_user_image(game: &crate::model::Game, kind: ImageKind) -> bool {
    game.extra.get(user_image_key(kind)).is_some()
}

/// Deletes the game's icon when it was made from the cover (the cover just changed); the tray
/// then queues a new one. Returns whether an icon was dropped.
pub(crate) fn drop_cover_icon(
    db: &crate::db::Database,
    images: &ImageCache,
    game: &crate::model::Game,
) -> Result<bool> {
    if game.extra.get(ICON_FROM_COVER_KEY).is_none() {
        return Ok(false);
    }
    images.remove_image(ImageKind::Icon, game.id)?;
    db.set_icon_path(game.id, None)?;
    db.set_extra_value(game.id, ICON_FROM_COVER_KEY, None)?;
    Ok(true)
}

const ALLOWED_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];
/// Size of a cover tile in pixels (2:3): sharp for the tray popup's tiles up to 300 % display
/// scaling. Changing it needs a new game state version (`game_state`) so the tiles are redone.
pub const TILE_WIDTH: u32 = 270;
pub const TILE_HEIGHT: u32 = 405;
/// Upper bound for user-picked and downloaded images.
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Kind of cached image; each kind has its own folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// Portrait 2:3 cover.
    Cover,
    /// Wide background / hero image.
    Background,
    /// Small square icon for menus (always a 32x32 PNG, see [`icons`]).
    Icon,
}

impl ImageKind {
    const ALL: [ImageKind; 3] = [ImageKind::Cover, ImageKind::Background, ImageKind::Icon];

    fn dir_name(self) -> &'static str {
        match self {
            ImageKind::Cover => "covers",
            ImageKind::Background => "backgrounds",
            ImageKind::Icon => "icons",
        }
    }
}

/// File-system side of the image cache.
#[derive(Debug, Clone)]
pub struct ImageCache {
    paths: DataPaths,
}

impl ImageCache {
    pub fn new(paths: DataPaths) -> Self {
        Self { paths }
    }

    fn kind_dir(&self, kind: ImageKind) -> PathBuf {
        self.paths.images_dir().join(kind.dir_name())
    }

    /// Relative path (as stored in the DB) for a game's image with the given extension.
    fn relative(kind: ImageKind, id: Uuid, ext: &str) -> String {
        format!("images/{}/{id}.{ext}", kind.dir_name())
    }

    /// Copies a user-picked image into the cache and returns the relative path.
    pub fn import_image(&self, kind: ImageKind, id: Uuid, source: &Path) -> Result<String> {
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
        std::fs::create_dir_all(self.kind_dir(kind))?;
        self.remove_image(kind, id)?;
        let relative = Self::relative(kind, id, &ext);
        std::fs::copy(source, self.paths.absolute(&relative))?;
        self.refresh_tile_for(kind, id, &relative);
        Ok(relative)
    }

    /// Writes downloaded bytes as the game's image (atomically) and returns the relative path.
    pub fn store_image(
        &self,
        kind: ImageKind,
        id: Uuid,
        ext: &str,
        bytes: &[u8],
    ) -> Result<String> {
        std::fs::create_dir_all(self.kind_dir(kind))?;
        let relative = Self::relative(kind, id, ext);
        let target = self.paths.absolute(&relative);
        let tmp = target.with_extension(format!("{ext}.part"));
        std::fs::write(&tmp, bytes)?;
        self.remove_image(kind, id)?;
        std::fs::rename(&tmp, &target)?;
        self.refresh_tile_for(kind, id, &relative);
        Ok(relative)
    }

    /// Relative path of a game's cover tile.
    fn tile_relative(id: Uuid) -> String {
        format!("images/tiles/{id}.jpg")
    }

    /// The game's cover tile (a small JPEG made from the cover, for the tray popup), if it exists.
    pub fn tile_path(&self, id: Uuid) -> Option<PathBuf> {
        self.resolve(&Self::tile_relative(id))
    }

    /// Makes (or replaces) the cover tile from the cover at `cover_relative`. Returns whether a
    /// tile exists afterwards: an unreadable cover leaves none, and a tile of an earlier cover is
    /// removed so it never shows the wrong image.
    pub fn refresh_tile(&self, id: Uuid, cover_relative: &str) -> Result<bool> {
        let source = self.paths.absolute(cover_relative);
        let target = self.paths.absolute(&Self::tile_relative(id));
        let Some(bytes) = questkeep_imaging::cover_tile_jpeg(&source, TILE_WIDTH, TILE_HEIGHT)
        else {
            self.remove_tile(id)?;
            return Ok(false);
        };
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = target.with_extension("jpg.part");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &target)?;
        Ok(true)
    }

    /// Deletes the game's cover tile.
    pub fn remove_tile(&self, id: Uuid) -> Result<()> {
        match std::fs::remove_file(self.paths.absolute(&Self::tile_relative(id))) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Makes the tile after a cover was written; a failure only costs the tray popup its fast path.
    fn refresh_tile_for(&self, kind: ImageKind, id: Uuid, cover_relative: &str) {
        if kind != ImageKind::Cover {
            return;
        }
        if let Err(err) = self.refresh_tile(id, cover_relative) {
            tracing::warn!(%err, %id, "could not make the cover tile");
        }
    }

    /// Deletes a game's cached image of one kind (any extension).
    pub fn remove_image(&self, kind: ImageKind, id: Uuid) -> Result<()> {
        if kind == ImageKind::Cover {
            self.remove_tile(id)?;
        }
        for ext in ALLOWED_EXTENSIONS {
            let path = self.paths.absolute(&Self::relative(kind, id, ext));
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    /// Deletes every cached image of a game.
    pub fn remove_all(&self, id: Uuid) -> Result<()> {
        for kind in ImageKind::ALL {
            self.remove_image(kind, id)?;
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
    fn imports_replaces_and_removes_images() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let cache = ImageCache::new(paths);
        let id = Uuid::new_v4();

        let png = tmp.path().join("cover.PNG");
        std::fs::write(&png, b"png").unwrap();
        let rel = cache.import_image(ImageKind::Cover, id, &png).unwrap();
        assert_eq!(rel, format!("images/covers/{id}.png"));
        assert!(cache.resolve(&rel).is_some());

        let rel2 = cache
            .store_image(ImageKind::Cover, id, "jpg", b"jpg")
            .unwrap();
        assert!(cache.resolve(&rel).is_none(), "old extension removed");
        assert!(cache.resolve(&rel2).is_some());

        let bg = cache
            .store_image(ImageKind::Background, id, "jpg", b"bg")
            .unwrap();
        assert_eq!(bg, format!("images/backgrounds/{id}.jpg"));

        cache.remove_all(id).unwrap();
        assert!(cache.resolve(&rel2).is_none());
        assert!(cache.resolve(&bg).is_none());
    }

    #[test]
    fn covers_get_a_tile_that_follows_them() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(tmp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let cache = ImageCache::new(paths);
        let id = Uuid::new_v4();

        let png = tmp.path().join("cover.png");
        image::RgbaImage::from_pixel(400, 600, image::Rgba([10, 120, 200, 255]))
            .save(&png)
            .unwrap();
        cache.import_image(ImageKind::Cover, id, &png).unwrap();
        let tile = cache.tile_path(id).expect("tile made with the cover");
        let size = image::ImageReader::open(&tile)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap();
        assert_eq!(size, (TILE_WIDTH, TILE_HEIGHT));

        // A cover that cannot be read leaves no tile of the old cover behind.
        cache
            .store_image(ImageKind::Cover, id, "png", b"broken")
            .unwrap();
        assert!(cache.tile_path(id).is_none());

        cache.import_image(ImageKind::Cover, id, &png).unwrap();
        assert!(cache.tile_path(id).is_some());
        cache.remove_image(ImageKind::Cover, id).unwrap();
        assert!(cache.tile_path(id).is_none(), "removed with the cover");
    }

    #[test]
    fn rejects_non_images() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ImageCache::new(DataPaths::new(tmp.path()));
        let txt = tmp.path().join("x.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(matches!(
            cache.import_image(ImageKind::Cover, Uuid::new_v4(), &txt),
            Err(CoreError::Invalid(_))
        ));
    }
}
