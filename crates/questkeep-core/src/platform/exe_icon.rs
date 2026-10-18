//! Extracts the icon of a Windows executable as RGBA pixels (Win32: `PrivateExtractIconsW`, then
//! `GetIconInfo` + `GetDIBits`). No window, no shell UI and no clipboard is involved.
//!
//! One of the few modules with `unsafe` code (see docs/DECISIONS.md). Every GDI/USER handle it creates is released by a
//! guard, also on early returns.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW,
};

use crate::images::IconRgba;

/// Largest edge length accepted from the system (guards the allocation below).
const MAX_SIDE: i32 = 512;

/// An icon handle destroyed on drop.
struct Icon(HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle came from PrivateExtractIconsW and is destroyed exactly once.
            unsafe { DestroyIcon(self.0) };
        }
    }
}

/// A GDI object (bitmap) deleted on drop.
struct Gdi(HANDLE);

impl Drop for Gdi {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the bitmap came from GetIconInfo and is deleted exactly once.
            unsafe { DeleteObject(self.0) };
        }
    }
}

/// A memory device context deleted on drop.
struct Dc(HANDLE);

impl Drop for Dc {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the DC came from CreateCompatibleDC and is deleted exactly once.
            unsafe { DeleteDC(self.0) };
        }
    }
}

/// The icon of `path` at about `size` px (the system picks the closest image of the first icon
/// group and scales it). `None` when the file has no icon or anything goes wrong.
pub fn executable_icon(path: &Path, size: u32) -> Option<IconRgba> {
    let size = i32::try_from(size).ok()?.clamp(16, MAX_SIDE);
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut handle: HICON = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; one handle slot is provided for
    // the one icon requested, and the icon id output may be null.
    let extracted = unsafe {
        PrivateExtractIconsW(
            wide.as_ptr(),
            0,
            size,
            size,
            &mut handle,
            std::ptr::null_mut(),
            1,
            0,
        )
    };
    let icon = Icon(handle);
    if extracted == 0 || extracted == u32::MAX || icon.0.is_null() {
        return None;
    }
    read_pixels(&icon)
}

fn read_pixels(icon: &Icon) -> Option<IconRgba> {
    let mut info = ICONINFO {
        fIcon: 0,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: std::ptr::null_mut(),
        hbmColor: std::ptr::null_mut(),
    };
    // SAFETY: `info` is a valid out structure; the two bitmaps it receives are owned by us.
    if unsafe { GetIconInfo(icon.0, &mut info) } == 0 {
        return None;
    }
    let mask = Gdi(info.hbmMask);
    let color = Gdi(info.hbmColor);
    if color.0.is_null() {
        return None; // monochrome icon: no usable color bitmap
    }

    let mut bitmap = BITMAP {
        bmType: 0,
        bmWidth: 0,
        bmHeight: 0,
        bmWidthBytes: 0,
        bmPlanes: 0,
        bmBitsPixel: 0,
        bmBits: std::ptr::null_mut(),
    };
    // SAFETY: `bitmap` is a BITMAP and the size passed is its size.
    let got = unsafe {
        GetObjectW(
            color.0,
            std::mem::size_of::<BITMAP>() as i32,
            (&mut bitmap as *mut BITMAP).cast::<c_void>(),
        )
    };
    let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
    if got == 0 || width <= 0 || height <= 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }

    // SAFETY: a memory DC compatible with the screen; released by the guard.
    let dc = Dc(unsafe { CreateCompatibleDC(std::ptr::null_mut()) });
    if dc.0.is_null() {
        return None;
    }
    let mut bgra = dib_pixels(&dc, &color, width, height)?;

    // Icons without an alpha channel (older formats, 24-bit images) report alpha 0 everywhere;
    // their transparency is in the AND mask: a set bit means transparent.
    if bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        let mask_bgra = dib_pixels(&dc, &mask, width, height);
        for (i, pixel) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let transparent = mask_bgra.as_ref().is_some_and(|m| m[i * 4] != 0);
            pixel[3] = if transparent { 0 } else { 255 };
        }
    }

    let mut rgba = bgra;
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2); // BGRA -> RGBA
    }
    Some(IconRgba {
        width: width.unsigned_abs(),
        height: height.unsigned_abs(),
        rgba,
    })
}

/// The bitmap as top-down 32-bit BGRA bytes. For the (1 bit) mask bitmap every pixel comes out
/// as black (opaque) or white (transparent).
fn dib_pixels(dc: &Dc, bitmap: &Gdi, width: i32, height: i32) -> Option<Vec<u8>> {
    if bitmap.0.is_null() {
        return None;
    }
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // negative: top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    // SAFETY: `pixels` holds width*height 32-bit pixels, exactly what the header asks for; the
    // bitmap is not selected into any DC.
    let lines = unsafe {
        GetDIBits(
            dc.0,
            bitmap.0,
            0,
            height as u32,
            pixels.as_mut_ptr().cast::<c_void>(),
            &mut header,
            DIB_RGB_COLORS,
        )
    };
    (lines == height).then_some(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_notepad_icon() {
        let windir = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let notepad = Path::new(&windir).join("System32").join("notepad.exe");
        if !notepad.is_file() {
            return; // stripped-down systems
        }
        let icon = executable_icon(&notepad, 48).expect("notepad has an icon");
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert!(icon.width >= 32 && icon.height >= 32);
        assert!(
            icon.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
            "the icon is not fully transparent"
        );
        let png = crate::images::icon_png_from_rgba(&icon, crate::images::SquareFrom::Icon)
            .expect("converts to a 32 px icon");
        let small = crate::images::icon_png_from_bytes(&png, crate::images::SquareFrom::Icon);
        assert!(small.is_some());
    }

    #[test]
    fn missing_files_and_files_without_icons_give_none() {
        assert!(executable_icon(Path::new(r"C:\definitely\not\here.exe"), 48).is_none());
        let tmp = tempfile::tempdir().unwrap();
        let text = tmp.path().join("plain.exe");
        std::fs::write(&text, b"MZ not really").unwrap();
        assert!(executable_icon(&text, 48).is_none());
    }
}
