//! Game icons for the tray menu: every source (store icon, executable icon, cover) is turned into
//! one small square PNG so the menu build only has to decode a few hundred bytes per game.
//!
//! Icons are stored as `images/icons/<game id>.png`, always [`ICON_SIZE`] x [`ICON_SIZE`] RGBA.
//! 32 px stays sharp at 150-200 % display scaling, where menu icons are drawn at 16-20 logical px.

use std::io::Cursor;
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{ImageFormat, ImageReader, Limits, RgbaImage};

/// Edge length of a stored icon in pixels.
pub const ICON_SIZE: u32 = 32;
/// Sources smaller than this are not icons (usually broken or placeholder images).
const MIN_SOURCE_SIDE: u32 = 8;
/// Provider icons must be (almost) square; wide logos and box art are rejected. Allowed ratio of
/// the longer to the shorter side.
const MAX_ICON_ASPECT: f32 = 1.15;
/// Decoding limit, so a crafted image cannot allocate gigabytes.
const MAX_DECODED_SIDE: u32 = 8192;
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
/// Stored icons are tiny; anything bigger is not ours.
const MAX_STORED_ICON_BYTES: u64 = 256 * 1024;

/// How a non-square source is reduced to a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquareFrom {
    /// A store or application icon: it must already be (nearly) square, so a wide logo is
    /// rejected instead of being cropped into nonsense. The excess is cut off centered.
    Icon,
    /// A portrait cover: the top square (the key art), not the middle (often just a title).
    CoverTop,
}

/// Decoded RGBA pixels (row-major, straight alpha).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decodes PNG/JPEG/ICO bytes, makes the image square as `mode` says, scales it to
/// [`ICON_SIZE`] and returns the PNG bytes. `None` for invalid, unsupported, too small, wrongly
/// shaped or fully transparent images.
pub fn icon_png_from_bytes(bytes: &[u8], mode: SquareFrom) -> Option<Vec<u8>> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format()?,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Ico
    ) {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let decoded = reader.decode().ok()?.into_rgba8();
    icon_png_from_image(&decoded, mode)
}

/// Like [`icon_png_from_bytes`] for pixels that are already decoded (executable icons).
pub fn icon_png_from_rgba(icon: &IconRgba, mode: SquareFrom) -> Option<Vec<u8>> {
    let image = RgbaImage::from_raw(icon.width, icon.height, icon.rgba.clone())?;
    icon_png_from_image(&image, mode)
}

