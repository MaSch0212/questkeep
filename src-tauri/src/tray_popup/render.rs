//! Draws a popup [`Layout`] with Direct2D and DirectWrite onto any render target: the popup
//! window, or a WIC bitmap in the preview test. Everything is drawn in logical pixels (the target's
//! DPI is set to the monitor's), and images are drawn 1:1 at pixel-snapped positions, so covers
//! and icons are never resampled by the GPU.

#![allow(unsafe_code)]

use std::collections::HashMap;

use uuid::Uuid;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_GRADIENT_STOP,
    D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
    D2D1_BITMAP_PROPERTIES, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1Bitmap, ID2D1Brush, ID2D1Factory, ID2D1RenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_FAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use super::layout::{self, ItemKind, Layout, Rect};
use super::{PopupData, Prepared, row_icon_rect, snap};

/// Colors of the dark app theme (`ui/src/theme.css`), as 0xRRGGBB.
mod color {
    pub const SURFACE: u32 = 0x171a20;
    pub const RAISED: u32 = 0x1e222a;
    pub const HOVER: u32 = 0x262b35;
    pub const BORDER: u32 = 0x2a2f3a;
    pub const TEXT: u32 = 0xe6e8ec;
    pub const MUTED: u32 = 0x9aa1ae;
    pub const FAINT: u32 = 0x6c7381;
    pub const ACCENT: u32 = 0x7c9cff;
    pub const ACCENT_HOVER: u32 = 0x96b0ff;
    pub const ACCENT_CONTRAST: u32 = 0x0e1220;
    pub const COVER_BG: u32 = 0x0d0f13;
    pub const WHITE: u32 = 0xffffff;
}

const TILE_RADIUS: f32 = 8.0;
const ROW_RADIUS: f32 = 6.0;
const ICON_RADIUS: f32 = 6.0;
const BITMAP_ICON_RADIUS: f32 = 4.0;
const BUTTON_RADIUS: f32 = 8.0;
const FOCUS_WIDTH: f32 = 2.0;
const FONT: windows::core::PCWSTR = w!("Segoe UI Variable Text");

fn rgb(hex: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

fn hsl(h: f32, s: f32, l: f32) -> D2D1_COLOR_F {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    D2D1_COLOR_F {
        r: r + m,
        g: g + m,
        b: b + m,
        a: 1.0,
    }
}

/// Stable hue for a game's placeholder.
fn hue(id: Uuid) -> f32 {
    let sum = id.as_bytes().iter().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(*b))
    });
    (sum % 360) as f32
}

fn d2d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}

fn rounded(r: Rect, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: d2d_rect(r),
        radiusX: radius,
        radiusY: radius,
    }
}

fn inflate(r: Rect, by: f32) -> Rect {
    Rect::new(r.x - by, r.y - by, r.w + 2.0 * by, r.h + 2.0 * by)
}

fn snapped(r: Rect, scale: f32) -> Rect {
    let (x, y) = (snap(r.x, scale), snap(r.y, scale));
    Rect::new(
        x,
        y,
        snap(r.right(), scale) - x,
        snap(r.bottom(), scale) - y,
    )
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Interaction state the window tracks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewState {
    pub hover: Option<usize>,
    pub focus: Option<usize>,
    /// Focus rings only after keyboard use.
    pub show_focus: bool,
    pub pressed: Option<usize>,
}

/// Device-dependent bitmaps for one render target, keyed by the prepared image's address.
#[derive(Default)]
pub struct Bitmaps {
    bitmaps: HashMap<usize, ID2D1Bitmap>,
}

impl Bitmaps {
    /// Must be called when the render target is recreated.
    pub fn clear(&mut self) {
        self.bitmaps.clear();
    }
}

/// Everything one drawn frame depends on.
pub struct Frame<'a> {
    pub layout: &'a Layout,
    pub data: &'a PopupData,
    pub images: &'a Prepared,
    pub state: &'a ViewState,
    pub scale: f32,
}

