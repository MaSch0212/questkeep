//! Foreground hand-off and key presses for launcher dialogs the user asked for (Prism Launcher's
//! "Add Instance" has no command-line option, only its Ctrl+N shortcut). Keys are only sent while
//! the expected program's window is in front, so nothing reaches another program.
//!
//! One of the few modules with `unsafe` code (see docs/DECISIONS.md): plain Win32 calls with local
//! buffers.

#![allow(unsafe_code)]

/// Lets the process started next take the foreground (the user just asked for it).
pub(super) fn allow_foreground_handoff() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
    // SAFETY: plain Win32 call without pointers.
    unsafe {
        AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// The foreground window's title and the file name of its process.
fn foreground_window() -> Option<(String, String)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    // SAFETY: every buffer is local and its length is passed along; the process handle is
    // closed before returning.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let title = String::from_utf16_lossy(&title[..usize::try_from(len).ok()?]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut size = path.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut size);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&path[..size as usize]);
        let exe = path.rsplit('\\').next().unwrap_or_default().to_string();
        Some((title, exe))
    }
}

/// Presses Ctrl+`key` once a window of `exe` whose title starts with `title_prefix` is in front,
/// checking for up to `timeout`. Never sends anything to another program. True when sent.
pub(super) fn press_ctrl_key_in(
    exe: &str,
    title_prefix: &str,
    key: u16,
    timeout: std::time::Duration,
) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VK_CONTROL,
    };
    let deadline = std::time::Instant::now() + timeout;
    let in_front = || {
        foreground_window().is_some_and(|(title, name)| {
            name.eq_ignore_ascii_case(exe) && title.starts_with(title_prefix)
        })
    };
    while std::time::Instant::now() < deadline {
        if in_front() {
            // Let the window finish appearing, then check again right before typing.
            std::thread::sleep(std::time::Duration::from_millis(400));
            if !in_front() {
                continue;
            }
            let input = |vk: u16, flags: KEYBD_EVENT_FLAGS| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let inputs = [
                input(VK_CONTROL, 0),
                input(key, 0),
                input(key, KEYEVENTF_KEYUP),
                input(VK_CONTROL, KEYEVENTF_KEYUP),
            ];
            // SAFETY: `inputs` is a valid array and its exact length and element size are passed.
            let sent = unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_ptr(),
                    std::mem::size_of::<INPUT>() as i32,
                )
            };
            return sent == inputs.len() as u32;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    false
}