fn icon_png_from_image(source: &RgbaImage, mode: SquareFrom) -> Option<Vec<u8>> {
    let (w, h) = source.dimensions();
    let (short, long) = (w.min(h), w.max(h));
    if short < MIN_SOURCE_SIDE {
        return None;
    }
    if mode == SquareFrom::Icon && long as f32 / short as f32 > MAX_ICON_ASPECT {
        return None;
    }
    let x = (w - short) / 2;
    let y = match mode {
        SquareFrom::Icon => (h - short) / 2,
        SquareFrom::CoverTop => 0,
    };
    let square = imageops::crop_imm(source, x, y, short, short).to_image();
    let small = resize_straight_alpha(&square, ICON_SIZE);
    if small.pixels().all(|p| p.0[3] == 0) {
        return None;
    }
    let mut out = Cursor::new(Vec::new());
    small.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// Lanczos resize that does not bleed the color of transparent pixels into the edges: the
/// resampling runs on premultiplied alpha and the result is converted back.
fn resize_straight_alpha(source: &RgbaImage, side: u32) -> RgbaImage {
    let mut premultiplied = source.clone();
    for p in premultiplied.pixels_mut() {
        let a = u32::from(p.0[3]);
        for c in &mut p.0[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
    let mut out = imageops::resize(&premultiplied, side, side, FilterType::Lanczos3);
    for p in out.pixels_mut() {
        let a = u32::from(p.0[3]);
        if a == 0 {
            p.0 = [0, 0, 0, 0];
        } else if a < 255 {
            for c in &mut p.0[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    out
}

/// Reads a stored icon (a small PNG from the icon cache) for the tray menu. `None` when the file
/// is missing, too large or not a PNG.
pub fn load_icon_rgba(path: &Path) -> Option<IconRgba> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_STORED_ICON_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::Png).ok()?;
    let rgba = decoded.into_rgba8();
    Some(IconRgba {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::*;

    fn encode(image: &RgbaImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        if format == ImageFormat::Jpeg {
            image::DynamicImage::ImageRgba8(image.clone())
                .to_rgb8()
                .write_to(&mut out, format)
                .unwrap();
        } else {
            image.write_to(&mut out, format).unwrap();
        }
        out.into_inner()
    }

    fn decode(png: &[u8]) -> RgbaImage {
        image::load_from_memory_with_format(png, ImageFormat::Png)
            .unwrap()
            .into_rgba8()
    }

    /// Red top half, blue bottom half.
    fn two_tone(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |_, y| {
            if y < h / 2 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        })
    }

    #[test]
    fn square_png_roundtrips_to_32_pixels() {
        let png = encode(&two_tone(256, 256), ImageFormat::Png);
        let icon = icon_png_from_bytes(&png, SquareFrom::Icon).unwrap();
        let out = decode(&icon);
        assert_eq!(out.dimensions(), (ICON_SIZE, ICON_SIZE));
        assert_eq!(out.get_pixel(16, 2).0, [255, 0, 0, 255]);
        assert_eq!(out.get_pixel(16, 29).0, [0, 0, 255, 255]);
    }

    #[test]
    fn small_and_jpeg_sources_work() {
        let jpg = encode(&two_tone(48, 48), ImageFormat::Jpeg);
        let out = decode(&icon_png_from_bytes(&jpg, SquareFrom::Icon).unwrap());
        assert_eq!(out.dimensions(), (ICON_SIZE, ICON_SIZE));
        let tiny = encode(&two_tone(16, 16), ImageFormat::Png);
        assert!(icon_png_from_bytes(&tiny, SquareFrom::Icon).is_some());
    }

    #[test]
    fn cover_is_cropped_from_the_top() {
        // 2:3 cover: red for the top third (the top square covers rows 0..200 of 300).
        let cover = RgbaImage::from_fn(200, 300, |_, y| {
            if y < 100 {
                Rgba([255, 0, 0, 255])
            } else if y < 200 {
                Rgba([0, 255, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let png = encode(&cover, ImageFormat::Png);
        let out = decode(&icon_png_from_bytes(&png, SquareFrom::CoverTop).unwrap());
        assert_eq!(out.dimensions(), (ICON_SIZE, ICON_SIZE));
        assert_eq!(out.get_pixel(16, 2).0, [255, 0, 0, 255]);
        assert_eq!(
            out.get_pixel(16, 29).0,
            [0, 255, 0, 255],
            "blue part is cropped away"
        );
    }

    #[test]
    fn provider_icons_must_be_square() {
        let wide = encode(&two_tone(300, 100), ImageFormat::Png);
        assert!(icon_png_from_bytes(&wide, SquareFrom::Icon).is_none());
        // The same image is fine as a cover stand-in.
        assert!(icon_png_from_bytes(&wide, SquareFrom::CoverTop).is_some());
        let nearly = encode(&two_tone(100, 105), ImageFormat::Png);
        assert!(icon_png_from_bytes(&nearly, SquareFrom::Icon).is_some());
    }

    #[test]
    fn invalid_data_is_skipped() {
        assert!(icon_png_from_bytes(b"", SquareFrom::Icon).is_none());
        assert!(icon_png_from_bytes(b"<html>nope</html>", SquareFrom::Icon).is_none());
        let truncated = encode(&two_tone(64, 64), ImageFormat::Png);
        assert!(icon_png_from_bytes(&truncated[..truncated.len() / 2], SquareFrom::Icon).is_none());
        let clear = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        assert!(icon_png_from_bytes(&encode(&clear, ImageFormat::Png), SquareFrom::Icon).is_none());
        let speck = encode(&two_tone(4, 4), ImageFormat::Png);
        assert!(icon_png_from_bytes(&speck, SquareFrom::Icon).is_none());
    }

    #[test]
    fn transparency_is_kept_without_color_fringes() {
        // Opaque white disc-ish block on a transparent background with black "hidden" color.
        let image = RgbaImage::from_fn(64, 64, |x, y| {
            if (16..48).contains(&x) && (16..48).contains(&y) {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        let out = decode(
            &icon_png_from_bytes(&encode(&image, ImageFormat::Png), SquareFrom::Icon).unwrap(),
        );
        assert_eq!(out.get_pixel(0, 0).0[3], 0);
        assert_eq!(out.get_pixel(16, 16).0, [255, 255, 255, 255]);
        for p in out.pixels().filter(|p| p.0[3] > 0) {
            assert!(
                p.0[0] >= 250,
                "partially transparent pixels stay white: {:?}",
                p.0
            );
        }
    }

    #[test]
    fn rgba_input_and_stored_icon_loading() {
        let icon = IconRgba {
            width: 48,
            height: 48,
            rgba: two_tone(48, 48).into_raw(),
        };
        let png = icon_png_from_rgba(&icon, SquareFrom::Icon).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("icon.png");
        std::fs::write(&path, &png).unwrap();
        let loaded = load_icon_rgba(&path).unwrap();
        assert_eq!((loaded.width, loaded.height), (ICON_SIZE, ICON_SIZE));
        assert_eq!(loaded.rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert!(load_icon_rgba(&tmp.path().join("missing.png")).is_none());
        std::fs::write(tmp.path().join("bad.png"), b"nope").unwrap();
        assert!(load_icon_rgba(&tmp.path().join("bad.png")).is_none());
        let bad = IconRgba {
            width: 10,
            height: 10,
            rgba: vec![0; 5],
        };
        assert!(icon_png_from_rgba(&bad, SquareFrom::Icon).is_none());
    }
}