pub struct Renderer {
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    label: IDWriteTextFormat,
    row: IDWriteTextFormat,
    button: IDWriteTextFormat,
    tile_name: IDWriteTextFormat,
    placeholder: IDWriteTextFormat,
    empty: IDWriteTextFormat,
    letter: IDWriteTextFormat,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        // SAFETY: plain factory creation; the returned COM objects manage their own lifetime.
        let (factory, dwrite) = unsafe {
            (
                D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)?,
            )
        };
        let format =
            |size: f32, weight: DWRITE_FONT_WEIGHT, wrap: bool| -> Result<IDWriteTextFormat> {
                // SAFETY: valid font name and locale strings; the format is configured before use.
                unsafe {
                    let f = dwrite.CreateTextFormat(
                        FONT,
                        None,
                        weight,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        size,
                        w!(""),
                    )?;
                    f.SetWordWrapping(if wrap {
                        DWRITE_WORD_WRAPPING_WRAP
                    } else {
                        DWRITE_WORD_WRAPPING_NO_WRAP
                    })?;
                    f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                    let sign = dwrite.CreateEllipsisTrimmingSign(&f)?;
                    let trimming = DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        delimiter: 0,
                        delimiterCount: 0,
                    };
                    f.SetTrimming(&trimming, &sign)?;
                    Ok(f)
                }
            };
        let tile_name = format(12.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, true)?;
        let placeholder = format(13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, true)?;
        let letter = format(15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, false)?;
        // SAFETY: setting alignment on formats created above.
        unsafe {
            tile_name.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_FAR)?;
            placeholder.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            letter.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        }
        Ok(Self {
            label: format(12.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, false)?,
            row: format(14.0, DWRITE_FONT_WEIGHT_NORMAL, false)?,
            button: format(13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, false)?,
            empty: format(13.0, DWRITE_FONT_WEIGHT_NORMAL, false)?,
            tile_name,
            placeholder,
            letter,
            factory,
            dwrite,
        })
    }

    pub fn factory(&self) -> &ID2D1Factory {
        &self.factory
    }

    /// Width of a button or pill label in logical pixels.
    pub fn measure(&self, text: &str) -> f32 {
        let text = wide(text);
        // SAFETY: the layout only reads `text` during the call.
        unsafe {
            let Ok(layout) = self
                .dwrite
                .CreateTextLayout(&text, &self.button, 10_000.0, 100.0)
            else {
                return text.len() as f32 * 7.0;
            };
            let mut metrics = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_err() {
                return text.len() as f32 * 7.0;
            }
            metrics.widthIncludingTrailingWhitespace
        }
    }

    /// Draws the whole popup between `BeginDraw` and `EndDraw`, which the caller does.
    pub fn draw(
        &self,
        target: &ID2D1RenderTarget,
        frame: &Frame,
        bitmaps: &mut Bitmaps,
    ) -> Result<()> {
        let painter = Painter {
            target,
            renderer: self,
            scale: frame.scale,
        };
        painter.draw(frame.layout, frame.data, frame.images, bitmaps, frame.state)
    }
}

struct Painter<'a> {
    target: &'a ID2D1RenderTarget,
    renderer: &'a Renderer,
    scale: f32,
}

