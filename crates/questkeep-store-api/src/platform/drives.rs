//! The drives of this PC, for the plugins' read-only view of the file system.
//!
//! One of the few modules with `unsafe` code (see docs/DECISIONS.md): two plain Win32 calls with
//! a local buffer.

#![allow(unsafe_code)]

use std::path::PathBuf;

/// `DRIVE_REMOVABLE` and `DRIVE_FIXED` of `GetDriveTypeW`.
const REMOVABLE: u32 = 2;
const FIXED: u32 = 3;

/// The roots (`C:\`) of the local drives: fixed disks and removable ones such as USB SSDs.
/// Network drives, optical drives and RAM disks are left out (an unreachable network drive
/// would stall every access).
pub(super) fn fixed_drives() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    // SAFETY: plain Win32 call without pointers.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| {
            let root = format!("{}:\\", (b'A' + i) as char);
            let wide: Vec<u16> = root.encode_utf16().chain([0]).collect();
            // SAFETY: `wide` is a NUL-terminated buffer that lives through the call.
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            matches!(kind, REMOVABLE | FIXED).then(|| PathBuf::from(root))
        })
        .collect()
}
