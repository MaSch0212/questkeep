//! Finding a game's small square icon in Steam's local library cache.
//!
//! Steam's `appcache/librarycache/<appid>/` holds the artwork the client downloaded. The icon is
//! the small square `<40 hex digits>.jpg` next to `header.jpg`, `library_600x900.jpg`,
//! `library_hero.jpg`, `logo.png` and friends (a name that is a hash of the icon, so it can't be
//! derived from the app id). Very old clients used a flat `<appid>_icon.jpg`. The layout changes
//! between client versions, so the pick is by name pattern first and image size second.

use std::path::{Path, PathBuf};

/// Icons below this edge length are not worth using (and not what Steam stores as icons).
const MIN_SIDE: u32 = 16;
/// Square images above this edge length are artwork, not icons.
const MAX_SIDE: u32 = 256;

/// Files of the library cache that are known artwork, never an icon.
fn is_known_artwork(stem: &str) -> bool {
    let stem = stem.to_ascii_lowercase();
    stem == "header"
        || stem == "logo"
        || stem == "markers"
        || stem.starts_with("library_")
        || stem.contains("capsule")
        || stem.contains("hero")
        || stem.contains("background")
}

/// `<sha1 as 40 hex digits>`, the name Steam gives to icons.
fn is_icon_hash(stem: &str) -> bool {
    stem.len() == 40 && stem.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["jpg", "jpeg", "png"].contains(&e.to_ascii_lowercase().as_str()))
}

/// Edge length of `path` when it is a square image of a plausible icon size.
fn icon_side(path: &Path) -> Option<u32> {
    let (w, h) = image::image_dimensions(path).ok()?;
    (w == h && (MIN_SIDE..=MAX_SIDE).contains(&w)).then_some(w)
}

/// Icon files in one cache folder (`<appid>` or `<appid>/<hash>`), best first: hash-named files
/// before other unknown names, then the smaller (more icon-like) image, then by name. Only square
/// images between 16 and 256 px count; known artwork names are skipped.
pub(super) fn icon_files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(bool, u32, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image_file(p))
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_string();
            if is_known_artwork(&stem) {
                return None;
            }
            let side = icon_side(&path)?;
            Some((!is_icon_hash(&stem), side, path))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, _, path)| path).collect()
}

/// Every icon candidate of `app_id` below one library cache folder, in layout order: the old flat
/// `<appid>_icon.jpg`, the app folder, then its hash sub-folders.
pub(super) fn icon_candidates(cache: &Path, app_id: &str) -> Vec<PathBuf> {
    let mut out = vec![cache.join(format!("{app_id}_icon.jpg"))];
    let app_dir = cache.join(app_id);
    out.extend(icon_files_in(&app_dir));
    if let Ok(entries) = std::fs::read_dir(&app_dir) {
        let mut hashed: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        hashed.sort();
        for dir in hashed {
            out.extend(icon_files_in(&dir));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use image::{ImageFormat, Rgb, RgbImage};

    use super::*;

    const HASH: &str = "7807d6dcd71d8161465619b4f041794b0353a6d0";

    fn write(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let format = if name.ends_with(".png") {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        };
        RgbImage::from_pixel(w, h, Rgb([10, 120, 200]))
            .save_with_format(&path, format)
            .unwrap();
        path
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn current_layout_picks_the_hash_named_square_jpg() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("440");
        write(&app, &format!("{HASH}.jpg"), 32, 32);
        write(&app, "header.jpg", 460, 215);
        write(&app, "library_600x900.jpg", 600, 900);
        write(&app, "library_hero.jpg", 1920, 620);
        write(&app, "library_hero_blur.jpg", 192, 62);
        write(&app, "logo.png", 640, 360);
        let found = icon_candidates(tmp.path(), "440");
        assert_eq!(
            names(&found),
            ["440_icon.jpg".to_string(), format!("{HASH}.jpg")]
        );
    }

    #[test]
    fn newer_layout_with_hash_folders_is_searched_too() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("730");
        let hashed = app.join("abcdef");
        write(&hashed, "library_capsule.jpg", 300, 450);
        write(&hashed, "logo.png", 640, 360);
        write(&hashed, &format!("{HASH}.jpg"), 64, 64);
        let found = icon_candidates(tmp.path(), "730");
        assert_eq!(found.len(), 2);
        assert!(
            found[1].ends_with(format!("abcdef/{HASH}.jpg"))
                || found[1].ends_with(format!("abcdef\\{HASH}.jpg"))
        );
    }

    #[test]
    fn size_rules_decide_among_unnamed_files() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("1");
        write(&app, "big_square.jpg", 512, 512);
        write(&app, "tiny.jpg", 8, 8);
        write(&app, "wide.jpg", 64, 32);
        write(&app, "icon64.png", 64, 64);
        write(&app, "icon48.png", 48, 48);
        // A hash-named icon wins over unknown names, even when larger.
        write(&app, &format!("{HASH}.jpg"), 128, 128);
        std::fs::write(app.join("notes.txt"), b"x").unwrap();
        std::fs::write(app.join("broken.jpg"), b"not a jpeg").unwrap();
        let found = icon_files_in(&app);
        assert_eq!(
            names(&found),
            [
                format!("{HASH}.jpg"),
                "icon48.png".into(),
                "icon64.png".into()
            ]
        );
    }

    #[test]
    fn legacy_flat_icon_and_missing_folders() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "10_icon.jpg", 32, 32);
        let found = icon_candidates(tmp.path(), "10");
        assert_eq!(names(&found), ["10_icon.jpg"]);
        // Nothing cached: only the (missing) legacy path is offered and later skipped.
        assert_eq!(icon_candidates(tmp.path(), "999").len(), 1);
    }
}