impl Painter<'_> {
    fn brush(&self, hex: u32, alpha: f32) -> Result<ID2D1Brush> {
        // SAFETY: creating a brush on a valid target.
        let brush = unsafe { self.target.CreateSolidColorBrush(&rgb(hex, alpha), None)? };
        Ok(brush.into())
    }

    fn fill(&self, r: Rect, radius: f32, brush: &ID2D1Brush) {
        // SAFETY: drawing calls on a target inside BeginDraw/EndDraw.
        unsafe {
            if radius > 0.0 {
                self.target.FillRoundedRectangle(&rounded(r, radius), brush);
            } else {
                self.target.FillRectangle(&d2d_rect(r), brush);
            }
        }
    }

    fn stroke(&self, r: Rect, radius: f32, brush: &ID2D1Brush, width: f32) {
        // SAFETY: as above.
        unsafe {
            self.target
                .DrawRoundedRectangle(&rounded(r, radius), brush, width, None);
        }
    }

    fn text(&self, text: &str, format: &IDWriteTextFormat, r: Rect, brush: &ID2D1Brush) {
        let text = wide(text);
        // SAFETY: as above; `text` outlives the call.
        unsafe {
            self.target.DrawText(
                &text,
                format,
                &d2d_rect(r),
                brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    fn bitmap(
        &self,
        index: usize,
        images: &Prepared,
        bitmaps: &mut Bitmaps,
    ) -> Result<Option<ID2D1Bitmap>> {
        let Some(pixels) = images.get(index) else {
            return Ok(None);
        };
        let key = std::sync::Arc::as_ptr(pixels) as usize;
        if let Some(b) = bitmaps.bitmaps.get(&key) {
            return Ok(Some(b.clone()));
        }
        let dpi = 96.0 * self.scale;
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi,
            dpiY: dpi,
        };
        // SAFETY: `bgra` holds width * height * 4 bytes with a pitch of width * 4; D2D copies it.
        let bitmap = unsafe {
            self.target.CreateBitmap(
                D2D_SIZE_U {
                    width: pixels.width,
                    height: pixels.height,
                },
                Some(pixels.bgra.as_ptr().cast()),
                pixels.width * 4,
                &props,
            )?
        };
        bitmaps.bitmaps.insert(key, bitmap.clone());
        Ok(Some(bitmap))
    }

    /// Fills `r` (pixel-snapped, the bitmap's exact size) with the bitmap, clipped to rounded
    /// corners.
    fn fill_bitmap(&self, r: Rect, radius: f32, bitmap: &ID2D1Bitmap) -> Result<()> {
        // SAFETY: as above; the brush transform only moves the bitmap to the rectangle.
        unsafe {
            let brush = self.target.CreateBitmapBrush(bitmap, None, None)?;
            brush.SetInterpolationMode(D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
            brush.SetTransform(&Matrix3x2::translation(r.x, r.y));
            self.target
                .FillRoundedRectangle(&rounded(r, radius), &brush);
        }
        Ok(())
    }

    fn gradient(
        &self,
        r: Rect,
        stops: &[D2D1_GRADIENT_STOP],
        horizontal: bool,
    ) -> Result<ID2D1Brush> {
        let end = if horizontal {
            Vector2::new(r.right(), r.bottom())
        } else {
            Vector2::new(r.x, r.bottom())
        };
        // SAFETY: as above.
        unsafe {
            let collection = self.target.CreateGradientStopCollection(
                stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?;
            let brush = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2::new(r.x, r.y),
                    endPoint: end,
                },
                None,
                &collection,
            )?;
            Ok(brush.into())
        }
    }

    fn placeholder_brush(&self, id: Uuid, r: Rect) -> Result<ID2D1Brush> {
        // Same recipe as the UI's placeholder covers (theme.css: saturation 38 %, lightness
        // 34 % -> 18 %, hue shifted by 40).
        let h = hue(id);
        self.gradient(
            r,
            &[
                D2D1_GRADIENT_STOP {
                    position: 0.0,
                    color: hsl(h, 0.38, 0.34),
                },
                D2D1_GRADIENT_STOP {
                    position: 1.0,
                    color: hsl(h + 40.0, 0.38, 0.18),
                },
            ],
            true,
        )
    }

    fn draw(
        &self,
        layout: &Layout,
        data: &PopupData,
        images: &Prepared,
        bitmaps: &mut Bitmaps,
        state: &ViewState,
    ) -> Result<()> {
        let t = self.target;
        // SAFETY: as above.
        unsafe {
            t.SetTransform(&Matrix3x2::identity());
            t.Clear(Some(&rgb(color::SURFACE, 1.0)));
        }
        let muted = self.brush(color::MUTED, 1.0)?;
        let text = self.brush(color::TEXT, 1.0)?;
        let faint = self.brush(color::FAINT, 1.0)?;
        let focus = self.brush(color::ACCENT, 1.0)?;

        for (label, r) in &layout.labels {
            let r = Rect::new(r.x + 2.0, r.y + 4.0, r.w, r.h - 4.0);
            self.text(label, &self.renderer.label, r, &muted);
        }
        if let Some(r) = layout.empty_recent {
            self.text(
                layout::EMPTY_RECENT,
                &self.renderer.empty,
                Rect::new(r.x + 2.0, r.y, r.w, r.h),
                &faint,
            );
        }

        // Favorites are clipped to their scroll viewport.
        for (index, item) in layout.items.iter().enumerate() {
            let hovered = state.hover == Some(index);
            let pressed = hovered && state.pressed == Some(index);
            let focused = state.show_focus && state.focus == Some(index);
            let r = snapped(item.rect, self.scale);
            match item.kind {
                ItemKind::Tile(_) => {
                    let Some(game) = data.game(item.kind) else {
                        continue;
                    };
                    self.draw_tile(
                        index,
                        game.id,
                        &game.name,
                        r,
                        images,
                        bitmaps,
                        hovered || focused,
                        pressed,
                    )?;
                    if focused {
                        self.stroke(inflate(r, 3.0), TILE_RADIUS + 3.0, &focus, FOCUS_WIDTH);
                    } else if hovered {
                        let ring = self.brush(color::WHITE, 0.35)?;
                        self.stroke(inflate(r, 0.5), TILE_RADIUS, &ring, 1.0);
                    }
                }
                ItemKind::Row(_) => {
                    if !layout.visible(item) {
                        continue;
                    }
                    let Some(game) = data.game(item.kind) else {
                        continue;
                    };
                    let clip = layout.favorites_clip.unwrap_or(item.rect);
                    // SAFETY: as above; popped below.
                    unsafe {
                        t.PushAxisAlignedClip(
                            &d2d_rect(snapped(clip, self.scale)),
                            D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        );
                    }
                    if hovered || focused {
                        let bg =
                            self.brush(if pressed { color::BORDER } else { color::HOVER }, 1.0)?;
                        self.fill(r, ROW_RADIUS, &bg);
                    }
                    let icon = snapped(row_icon_rect(&item.rect), self.scale);
                    match self.bitmap(index, images, bitmaps)? {
                        // Exact pixel size; slightly rounded so square icons fit the
                        // rounded look (transparent icons keep their own shape).
                        Some(b) => self.fill_bitmap(icon, BITMAP_ICON_RADIUS, &b)?,
                        None => {
                            let bg = self.placeholder_brush(game.id, icon)?;
                            self.fill(icon, ICON_RADIUS, &bg);
                            let letter: String = game
                                .name
                                .chars()
                                .find(|c| c.is_alphanumeric())
                                .map(|c| c.to_uppercase().collect())
                                .unwrap_or_default();
                            let white = self.brush(color::WHITE, 0.9)?;
                            self.text(&letter, &self.renderer.letter, icon, &white);
                        }
                    }
                    let text_x = icon.right() + 12.0;
                    let name_rect = Rect::new(text_x, r.y, r.right() - text_x - 8.0, r.h);
                    self.text(&game.name, &self.renderer.row, name_rect, &text);
                    if focused {
                        self.stroke(inflate(r, -1.0), ROW_RADIUS, &focus, FOCUS_WIDTH);
                    }
                    // SAFETY: matches the push above.
                    unsafe { t.PopAxisAlignedClip() };
                }
                ItemKind::Pill(i) => {
                    let Some(client) = data.clients.get(i) else {
                        continue;
                    };
                    let bg = self.brush(
                        if pressed {
                            color::BORDER
                        } else if hovered {
                            color::HOVER
                        } else {
                            color::RAISED
                        },
                        1.0,
                    )?;
                    let radius = r.h / 2.0;
                    self.fill(r, radius, &bg);
                    let border = self.brush(color::BORDER, 1.0)?;
                    self.stroke(inflate(r, -0.5), radius, &border, 1.0);
                    self.centered_text(client.name, r, &text);
                    if focused {
                        self.stroke(inflate(r, 2.0), radius + 2.0, &focus, FOCUS_WIDTH);
                    }
                }
                ItemKind::OpenButton => {
                    let bg = self.brush(
                        if hovered {
                            color::ACCENT_HOVER
                        } else {
                            color::ACCENT
                        },
                        if pressed { 0.85 } else { 1.0 },
                    )?;
                    self.fill(r, BUTTON_RADIUS, &bg);
                    let fg = self.brush(color::ACCENT_CONTRAST, 1.0)?;
                    self.centered_text(layout::OPEN_LABEL, r, &fg);
                    if focused {
                        self.stroke(inflate(r, 2.5), BUTTON_RADIUS + 2.5, &focus, FOCUS_WIDTH);
                    }
                }
                ItemKind::UpdateButton => {
                    let Some(version) = data.update.as_deref() else {
                        continue;
                    };
                    let bg = self.brush(
                        if pressed {
                            color::BORDER
                        } else if hovered {
                            color::HOVER
                        } else {
                            color::RAISED
                        },
                        1.0,
                    )?;
                    self.fill(r, BUTTON_RADIUS, &bg);
                    let accent = self.brush(color::ACCENT, 1.0)?;
                    self.stroke(inflate(r, -0.5), BUTTON_RADIUS, &accent, 1.0);
                    self.centered_text(&layout::update_label(version), r, &accent);
                    if focused {
                        self.stroke(inflate(r, 2.5), BUTTON_RADIUS + 2.5, &focus, FOCUS_WIDTH);
                    }
                }
                ItemKind::ExitButton => {
                    if hovered || focused {
                        let bg =
                            self.brush(if pressed { color::BORDER } else { color::HOVER }, 1.0)?;
                        self.fill(r, BUTTON_RADIUS, &bg);
                    }
                    self.centered_text(layout::EXIT_LABEL, r, if hovered { &text } else { &muted });
                    if focused {
                        self.stroke(inflate(r, 2.5), BUTTON_RADIUS + 2.5, &focus, FOCUS_WIDTH);
                    }
                }
            }
        }

        // Scroll indicator for long favorites lists.
        if let (Some(clip), true) = (layout.favorites_clip, layout.max_scroll > 0.0) {
            let total = clip.h + layout.max_scroll;
            let scrolled = layout
                .items
                .iter()
                .find(|i| i.kind == ItemKind::Row(0))
                .map_or(0.0, |i| clip.y - i.rect.y);
            let thumb_h = (clip.h * clip.h / total).max(24.0);
            let thumb_y = clip.y + (clip.h - thumb_h) * (scrolled / layout.max_scroll);
            let thumb = self.brush(color::FAINT, 0.6)?;
            self.fill(
                Rect::new(clip.right() + 4.0, thumb_y, 3.0, thumb_h),
                1.5,
                &thumb,
            );
        }

        let line = self.brush(color::BORDER, 1.0)?;
        let y = snap(layout.separator_y, self.scale) + 0.5 / self.scale;
        // SAFETY: as above.
        unsafe {
            t.DrawLine(
                Vector2::new(0.0, y),
                Vector2::new(layout.width, y),
                &line,
                1.0 / self.scale,
                None,
            );
        }
        Ok(())
    }

    fn centered_text(&self, label: &str, r: Rect, brush: &ID2D1Brush) {
        let w = self.renderer.measure(label);
        let x = r.x + (r.w - w) / 2.0;
        self.text(
            label,
            &self.renderer.button,
            Rect::new(x, r.y, w + 1.0, r.h),
            brush,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tile(
        &self,
        index: usize,
        id: Uuid,
        name: &str,
        r: Rect,
        images: &Prepared,
        bitmaps: &mut Bitmaps,
        highlighted: bool,
        pressed: bool,
    ) -> Result<()> {
        let white = self.brush(color::WHITE, 1.0)?;
        match self.bitmap(index, images, bitmaps)? {
            Some(bitmap) => {
                let bg = self.brush(color::COVER_BG, 1.0)?;
                self.fill(r, TILE_RADIUS, &bg);
                self.fill_bitmap(r, TILE_RADIUS, &bitmap)?;
                if highlighted {
                    // Darkened bottom with the name, like the hover state of the library grid.
                    let shade = self.gradient(
                        r,
                        &[
                            D2D1_GRADIENT_STOP {
                                position: 0.45,
                                color: rgb(0x000000, 0.0),
                            },
                            D2D1_GRADIENT_STOP {
                                position: 1.0,
                                color: rgb(0x000000, 0.88),
                            },
                        ],
                        false,
                    )?;
                    self.fill(r, TILE_RADIUS, &shade);
                    let text_rect =
                        Rect::new(r.x + 8.0, r.y + r.h * 0.5, r.w - 16.0, r.h * 0.5 - 8.0);
                    self.text(name, &self.renderer.tile_name, text_rect, &white);
                }
            }
            None => {
                let bg = self.placeholder_brush(id, r)?;
                self.fill(r, TILE_RADIUS, &bg);
                let text_rect = Rect::new(r.x + 8.0, r.y + 8.0, r.w - 16.0, r.h - 16.0);
                self.text(name, &self.renderer.placeholder, text_rect, &white);
            }
        }
        if pressed {
            let dim = self.brush(0x000000, 0.25)?;
            self.fill(r, TILE_RADIUS, &dim);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use questkeep_core::platform::StoreClient;
    use questkeep_core::tray::{TrayGame, TrayGames};
    use windows::Win32::Graphics::Direct2D::{
        D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES,
        D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    };
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
        WICBitmapCacheOnLoad,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    };

    use crate::tray_popup::layout::layout;
    use crate::tray_popup::prepare;

    fn game(n: u128, name: &str, cover: Option<PathBuf>, icon: Option<PathBuf>) -> TrayGame {
        TrayGame {
            id: Uuid::from_u128(n),
            name: name.into(),
            icon,
            cover,
        }
    }

    /// Writes test images (a gradient "cover" and a round "icon") into `dir`.
    fn sample_images(dir: &std::path::Path) -> (PathBuf, PathBuf) {
        let cover = dir.join("cover.png");
        image::RgbaImage::from_fn(600, 900, |x, y| {
            image::Rgba([(x * 255 / 600) as u8, (y * 255 / 900) as u8, 160, 255])
        })
        .save(&cover)
        .unwrap();
        let icon = dir.join("icon.png");
        image::RgbaImage::from_fn(32, 32, |x, y| {
            let (dx, dy) = (x as i32 - 16, y as i32 - 16);
            if dx * dx + dy * dy < 200 {
                image::Rgba([240, 90, 60, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        })
        .save(&icon)
        .unwrap();
        (cover, icon)
    }

    /// Games named after their id from an image cache folder: those with an icon become
    /// favorites, those with a cover recent tiles.
    fn real_data(images: &std::path::Path) -> PopupData {
        let ids = |sub: &str| -> Vec<(String, PathBuf)> {
            let mut files: Vec<(String, PathBuf)> = std::fs::read_dir(images.join(sub))
                .map(|d| {
                    d.flatten()
                        .map(|e| e.path())
                        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
                        .collect()
                })
                .unwrap_or_default();
            files.sort();
            files
        };
        let covers = ids("covers");
        let icons = ids("icons");
        let entry = |n: usize, id: &str, cover: Option<PathBuf>, icon: Option<PathBuf>| {
            game(
                n as u128 + 1,
                &format!("Game {}", &id[..8.min(id.len())]),
                cover,
                icon,
            )
        };
        PopupData {
            games: TrayGames {
                recent: covers
                    .iter()
                    .take(8)
                    .enumerate()
                    .map(|(n, (id, p))| entry(n, id, Some(p.clone()), None))
                    .collect(),
                favorites: icons
                    .iter()
                    .take(10)
                    .enumerate()
                    .map(|(n, (id, p))| entry(100 + n, id, None, Some(p.clone())))
                    .collect(),
            },
            clients: vec![StoreClient {
                id: "steam",
                name: "Steam",
                uri: "steam://open/main",
            }],
            update: None,
        }
    }

    /// Renders the popup into a bitmap. Set `QUESTKEEP_POPUP_PREVIEW` to a file path to also save
    /// it as PNG (a way to look at the popup without opening a window).
    #[test]
    fn renders_offscreen() {
        // SAFETY: COM setup and WIC/D2D calls on owned objects; buffers are sized to the bitmap.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let dir = tempfile::tempdir().unwrap();
            let (cover, icon) = sample_images(dir.path());
            let data = PopupData {
                games: TrayGames {
                    recent: vec![
                        game(
                            1,
                            "The Binding of Isaac: Rebirth",
                            Some(cover.clone()),
                            None,
                        ),
                        game(2, "A game without a cover", None, None),
                        game(3, "Hades", Some(cover.clone()), None),
                    ],
                    favorites: (0..8)
                        .map(|i| {
                            game(
                                10 + i,
                                &format!("Favorite game {i}"),
                                None,
                                (i % 2 == 0).then(|| icon.clone()),
                            )
                        })
                        .collect(),
                },
                clients: vec![
                    StoreClient {
                        id: "steam",
                        name: "Steam",
                        uri: "steam://open/main",
                    },
                    StoreClient {
                        id: "gog",
                        name: "GOG Galaxy",
                        uri: "goggalaxy://",
                    },
                ],
                update: Some("0.3.0".into()),
            };
            // QUESTKEEP_POPUP_IMAGES=<data dir>/images previews real covers and icons instead.
            let real = std::env::var_os("QUESTKEEP_POPUP_IMAGES")
                .map(|d| real_data(std::path::Path::new(&d)));
            let checks = real.is_none();
            let data = real.unwrap_or(data);
            let scale: f32 = std::env::var("QUESTKEEP_POPUP_SCALE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.5);
            let renderer = Renderer::new().unwrap();
            let l = layout(&data.content(), 0.0, |t| renderer.measure(t));
            let images = prepare(&data, &l, scale);
            if checks {
                assert!(images.get(0).is_some(), "cover prepared");
                assert!(images.get(3).is_some(), "icon prepared");
            }
            let (w, h) = (
                (l.width * scale).round() as u32,
                (l.height * scale).round() as u32,
            );

            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();
            let bitmap = wic
                .CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)
                .unwrap();
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = renderer
                .factory()
                .CreateWicBitmapRenderTarget(&bitmap, &props)
                .unwrap();
            let state = ViewState {
                hover: Some(0),
                focus: Some(4),
                show_focus: true,
                pressed: None,
            };
            target.BeginDraw();
            let frame = Frame {
                layout: &l,
                data: &data,
                images: &images,
                state: &state,
                scale,
            };
            renderer
                .draw(&target, &frame, &mut Bitmaps::default())
                .unwrap();
            target.EndDraw(None, None).unwrap();

            let mut pixels = vec![0u8; (w * h * 4) as usize];
            bitmap
                .CopyPixels(std::ptr::null(), w * 4, &mut pixels)
                .unwrap();
            // Background is the surface color; the first tile shows the cover's red/green ramp.
            let at = |x: u32, y: u32| {
                let i = ((y * w + x) * 4) as usize;
                [pixels[i + 2], pixels[i + 1], pixels[i]]
            };
            if checks {
                assert_eq!(at(2, (l.height * scale) as u32 / 2), [0x17, 0x1a, 0x20]);
                let tile = l.items[0].rect;
                let px = at(
                    ((tile.x + 20.0) * scale) as u32,
                    ((tile.y + 20.0) * scale) as u32,
                );
                assert_ne!(px, [0x17, 0x1a, 0x20], "cover drawn");
            }
            if let Ok(out) = std::env::var("QUESTKEEP_POPUP_PREVIEW") {
                for p in pixels.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
                image::RgbaImage::from_raw(w, h, pixels)
                    .unwrap()
                    .save(out)
                    .unwrap();
            }
        }
    }
}
