//! Layout, hit testing and keyboard navigation of the tray popup, in logical pixels. Pure, so it
//! is unit tested here; the Windows renderer only draws what this computes.
//!
//! ```text
//! Recently played
//! [tile] [tile] [tile] [tile]      cover tiles, 2:3, up to two rows
//! [tile] [tile] ...
//! Favorites
//! (icon) Name                      rows, scrollable when there are many
//! Launchers
//! [logo] [logo] [logo] [logo]      launcher tiles, logo above the name, four per row
//! ─────────────────────────────
//! [Open Questkeep] [Update to x.y]   [Exit]
//! ```

use uuid::Uuid;

pub const WIDTH: f32 = 408.0;
pub const PADDING: f32 = 14.0;
pub const LABEL_HEIGHT: f32 = 30.0;
pub const COLUMNS: usize = 4;
pub const TILE_GAP: f32 = 10.0;
/// Most recent games shown as tiles (two rows).
pub const MAX_TILES: usize = 8;
pub const ROW_HEIGHT: f32 = 40.0;
pub const ICON_SIZE: f32 = 28.0;
/// Favorites rows visible at once; more scroll.
pub const MAX_VISIBLE_ROWS: usize = 6;
pub const LAUNCHER_HEIGHT: f32 = 64.0;
pub const LAUNCHER_ICON_SIZE: f32 = 24.0;
pub const BUTTON_GAP: f32 = 8.0;
pub const FOOTER_HEIGHT: f32 = 52.0;
pub const BUTTON_HEIGHT: f32 = 34.0;
pub const BUTTON_PADDING: f32 = 16.0;
pub const EMPTY_HEIGHT: f32 = 36.0;
pub const SECTION_GAP: f32 = 6.0;

pub const RECENT_LABEL: &str = "Recently played";
pub const FAVORITES_LABEL: &str = "Favorites";
pub const LAUNCHERS_LABEL: &str = "Launchers";
pub const EMPTY_RECENT: &str = "No recently played games yet";
pub const OPEN_LABEL: &str = "Open Questkeep";
pub const EXIT_LABEL: &str = "Exit";

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    fn offset_y(self, dy: f32) -> Self {
        Self {
            y: self.y + dy,
            ..self
        }
    }
}

/// What activating an item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Launch(Uuid),
    /// Opens a store's launcher (provider id).
    OpenLauncher(String),
    OpenWindow,
    /// Opens the window with its update dialog.
    ShowUpdate,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// Recent game `n` as a cover tile.
    Tile(usize),
    /// Favorite `n` as a row.
    Row(usize),
    /// Store launcher `n` as a logo tile.
    Launcher(usize),
    OpenButton,
    /// Shown when an app update is available.
    UpdateButton,
    ExitButton,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub action: Action,
    /// Where the item is drawn, with the favorites scroll applied.
    pub rect: Rect,
}

/// What the popup lists.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Content {
    pub recent: Vec<Uuid>,
    pub favorites: Vec<Uuid>,
    /// Provider ids of the installed store launchers.
    pub launchers: Vec<String>,
    /// Version of an available app update.
    pub update: Option<String>,
}

