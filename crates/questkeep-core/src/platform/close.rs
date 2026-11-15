//! Closing other programs: asking their windows to close, and ending them by force (closing a
//! store's launcher after playing, stopping scripts that ran too long).
//!
//! One of the few modules with `unsafe` code (see docs/DECISIONS.md): plain Win32 calls.

#![cfg_attr(windows, allow(unsafe_code))]

/// Asks the top-level windows of these processes to close (`WM_CLOSE`); on Linux sends SIGTERM.
pub fn close_windows(pids: &[u32]) {
    if pids.is_empty() {
        return;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
        };

        unsafe extern "system" fn each(window: HWND, data: LPARAM) -> i32 {
            // SAFETY: `data` is the address of the slice passed to `EnumWindows` below, which
            // outlives the enumeration.
            let pids = unsafe { &*(data as *const Vec<u32>) };
            let mut pid = 0u32;
            // SAFETY: a valid window handle from the enumeration and a local out parameter.
            unsafe { GetWindowThreadProcessId(window, &mut pid) };
            if pids.contains(&pid) {
                // SAFETY: posting a message to a window handle is always allowed.
                unsafe { PostMessageW(window, WM_CLOSE, 0 as WPARAM, 0) };
            }
            1
        }

        let pids = pids.to_vec();
        // SAFETY: the callback only reads `pids`, which lives until `EnumWindows` returns.
        unsafe { EnumWindows(Some(each), &pids as *const Vec<u32> as LPARAM) };
    }
    #[cfg(not(windows))]
    signal(pids, "-TERM");
}

/// Ends these processes right away.
pub fn terminate(pids: &[u32]) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_TERMINATE, TerminateProcess,
        };
        for &pid in pids {
            // SAFETY: opening a process by id; the handle is checked and closed below.
            let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
            if handle.is_null() {
                tracing::debug!(pid, "could not open a process to end it");
                continue;
            }
            // SAFETY: a valid handle opened with PROCESS_TERMINATE, closed exactly once.
            unsafe {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
    #[cfg(not(windows))]
    signal(pids, "-KILL");
}

#[cfg(not(windows))]
fn signal(pids: &[u32], which: &str) {
    if pids.is_empty() {
        return;
    }
    let result = std::process::Command::new("kill")
        .arg(which)
        .args(pids.iter().map(u32::to_string))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if let Err(err) = result {
        tracing::debug!(%err, "could not signal processes");
    }
}
