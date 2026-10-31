//! Cheap process listing for noticing games started outside Questkeep. Listing returns only ids
//! (`EnumProcesses` on Windows: microseconds, where a ToolHelp snapshot of every process and thread
//! takes milliseconds); a process is opened and its paths read once, when it first shows up.
//!
//! On Windows the opened handle is kept: an id can't be reused while a handle to its process is
//! open, so a known id always means the same process, and an exited process is noticed through
//! its handle (exited processes still referenced by handles may stay listed).
//!
//! One of the few modules with `unsafe` code (see docs/DECISIONS.md): plain Win32 calls with local
//! buffers and handles owned by [`ProcessRef`].

#![cfg_attr(windows, allow(unsafe_code))]

/// A process opened by [`open_process`]; closes its handle when dropped.
#[derive(Debug)]
pub struct ProcessRef {
    /// The process handle as an integer (raw handles are not `Send`); 0 when not opened.
    #[cfg(windows)]
    handle: isize,
}

impl ProcessRef {
    /// A reference that holds nothing (processes that can't be opened, tests).
    pub fn none() -> Self {
        Self {
            #[cfg(windows)]
            handle: 0,
        }
    }

    /// Whether the process is known to have exited.
    pub fn has_exited(&self) -> bool {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
            use windows_sys::Win32::System::Threading::WaitForSingleObject;
            if self.handle == 0 {
                return false;
            }
            // SAFETY: the handle is owned by `self` and open until drop.
            unsafe { WaitForSingleObject(self.handle as _, 0) == WAIT_OBJECT_0 }
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessRef {
    fn drop(&mut self) {
        if self.handle != 0 {
            // SAFETY: the handle is owned by `self` and closed exactly once.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
            }
        }
    }
}

/// Replaces `out` with the ids of the running processes.
#[cfg(windows)]
pub fn list_processes(out: &mut Vec<u32>) {
    use windows_sys::Win32::System::ProcessStatus::EnumProcesses;
    if out.capacity() < 1024 {
        out.reserve(1024 - out.len());
    }
    loop {
        out.resize(out.capacity(), 0);
        let size = u32::try_from(out.len() * size_of::<u32>()).unwrap_or(u32::MAX);
        let mut written = 0u32;
        // SAFETY: the buffer is a local vector whose byte size is passed along.
        let ok = unsafe { EnumProcesses(out.as_mut_ptr(), size, &mut written) } != 0;
        if !ok {
            out.clear();
            return;
        }
        let count = written as usize / size_of::<u32>();
        // A full buffer may have cut the list short: grow and ask again.
        if count < out.len() {
            out.truncate(count);
            return;
        }
        out.reserve(out.len());
    }
}

/// Opens a process and returns the paths identifying it (its executable) and the reference that
/// keeps its id. No paths when the process is gone or protected.
#[cfg(windows)]
pub fn open_process(pid: u32) -> (Vec<String>, ProcessRef) {
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        QueryFullProcessImageNameW,
    };
    // SAFETY: the buffer is local and its length is passed along; the handle is owned by the
    // returned `ProcessRef`, which closes it.
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        );
        if handle.is_null() {
            return (Vec::new(), ProcessRef::none());
        }
        let process = ProcessRef {
            handle: handle as isize,
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) == 0 {
            return (Vec::new(), process);
        }
        let len = (len as usize).min(buf.len());
        (vec![String::from_utf16_lossy(&buf[..len])], process)
    }
}

/// Replaces `out` with the ids of the running processes.
#[cfg(target_os = "linux")]
pub fn list_processes(out: &mut Vec<u32>) {
    out.clear();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in dir.flatten() {
        if let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse().ok()) {
            out.push(pid);
        }
    }
}

/// Paths identifying a process: its executable, working directory and path-like arguments
/// (Proton games run as `wine64-preloader` and only those point into the game folder). Linux
/// hands out ids in increasing order, so no reference is needed to keep one.
#[cfg(target_os = "linux")]
pub fn open_process(pid: u32) -> (Vec<String>, ProcessRef) {
    let base = format!("/proc/{pid}");
    let link = |name: &str| {
        std::fs::read_link(format!("{base}/{name}"))
            .ok()
            .map(|p| p.display().to_string())
    };
    let mut paths: Vec<String> = link("exe").into_iter().collect();
    let cwd = link("cwd");
    let args = std::fs::read(format!("{base}/cmdline")).unwrap_or_default();
    let args = args
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned());
    paths.extend(crate::launcher::path_hints(cwd, args));
    (paths, ProcessRef::none())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn list_processes(out: &mut Vec<u32>) {
    out.clear();
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn open_process(_pid: u32) -> (Vec<String>, ProcessRef) {
    (Vec::new(), ProcessRef::none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn lists_and_opens_the_current_process() {
        let me = std::process::id();
        let mut pids = Vec::new();
        list_processes(&mut pids);
        assert!(pids.contains(&me));
        let exe = std::env::current_exe().unwrap();
        let (paths, process) = open_process(me);
        let name = exe.file_name().unwrap().to_string_lossy().to_string();
        assert!(paths.iter().any(|p| p.ends_with(&name)), "{paths:?}");
        assert!(!process.has_exited());
    }

    #[cfg(windows)]
    #[test]
    fn notices_an_exited_process_through_its_handle() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/C", "exit"])
            .spawn()
            .unwrap();
        let (_, process) = open_process(child.id());
        child.wait().unwrap();
        assert!(process.has_exited());
    }
}