/// Label of the update button.
pub fn update_label(version: &str) -> String {
    format!("Update to {version}")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub items: Vec<Item>,
    /// Section labels and the empty-state text: (text, rect).
    pub labels: Vec<(&'static str, Rect)>,
    pub empty_recent: Option<Rect>,
    /// Horizontal line above the footer.
    pub separator_y: f32,
    /// Visible part of the favorites list; rows outside it are clipped and not clickable.
    pub favorites_clip: Option<Rect>,
    /// Largest valid favorites scroll offset.
    pub max_scroll: f32,
}

/// Lays the popup out. `measure` returns the width of a label in logical pixels (button and pill
/// widths depend on it); `scroll` is the favorites scroll offset, clamped to the valid range.
pub fn layout(content: &Content, scroll: f32, measure: impl Fn(&str) -> f32) -> Layout {
    let inner = WIDTH - 2.0 * PADDING;
    let mut items = Vec::new();
    let mut labels = Vec::new();
    let mut y = PADDING / 2.0;

    labels.push((RECENT_LABEL, Rect::new(PADDING, y, inner, LABEL_HEIGHT)));
    y += LABEL_HEIGHT;
    let mut empty_recent = None;
    let tiles = content.recent.len().min(MAX_TILES);
    if tiles == 0 {
        empty_recent = Some(Rect::new(PADDING, y, inner, EMPTY_HEIGHT));
        y += EMPTY_HEIGHT;
    } else {
        let tile_w = (inner - TILE_GAP * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
        let tile_h = (tile_w * 1.5).round();
        for (i, id) in content.recent.iter().take(MAX_TILES).enumerate() {
            let (row, col) = (i / COLUMNS, i % COLUMNS);
            let rect = Rect::new(
                PADDING + col as f32 * (tile_w + TILE_GAP),
                y + row as f32 * (tile_h + TILE_GAP),
                tile_w,
                tile_h,
            );
            items.push(Item {
                kind: ItemKind::Tile(i),
                action: Action::Launch(*id),
                rect,
            });
        }
        let rows = tiles.div_ceil(COLUMNS);
        y += rows as f32 * tile_h + (rows as f32 - 1.0) * TILE_GAP;
    }

    let mut favorites_clip = None;
    let mut max_scroll = 0.0;
    if !content.favorites.is_empty() {
        y += SECTION_GAP;
        labels.push((FAVORITES_LABEL, Rect::new(PADDING, y, inner, LABEL_HEIGHT)));
        y += LABEL_HEIGHT;
        let visible = content.favorites.len().min(MAX_VISIBLE_ROWS);
        let clip = Rect::new(PADDING, y, inner, visible as f32 * ROW_HEIGHT);
        max_scroll = (content.favorites.len() - visible) as f32 * ROW_HEIGHT;
        let scroll = scroll.clamp(0.0, max_scroll);
        for (i, id) in content.favorites.iter().enumerate() {
            let rect = Rect::new(PADDING, y + i as f32 * ROW_HEIGHT, inner, ROW_HEIGHT);
            items.push(Item {
                kind: ItemKind::Row(i),
                action: Action::Launch(*id),
                rect: rect.offset_y(-scroll),
            });
        }
        favorites_clip = Some(clip);
        y += clip.h;
    }

    if !content.launchers.is_empty() {
        y += SECTION_GAP;
        labels.push((LAUNCHERS_LABEL, Rect::new(PADDING, y, inner, LABEL_HEIGHT)));
        y += LABEL_HEIGHT;
        let tile_w = (inner - TILE_GAP * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
        for (i, id) in content.launchers.iter().enumerate() {
            let (row, col) = (i / COLUMNS, i % COLUMNS);
            items.push(Item {
                kind: ItemKind::Launcher(i),
                action: Action::OpenLauncher(id.clone()),
                rect: Rect::new(
                    PADDING + col as f32 * (tile_w + TILE_GAP),
                    y + row as f32 * (LAUNCHER_HEIGHT + TILE_GAP),
                    tile_w,
                    LAUNCHER_HEIGHT,
                ),
            });
        }
        let rows = content.launchers.len().div_ceil(COLUMNS);
        y += rows as f32 * LAUNCHER_HEIGHT + (rows as f32 - 1.0) * TILE_GAP;
    }

    y += PADDING;
    let separator_y = y;
    let button_y = y + (FOOTER_HEIGHT - BUTTON_HEIGHT) / 2.0;
    let open_w = (measure(OPEN_LABEL) + 2.0 * BUTTON_PADDING).ceil();
    let exit_w = (measure(EXIT_LABEL) + 2.0 * BUTTON_PADDING).ceil();
    items.push(Item {
        kind: ItemKind::OpenButton,
        action: Action::OpenWindow,
        rect: Rect::new(PADDING, button_y, open_w, BUTTON_HEIGHT),
    });
    if let Some(version) = &content.update {
        let update_w = (measure(&update_label(version)) + 2.0 * BUTTON_PADDING).ceil();
        items.push(Item {
            kind: ItemKind::UpdateButton,
            action: Action::ShowUpdate,
            rect: Rect::new(
                PADDING + open_w + BUTTON_GAP,
                button_y,
                update_w,
                BUTTON_HEIGHT,
            ),
        });
    }
    items.push(Item {
        kind: ItemKind::ExitButton,
        action: Action::Exit,
        rect: Rect::new(WIDTH - PADDING - exit_w, button_y, exit_w, BUTTON_HEIGHT),
    });

    Layout {
        width: WIDTH,
        height: (separator_y + FOOTER_HEIGHT).ceil(),
        items,
        labels,
        empty_recent,
        separator_y,
        favorites_clip,
        max_scroll,
    }
}

impl Layout {
    /// Whether an item can be seen (favorites rows scrolled out of view cannot).
    pub fn visible(&self, item: &Item) -> bool {
        match (item.kind, self.favorites_clip) {
            (ItemKind::Row(_), Some(clip)) => {
                item.rect.bottom() > clip.y + 0.5 && item.rect.y < clip.bottom() - 0.5
            }
            _ => true,
        }
    }

    /// The item under a point.
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.items.iter().position(|item| {
            item.rect.contains(x, y)
                && match (item.kind, self.favorites_clip) {
                    (ItemKind::Row(_), Some(clip)) => clip.contains(x, y),
                    _ => true,
                }
        })
    }

    /// The scroll offset that brings item `index` fully into view, starting from `scroll`.
    pub fn reveal(&self, index: usize, scroll: f32) -> f32 {
        let (Some(item), Some(clip)) = (self.items.get(index), self.favorites_clip) else {
            return scroll;
        };
        if !matches!(item.kind, ItemKind::Row(_)) {
            return scroll;
        }
        let new = if item.rect.y < clip.y {
            scroll - (clip.y - item.rect.y)
        } else if item.rect.bottom() > clip.bottom() {
            scroll + (item.rect.bottom() - clip.bottom())
        } else {
            scroll
        };
        new.clamp(0.0, self.max_scroll)
    }
}

/// A navigation key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Next,
    Previous,
    First,
    Last,
}

/// The item focused after pressing `nav` with `current` focused (`None`: nothing focused yet, so
/// any key focuses the first item). Arrows move spatially to the nearest item in that direction
/// (favorites rows are compared at their unscrolled position, so the whole list is reachable);
/// Tab order wraps around.
pub fn navigate(layout: &Layout, current: Option<usize>, nav: Nav) -> Option<usize> {
    let count = layout.items.len();
    if count == 0 {
        return None;
    }
    let Some(current) = current.filter(|&c| c < count) else {
        return Some(if nav == Nav::Last || nav == Nav::Previous {
            count - 1
        } else {
            0
        });
    };
    match nav {
        Nav::Next => Some((current + 1) % count),
        Nav::Previous => Some((current + count - 1) % count),
        Nav::First => Some(0),
        Nav::Last => Some(count - 1),
        Nav::Up | Nav::Down | Nav::Left | Nav::Right => {
            Some(spatial(layout, current, nav).unwrap_or(current))
        }
    }
}

fn spatial(layout: &Layout, current: usize, nav: Nav) -> Option<usize> {
    let row_count = layout
        .items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::Row(_)))
        .count();
    let mut from = layout.items[current].rect;
    if let ItemKind::Row(n) = layout.items[current].kind {
        // Inside the favorites list Up/Down step through it (scrolling as needed); leaving it,
        // the whole list counts as one block.
        let step = match nav {
            Nav::Up if n > 0 => Some(current - 1),
            Nav::Down if n + 1 < row_count => Some(current + 1),
            _ => None,
        };
        if step.is_some() {
            return step;
        }
        from = layout.favorites_clip.unwrap_or(from);
    }
    let (fx, fy) = from.center();
    // (edge gap, center offset, index) of every item in that direction. Rows scrolled out of view
    // are skipped, so entering the list lands on its first or last visible row.
    let candidates: Vec<(f32, f32, usize)> = layout
        .items
        .iter()
        .enumerate()
        .filter(|(i, item)| {
            *i != current
                && layout.visible(item)
                && !(matches!(layout.items[current].kind, ItemKind::Row(_))
                    && matches!(item.kind, ItemKind::Row(_)))
        })
        .filter_map(|(i, item)| {
            let to = item.rect;
            let (tx, ty) = to.center();
            match nav {
                Nav::Up if to.bottom() <= from.y + 0.5 => {
                    Some((from.y - to.bottom(), (tx - fx).abs(), i))
                }
                Nav::Down if to.y >= from.bottom() - 0.5 => {
                    Some((to.y - from.bottom(), (tx - fx).abs(), i))
                }
                Nav::Left if to.right() <= from.x + 0.5 && overlaps_y(&from, &to) => {
                    Some((from.x - to.right(), (ty - fy).abs(), i))
                }
                Nav::Right if to.x >= from.right() - 0.5 && overlaps_y(&from, &to) => {
                    Some((to.x - from.right(), (ty - fy).abs(), i))
                }
                _ => None,
            }
        })
        .collect();
    // The nearest band (row or column) first, then the item closest to the current one in it.
    let nearest = candidates.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    candidates
        .into_iter()
        .filter(|c| c.0 <= nearest + 1.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|c| c.2)
}

