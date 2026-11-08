//! Whether this copy was installed (and can update itself) or is a downloaded executable that
//! the user runs from a folder of their choice (it links to the release page instead).
//!
//! Windows: the bundler stamps the executable with the package type, but the portable exe in the
//! release is the same file and carries that stamp too. So an installed copy is recognized by the
//! installer's records instead: the NSIS installer leaves `uninstall.exe` next to the exe and
//! writes the install folder to `HKCU\Software\<manufacturer>\Questkeep` (default value), the MSI
//! writes it to the `InstallDir` value of that key, and both register an uninstall entry.
//! Linux: every package that is shipped (AppImage, deb, rpm) carries its own stamp.

use std::path::Path;
use std::sync::OnceLock;

/// Whether this copy can install updates itself. Decided once per run.
pub fn self_updating() -> bool {
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    *INSTALLED.get_or_init(|| {
        let installed = detect();
        tracing::info!(installed, "install kind");
        installed
    })
}

#[cfg(windows)]
fn detect() -> bool {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return false;
    };
    if dir.join("uninstall.exe").is_file() {
        return true;
    }
    windows_impl::install_dirs()
        .iter()
        .any(|recorded| same_dir(&dir, recorded))
}

#[cfg(not(windows))]
fn detect() -> bool {
    tauri::utils::platform::bundle_type().is_some()
}

/// Whether `dir` is the folder an installer recorded. Recorded paths may carry quotes and a
/// trailing separator, and Windows paths compare without regard to case or slash direction.
#[cfg_attr(not(windows), allow(dead_code))]
fn same_dir(dir: &Path, recorded: &str) -> bool {
    let a = normalize(&dir.to_string_lossy());
    let b = normalize(recorded);
    !b.is_empty() && a == b
}

#[cfg_attr(not(windows), allow(dead_code))]
fn normalize(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

// Reads a few string values of the installer's registry keys. Each call is a plain Win32 FFI
// call with owned buffers.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_impl {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
    };

    /// The bundler's key: `Software\<manufacturer>\<product name>`.
    const APP_KEY: &str = r"Software\masch0212\Questkeep";
    /// The NSIS installer's uninstall entry (the MSI's is named by its product code).
    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Questkeep";

    /// Every install folder the installers recorded, per user and per machine.
    pub fn install_dirs() -> Vec<String> {
        let mut dirs = Vec::new();
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for (key, value) in [
                (APP_KEY, None),
                (APP_KEY, Some("InstallDir")),
                (UNINSTALL_KEY, Some("InstallLocation")),
            ] {
                if let Some(dir) = string_value(root, key, value) {
                    dirs.push(dir);
                }
            }
        }
        dirs
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn string_value(root: HKEY, key: &str, value: Option<&str>) -> Option<String> {
        let key = wide(key);
        let value = value.map(wide);
        let value_ptr = value.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
        let mut size: u32 = 0;
        // SAFETY: valid NUL-terminated strings; a null buffer asks for the size only.
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                value_ptr,
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: the buffer holds `size` bytes, as the call above asked for.
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                value_ptr,
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..len]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_recorded_folders_loosely() {
        let dir = Path::new(r"C:\Users\me\AppData\Local\Questkeep");
        assert!(same_dir(dir, r#""C:\Users\me\AppData\Local\Questkeep""#));
        assert!(same_dir(dir, r"c:\users\me\appdata\local\questkeep\"));
        assert!(same_dir(dir, "C:/Users/me/AppData/Local/Questkeep"));
        assert!(!same_dir(dir, r"C:\Users\me\Downloads"));
        assert!(!same_dir(dir, ""));
        assert!(!same_dir(dir, "\"\""));
        assert!(same_dir(
            Path::new(r"\\?\D:\Games\Questkeep"),
            r"D:\Games\Questkeep"
        ));
    }
}
