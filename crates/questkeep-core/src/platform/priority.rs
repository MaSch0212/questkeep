//! Background thread priority for long jobs (backups) that must not slow down a running game.
//! One of the few modules with `unsafe` code (see docs/DECISIONS.md): two Win32 calls on the
//! current thread's pseudo handle, which needs no closing.
#![allow(unsafe_code)]

use windows_sys::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN, THREAD_MODE_BACKGROUND_END,
};

/// Lowers the current thread's CPU and I/O priority until dropped.
pub struct BackgroundPriority {
    active: bool,
}

impl BackgroundPriority {
    pub fn begin() -> Self {
        // SAFETY: GetCurrentThread returns a pseudo handle valid for this thread.
        let active =
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) } != 0;
        if !active {
            tracing::debug!("could not enter background priority");
        }
        Self { active }
    }
}

impl Drop for BackgroundPriority {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: as in `begin`; ends the mode this thread entered.
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_END) };
        }
    }
}
