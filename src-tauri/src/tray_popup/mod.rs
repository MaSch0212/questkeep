//! The tray popup (Windows): a small native window shown from the tray icon instead of a context
//! menu, with recent games as cover tiles, favorites with icons, store launchers and the app
//! actions. Drawn with Direct2D/DirectWrite on its own thread, created when opened and destroyed
//! when it closes, so it costs nothing while the app waits in the tray. Other platforms keep the
//! native tray menu (`tray.rs`).
//!
//! `layout` is pure and tested; `render` draws a layout onto any Direct2D target (the window, or a
//! bitmap for the preview test); `window` owns the Win32 window and input.

mod a11y;
pub mod layout;
#[cfg(windows)]
mod render;
#[cfg(windows)]
mod window;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use questkeep_core::images::{IconRgba, load_icon_rgba, load_image_filled, scale_icon};
use questkeep_core::tray::{TrayGame, TrayGames};

pub use crate::tray::TrayLauncher;

pub use layout::{Action, ScreenRect};
use layout::{Content, Item, ItemKind, Layout, MAX_TILES};

#[cfg(windows)]
pub use window::show;

/// What the popup shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PopupData {
    pub games: TrayGames,
    pub launchers: Vec<TrayLauncher>,
    /// Version of an available app update.
    pub update: Option<String>,
}

impl PopupData {
    fn content(&self) -> Content {
        Content {
            recent: self.games.recent.iter().map(|g| g.id).collect(),
            favorites: self.games.favorites.iter().map(|g| g.id).collect(),
            launchers: self.launchers.iter().map(|l| l.id.clone()).collect(),
            update: self.update.clone(),
        }
    }

    fn game(&self, kind: ItemKind) -> Option<&TrayGame> {
        match kind {
            ItemKind::Tile(i) => self.games.recent.get(i),
            ItemKind::Row(i) => self.games.favorites.get(i),
            _ => None,
        }
    }
}

/// Premultiplied BGRA pixels, ready for a Direct2D bitmap.
#[derive(Debug, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Pixels {
    fn from_rgba(image: &IconRgba) -> Self {
        let mut bgra = image.rgba.clone();
        for p in bgra.as_chunks_mut::<4>().0 {
            let a = u32::from(p[3]);
            let premul = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
            let (r, g, b) = (premul(p[0]), premul(p[1]), premul(p[2]));
            p[0] = b;
            p[1] = g;
            p[2] = r;
        }
        Self {
            width: image.width,
            height: image.height,
            bgra,
        }
    }
}

/// Images at their exact on-screen pixel size, by item index of the layout.
#[derive(Default)]
pub struct Prepared {
    images: HashMap<usize, Arc<Pixels>>,
}

impl Prepared {
    pub fn get(&self, item: usize) -> Option<&Arc<Pixels>> {
        self.images.get(&item)
    }
}

/// Snaps a logical coordinate to the physical pixel grid.
pub fn snap(v: f32, scale: f32) -> f32 {
    (v * scale).round() / scale
}

/// Physical pixel size of a logical span starting at `start`, after snapping both ends.
pub fn pixel_span(start: f32, len: f32, scale: f32) -> u32 {
    ((start + len) * scale).round().max(0.0) as u32 - (start * scale).round().max(0.0) as u32
}

/// The rectangle of a row's icon, relative to the row.
pub fn row_icon_rect(row: &layout::Rect) -> layout::Rect {
    let inset = (layout::ROW_HEIGHT - layout::ICON_SIZE) / 2.0;
    layout::Rect::new(
        row.x + inset,
        row.y + inset,
        layout::ICON_SIZE,
        layout::ICON_SIZE,
    )
}

type CacheKey = (PathBuf, u32, u32);

/// Scaled images from earlier openings, so reopening the popup decodes nothing. Only the images
/// of the last opening are kept (a few hundred KB).
static IMAGE_CACHE: Mutex<Option<HashMap<CacheKey, Arc<Pixels>>>> = Mutex::new(None);

/// Loads every tile cover and row icon at the size it is drawn at `scale`.
pub fn prepare(data: &PopupData, layout: &Layout, scale: f32) -> Prepared {
    let mut cache = IMAGE_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    let old = cache.take().unwrap_or_default();
    let mut kept: HashMap<CacheKey, Arc<Pixels>> = HashMap::new();
    let mut prepared = Prepared::default();
    for (index, item) in layout.items.iter().enumerate() {
        let Some(game) = data.game(item.kind) else {
            continue;
        };
        let Some((path, w, h, kind)) = image_request(game, item, scale) else {
            continue;
        };
        let key = (path.to_path_buf(), w, h);
        let pixels = match kept.get(&key).or_else(|| old.get(&key)) {
            Some(p) => Some(Arc::clone(p)),
            None => load(path, w, h, kind).map(|img| Arc::new(Pixels::from_rgba(&img))),
        };
        if let Some(pixels) = pixels {
            kept.insert(key, Arc::clone(&pixels));
            prepared.images.insert(index, pixels);
        }
    }
    *cache = Some(kept);
    prepared
}

#[derive(Clone, Copy)]
enum ImageKind {
    Cover,
    Icon,
}

fn image_request<'a>(
    game: &'a TrayGame,
    item: &Item,
    scale: f32,
) -> Option<(&'a Path, u32, u32, ImageKind)> {
    match item.kind {
        ItemKind::Tile(i) if i < MAX_TILES => {
            let r = item.rect;
            let (w, h) = (pixel_span(r.x, r.w, scale), pixel_span(r.y, r.h, scale));
            game.cover.as_deref().map(|p| (p, w, h, ImageKind::Cover))
        }
        ItemKind::Row(_) => {
            let side = (layout::ICON_SIZE * scale).round() as u32;
            game.icon
                .as_deref()
                .map(|p| (p, side, side, ImageKind::Icon))
        }
        _ => None,
    }
}

fn load(path: &Path, w: u32, h: u32, kind: ImageKind) -> Option<IconRgba> {
    match kind {
        ImageKind::Cover => load_image_filled(path, w, h, false),
        ImageKind::Icon => load_icon_rgba(path).and_then(|icon| scale_icon(&icon, w)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_are_premultiplied_bgra() {
        let image = IconRgba {
            width: 2,
            height: 1,
            rgba: vec![255, 128, 0, 255, 200, 100, 50, 128],
        };
        let p = Pixels::from_rgba(&image);
        assert_eq!(&p.bgra[..4], &[0, 128, 255, 255]);
        assert_eq!(&p.bgra[4..], &[25, 50, 100, 128]);
    }

    #[test]
    fn spans_snap_to_whole_pixels() {
        assert_eq!(pixel_span(14.0, 87.5, 1.0), 88);
        assert_eq!(pixel_span(111.5, 87.5, 1.0), 87);
        assert_eq!(pixel_span(14.0, 87.5, 1.5), 131);
        assert_eq!(snap(10.3, 1.5), 10.0);
    }
}
