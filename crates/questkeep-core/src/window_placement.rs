//! Remembers where the main window was, so it reopens there (same monitor, position and size)
//! after it was closed to the tray or the app restarted. Stored as `window.json` in the data
//! folder (portable-friendly). All values are physical pixels in virtual-desktop coordinates.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Part of the window's top edge (its title bar) that must be on a monitor for a saved position to
/// be used; otherwise the window could not be dragged back.
const MIN_VISIBLE_WIDTH: i64 = 120;
const TITLE_BAR_HEIGHT: i64 = 32;

/// The window's last normal (not minimized, not maximized) rectangle, and whether it was maximized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

/// A monitor's work area (the screen minus the taskbar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowPlacement {
    /// The placement to restore on the current monitors: `None` when the saved title bar is not
    /// on any monitor any more (e.g. a disconnected screen), so the caller uses its default. A
    /// window larger than its monitor is shrunk to fit and moved inside it.
    pub fn fit(&self, monitors: &[MonitorArea]) -> Option<WindowPlacement> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let monitor = monitors
            .iter()
            .filter(|m| m.width > 0 && m.height > 0)
            .map(|m| (m, self.title_bar_overlap(m)))
            .filter(|(_, overlap)| *overlap > 0)
            .max_by_key(|(_, overlap)| *overlap)
            .map(|(m, _)| *m)?;
        let width = self.width.min(monitor.width);
        let height = self.height.min(monitor.height);
        let x = clamp_axis(self.x, width, monitor.x, monitor.width);
        let y = clamp_axis(self.y, height, monitor.y, monitor.height);
        Some(WindowPlacement {
            x,
            y,
            width,
            height,
            maximized: self.maximized,
        })
    }

    /// Area of the title-bar strip on `monitor`, or 0 when too little of it is visible.
    fn title_bar_overlap(&self, monitor: &MonitorArea) -> i64 {
        let left = i64::from(self.x).max(i64::from(monitor.x));
        let right = (i64::from(self.x) + i64::from(self.width))
            .min(i64::from(monitor.x) + i64::from(monitor.width));
        let top = i64::from(self.y).max(i64::from(monitor.y));
        let bottom = (i64::from(self.y) + TITLE_BAR_HEIGHT)
            .min(i64::from(monitor.y) + i64::from(monitor.height));
        let (w, h) = (right - left, bottom - top);
        let visible_width = MIN_VISIBLE_WIDTH.min(i64::from(self.width));
        if w < visible_width || h <= 0 {
            0
        } else {
            w * h
        }
    }
}

/// Moves a span of `len` starting at `pos` so it lies inside `[start, start + size)`.
fn clamp_axis(pos: i32, len: u32, start: i32, size: u32) -> i32 {
    let max = i64::from(start) + i64::from(size) - i64::from(len);
    let clamped = i64::from(pos).clamp(i64::from(start), max.max(i64::from(start)));
    i32::try_from(clamped).unwrap_or(start)
}

/// Loads and saves the placement file.
#[derive(Debug, Clone)]
pub struct PlacementStore {
    path: PathBuf,
}

impl PlacementStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved placement; a missing or unreadable file means none.
    pub fn load(&self) -> Option<WindowPlacement> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        match serde_json::from_str(&text) {
            Ok(placement) => Some(placement),
            Err(err) => {
                tracing::warn!(%err, path = %self.path.display(), "ignoring invalid window placement");
                None
            }
        }
    }

    pub fn save(&self, placement: &WindowPlacement) -> Result<()> {
        let json = serde_json::to_string_pretty(placement)?;
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: MonitorArea = MonitorArea {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    const LEFT: MonitorArea = MonitorArea {
        x: -2560,
        y: -200,
        width: 2560,
        height: 1400,
    };

    fn placement(x: i32, y: i32, width: u32, height: u32) -> WindowPlacement {
        WindowPlacement {
            x,
            y,
            width,
            height,
            maximized: false,
        }
    }

    #[test]
    fn keeps_a_window_on_a_second_monitor() {
        let saved = placement(-2000, 100, 1280, 800);
        assert_eq!(saved.fit(&[PRIMARY, LEFT]), Some(saved));
    }

    #[test]
    fn drops_a_window_whose_monitor_is_gone() {
        let saved = placement(-2000, 100, 1280, 800);
        assert_eq!(saved.fit(&[PRIMARY]), None);
    }

    #[test]
    fn needs_part_of_the_title_bar_on_screen() {
        // Only 50 px of the title bar on the primary monitor: not enough to grab it.
        assert_eq!(placement(1870, 100, 1280, 800).fit(&[PRIMARY]), None);
        // Title bar above the top of the screen.
        assert_eq!(placement(100, -500, 1280, 400).fit(&[PRIMARY]), None);
    }

    #[test]
    fn moves_a_partly_offscreen_window_inside() {
        let fitted = placement(1500, 600, 1280, 800).fit(&[PRIMARY]).unwrap();
        assert_eq!((fitted.x, fitted.y), (640, 240));
        assert_eq!((fitted.width, fitted.height), (1280, 800));
    }

    #[test]
    fn shrinks_a_window_larger_than_its_monitor() {
        let fitted = placement(10, 10, 3000, 2000).fit(&[PRIMARY]).unwrap();
        assert_eq!(fitted, placement(0, 0, 1920, 1040));
    }

    #[test]
    fn picks_the_monitor_holding_most_of_the_title_bar() {
        // Straddles both monitors, mostly on the left one.
        let fitted = placement(-1000, 100, 1280, 800)
            .fit(&[PRIMARY, LEFT])
            .unwrap();
        assert_eq!(fitted.x, -1280);
    }

    #[test]
    fn keeps_maximized() {
        let mut saved = placement(100, 100, 1280, 800);
        saved.maximized = true;
        assert!(saved.fit(&[PRIMARY]).unwrap().maximized);
    }

    #[test]
    fn rejects_empty_sizes() {
        assert_eq!(placement(0, 0, 0, 800).fit(&[PRIMARY]), None);
    }

    #[test]
    fn store_roundtrip_and_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlacementStore::new(dir.path().join("window.json"));
        assert_eq!(store.load(), None);
        let saved = placement(-2000, 100, 1280, 800);
        store.save(&saved).unwrap();
        assert_eq!(store.load(), Some(saved));
        std::fs::write(store.path(), "not json").unwrap();
        assert_eq!(store.load(), None);
    }
}