fn overlaps_y(a: &Rect, b: &Rect) -> bool {
    a.y < b.bottom() && b.y < a.bottom()
}

/// A rectangle in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where a popup of `w` x `h` physical pixels goes for a tray icon at `anchor` on a monitor
/// whose work area (the screen without the taskbar) is `work`: next to the taskbar, centered on
/// the icon, `gap` pixels from the edges, and always inside the work area.
pub fn place(anchor: ScreenRect, work: ScreenRect, w: i32, h: i32, gap: i32) -> (i32, i32) {
    let (cx, cy) = (anchor.x + anchor.w / 2, anchor.y + anchor.h / 2);
    let clamp = |v: i32, lo: i32, hi: i32| v.min(hi).max(lo);
    // When the popup is larger than the work area, its top-left corner stays visible.
    let x_range = (work.x + gap, (work.x + work.w - w - gap).max(work.x + gap));
    let y_range = (work.y + gap, (work.y + work.h - h - gap).max(work.y + gap));
    let centered_x = clamp(cx - w / 2, x_range.0, x_range.1);
    let centered_y = clamp(cy - h / 2, y_range.0, y_range.1);
    if cy >= work.y + work.h {
        (centered_x, y_range.1) // taskbar at the bottom
    } else if cy < work.y {
        (centered_x, y_range.0) // top
    } else if cx >= work.x + work.w {
        (x_range.1, centered_y) // right
    } else if cx < work.x {
        (x_range.0, centered_y) // left
    } else {
        // Icon inside the work area (overflow flyout, auto-hide taskbar): near the icon, bottom right.
        (centered_x, clamp(cy - h - gap, y_range.0, y_range.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<Uuid> {
        (0..n).map(|i| Uuid::from_u128(i as u128 + 1)).collect()
    }

    fn content(recent: usize, favorites: usize, launchers: usize) -> Content {
        Content {
            recent: ids(recent),
            favorites: ids(favorites),
            launchers: (0..launchers).map(|i| format!("store{i}")).collect(),
            update: None,
        }
    }

    /// 7 px per character, like a 12-14 px UI font.
    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 7.0
    }

    fn kinds(layout: &Layout) -> Vec<ItemKind> {
        layout.items.iter().map(|i| i.kind).collect()
    }

    #[test]
    fn full_popup_has_tiles_rows_launchers_and_buttons_in_tab_order() {
        let l = layout(&content(10, 2, 2), 0.0, measure);
        let k = kinds(&l);
        assert_eq!(
            k.len(),
            8 + 2 + 2 + 2,
            "recent is capped at two rows of tiles"
        );
        assert_eq!(k[0], ItemKind::Tile(0));
        assert_eq!(k[8], ItemKind::Row(0));
        assert_eq!(k[10], ItemKind::Launcher(0));
        assert_eq!(l.items[10].action, Action::OpenLauncher("store0".into()));
        assert_eq!(k[12], ItemKind::OpenButton);
        assert_eq!(k[13], ItemKind::ExitButton);
        assert_eq!(
            l.labels.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
            vec![RECENT_LABEL, FAVORITES_LABEL, LAUNCHERS_LABEL]
        );
        // Everything inside the popup, and the footer at the bottom.
        for item in &l.items {
            assert!(item.rect.x >= PADDING - 0.01 && item.rect.right() <= WIDTH - PADDING + 0.01);
            assert!(item.rect.bottom() <= l.height);
        }
        assert!(l.items[13].rect.y > l.separator_y);
    }

    #[test]
    fn tiles_are_portrait_and_in_four_columns() {
        let l = layout(&content(5, 0, 0), 0.0, measure);
        let (a, b, e) = (l.items[0].rect, l.items[1].rect, l.items[4].rect);
        assert!((a.h / a.w - 1.5).abs() < 0.02);
        assert!((b.x - a.right() - TILE_GAP).abs() < 0.01);
        assert_eq!(e.x, a.x, "fifth tile starts the second row");
        assert!((e.y - a.bottom() - TILE_GAP).abs() < 0.01);
    }

    #[test]
    fn update_button_sits_between_open_and_exit() {
        let c = Content {
            update: Some("0.3.0".into()),
            ..Content::default()
        };
        let l = layout(&c, 0.0, measure);
        assert_eq!(
            kinds(&l),
            vec![
                ItemKind::OpenButton,
                ItemKind::UpdateButton,
                ItemKind::ExitButton
            ]
        );
        let (open, update, exit) = (l.items[0].rect, l.items[1].rect, l.items[2].rect);
        assert_eq!(l.items[1].action, Action::ShowUpdate);
        assert_eq!(update.y, open.y);
        assert!(update.x >= open.right() && update.right() <= exit.x);
        assert_eq!(navigate(&l, Some(0), Nav::Right), Some(1));
        assert_eq!(navigate(&l, Some(1), Nav::Right), Some(2));
    }

    #[test]
    fn empty_sections() {
        let l = layout(&Content::default(), 0.0, measure);
        assert!(l.empty_recent.is_some());
        assert_eq!(kinds(&l), vec![ItemKind::OpenButton, ItemKind::ExitButton]);
        assert_eq!(
            l.labels.len(),
            1,
            "no favorites or launchers label without entries"
        );
        assert!(l.favorites_clip.is_none());
    }

    #[test]
    fn many_favorites_scroll_and_are_clipped() {
        let c = content(0, 10, 0);
        let l = layout(&c, 0.0, measure);
        let clip = l.favorites_clip.unwrap();
        assert_eq!(clip.h, MAX_VISIBLE_ROWS as f32 * ROW_HEIGHT);
        assert_eq!(l.max_scroll, 4.0 * ROW_HEIGHT);
        assert!(l.visible(&l.items[0]));
        assert!(!l.visible(&l.items[7]));
        // Row 7 sits below the clip: clicking where it would be does not hit it.
        let r = l.items[7].rect;
        assert_ne!(l.hit(r.x + 5.0, r.y + 5.0), Some(7));

        // Scrolled to the end: row 9 is visible, row 0 is not; scroll is clamped.
        let l = layout(&c, 10_000.0, measure);
        assert!(l.visible(&l.items[9]));
        assert!(!l.visible(&l.items[0]));
        let r = l.items[9].rect;
        assert_eq!(l.hit(r.x + 5.0, r.y + 5.0), Some(9));
    }

    #[test]
    fn reveal_scrolls_just_enough() {
        let c = content(0, 10, 0);
        let l = layout(&c, 0.0, measure);
        assert_eq!(l.reveal(2, 0.0), 0.0);
        assert_eq!(l.reveal(7, 0.0), 2.0 * ROW_HEIGHT);
        let l = layout(&c, 4.0 * ROW_HEIGHT, measure);
        assert_eq!(l.reveal(0, 4.0 * ROW_HEIGHT), 0.0);
        // Non-row items never scroll.
        assert_eq!(l.reveal(10, 40.0), 40.0);
    }

    #[test]
    fn launchers_are_tiles_four_per_row() {
        let l = layout(&content(0, 0, 6), 0.0, measure);
        let tiles: Vec<Rect> = l
            .items
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Launcher(_)))
            .map(|i| i.rect)
            .collect();
        assert_eq!(tiles.len(), 6);
        assert_eq!(tiles[0].h, LAUNCHER_HEIGHT);
        assert_eq!(tiles[3].y, tiles[0].y);
        assert_eq!(
            tiles[4].x, tiles[0].x,
            "fifth launcher starts the second row"
        );
        assert!((tiles[4].y - tiles[0].bottom() - TILE_GAP).abs() < 0.01);
        assert!(tiles.iter().all(|t| t.right() <= WIDTH - PADDING + 0.01));
        assert!(tiles[5].bottom() < l.separator_y);
    }

    #[test]
    fn hit_testing() {
        let l = layout(&content(3, 1, 1), 0.0, measure);
        let t = l.items[1].rect;
        assert_eq!(l.hit(t.x + 1.0, t.y + 1.0), Some(1));
        assert_eq!(
            l.hit(t.x - TILE_GAP / 2.0, t.y + 1.0),
            None,
            "gap between tiles"
        );
        assert_eq!(l.hit(-1.0, -1.0), None);
    }

    #[test]
    fn arrow_keys_move_through_the_grid_and_sections() {
        let l = layout(&content(6, 2, 1), 0.0, measure);
        let nav = |from, n| navigate(&l, Some(from), n);
        assert_eq!(navigate(&l, None, Nav::Down), Some(0));
        assert_eq!(nav(0, Nav::Right), Some(1));
        assert_eq!(nav(1, Nav::Down), Some(5), "tile below");
        assert_eq!(nav(0, Nav::Left), Some(0), "edge: stays");
        assert_eq!(
            nav(3, Nav::Down),
            Some(5),
            "no tile below: nearest in the next row"
        );
        assert_eq!(
            nav(5, Nav::Down),
            Some(6),
            "from the grid into the favorites"
        );
        assert_eq!(nav(6, Nav::Down), Some(7));
        assert_eq!(nav(7, Nav::Down), Some(8), "into the launchers");
        assert_eq!(nav(8, Nav::Down), Some(9), "to Open Questkeep");
        assert_eq!(nav(9, Nav::Right), Some(10), "to Exit");
        assert_eq!(nav(6, Nav::Up), Some(5), "back up to the nearest tile");
    }

    #[test]
    fn arrows_reach_scrolled_out_rows() {
        let l = layout(&content(0, 10, 0), 0.0, measure);
        let mut at = Some(0);
        for _ in 0..9 {
            at = navigate(&l, at, Nav::Down);
        }
        assert_eq!(at, Some(9));
        assert_eq!(navigate(&l, at, Nav::Down), Some(10), "then the footer");
    }

    #[test]
    fn tab_order_wraps() {
        let l = layout(&content(1, 0, 0), 0.0, measure);
        assert_eq!(navigate(&l, Some(2), Nav::Next), Some(0));
        assert_eq!(navigate(&l, Some(0), Nav::Previous), Some(2));
        assert_eq!(navigate(&l, None, Nav::Previous), Some(2));
        assert_eq!(navigate(&l, Some(1), Nav::First), Some(0));
        assert_eq!(navigate(&l, Some(0), Nav::Last), Some(2));
    }

    #[test]
    fn popup_sits_next_to_the_taskbar() {
        let work = ScreenRect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1032,
        };
        let icon = |x, y| ScreenRect { x, y, w: 24, h: 48 };
        // Bottom taskbar, icon near the right: above it, pushed inside the screen.
        assert_eq!(place(icon(1800, 1032), work, 400, 600, 12), (1508, 420));
        // Icon in the middle of a bottom taskbar: centered on it.
        assert_eq!(place(icon(900, 1032), work, 400, 600, 12), (712, 420));
        // Top taskbar.
        let top = ScreenRect {
            x: 0,
            y: 48,
            w: 1920,
            h: 1032,
        };
        assert_eq!(place(icon(900, 0), top, 400, 600, 12).1, 60);
        // Right taskbar.
        let right = ScreenRect {
            x: 0,
            y: 0,
            w: 1872,
            h: 1080,
        };
        let p = place(
            ScreenRect {
                x: 1880,
                y: 900,
                w: 48,
                h: 24,
            },
            right,
            400,
            600,
            12,
        );
        assert_eq!(p, (1460, 468));
        // Second monitor left of the primary (negative coordinates).
        let left = ScreenRect {
            x: -2560,
            y: 0,
            w: 2560,
            h: 1392,
        };
        let p = place(icon(-300, 1392), left, 400, 600, 12);
        assert_eq!(p, (-488, 780));
        // Taller than the screen: top edge stays visible.
        assert_eq!(place(icon(900, 1032), work, 400, 2000, 12).1, 12);
    }
}
