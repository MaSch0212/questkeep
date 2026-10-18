//! Tray icon colour: a monochrome glyph, white on dark taskbars and black on light ones.
//!
//! On Windows the taskbar theme is `SystemUsesLightTheme` under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`. A background thread blocks
//! in `RegNotifyChangeKeyValue` (no polling, no CPU while waiting) and swaps the icon when it
//! changes. Other platforms use the white glyph: most Linux panels are dark.

use tauri::image::Image;

const DARK_TASKBAR_ICON: &[u8] = include_bytes!("../icons/tray-dark.png");
const LIGHT_TASKBAR_ICON: &[u8] = include_bytes!("../icons/tray-light.png");

/// The glyph for a light (`true`) or dark taskbar.
pub fn icon(light_taskbar: bool) -> tauri::Result<Image<'static>> {
    Image::from_bytes(if light_taskbar {
        LIGHT_TASKBAR_ICON
    } else {
        DARK_TASKBAR_ICON
    })
}

/// Whether the taskbar currently uses the light theme.
pub fn light_taskbar() -> bool {
    #[cfg(windows)]
    {
        windows_impl::light_taskbar()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Calls `on_change` with the new value whenever the taskbar theme changes (Windows only; a
/// no-op elsewhere). Runs on its own thread for the lifetime of the process.
pub fn watch(on_change: impl Fn(bool) + Send + 'static) {
    #[cfg(windows)]
    windows_impl::watch(on_change);
    #[cfg(not(windows))]
    let _ = on_change;
}

// One of two unsafe modules (the other is core's `platform/exe_icon.rs`): winreg (used elsewhere) cannot wait for registry changes, and
// polling would wake the process while it sits in the tray. Each call is a plain Win32 FFI call
// with owned buffers; see docs/DECISIONS.md.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_impl {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
        RRF_RT_REG_DWORD, RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    const VALUE: &str = "SystemUsesLightTheme";

    pub fn light_taskbar() -> bool {
        let key = wide(KEY);
        let value = wide(VALUE);
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: valid NUL-terminated strings and a correctly sized output buffer.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        // Missing value (older Windows): dark taskbar, the default.
        status == ERROR_SUCCESS && data == 1
    }

    pub fn watch(on_change: impl Fn(bool) + Send + 'static) {
        let spawned = std::thread::Builder::new()
            .name("tray-theme".into())
            .spawn(move || {
                let key_name = wide(KEY);
                let mut key: HKEY = std::ptr::null_mut();
                // SAFETY: valid key name and output handle.
                let opened = unsafe {
                    RegOpenKeyExW(
                        HKEY_CURRENT_USER,
                        key_name.as_ptr(),
                        0,
                        KEY_NOTIFY | KEY_READ,
                        &mut key,
                    )
                };
                if opened != ERROR_SUCCESS {
                    tracing::debug!(status = opened, "taskbar theme key not available");
                    return;
                }
                let mut last = light_taskbar();
                loop {
                    // Blocks until a value under the key changes (synchronous: no event handle).
                    // SAFETY: `key` is an open handle with KEY_NOTIFY access.
                    let status = unsafe {
                        RegNotifyChangeKeyValue(
                            key,
                            0,
                            REG_NOTIFY_CHANGE_LAST_SET,
                            std::ptr::null_mut(),
                            0,
                        )
                    };
                    if status != ERROR_SUCCESS {
                        tracing::warn!(status, "stopped watching the taskbar theme");
                        break;
                    }
                    let light = light_taskbar();
                    if light != last {
                        last = light;
                        on_change(light);
                    }
                }
                // SAFETY: closing the handle opened above.
                unsafe { RegCloseKey(key) };
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not watch the taskbar theme");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_icons_decode() {
        for light in [false, true] {
            let image = icon(light).unwrap();
            assert_eq!((image.width(), image.height()), (32, 32));
        }
    }
}
