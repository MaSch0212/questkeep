//! What the window showed before an update restart: the UI hands over its open page, view and
//! game as an opaque JSON string, the shell writes it next to the data before the installer runs,
//! and the next start opens the window (never minimized to the tray) and gives the string back
//! once. A file older than [`MAX_AGE`] is ignored: the update did not restart the app.

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::error::Result;

/// How long after the update started the restored start may come.
pub const MAX_AGE: Duration = Duration::from_secs(15 * 60);

/// Remembers the window's state for the start after the update.
pub fn save(path: &Path, state: &str) -> Result<()> {
    std::fs::write(path, state)?;
    Ok(())
}

/// Drops a saved state (the update failed before restarting).
pub fn discard(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Whether a recent saved state waits: the start follows an update.
pub fn pending(path: &Path) -> bool {
    pending_at(path, SystemTime::now())
}

fn pending_at(path: &Path, now: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() <= MAX_AGE)
}

/// The saved state, once: the file is removed. `None` without a recent one.
pub fn take(path: &Path) -> Option<String> {
    let fresh = pending(path);
    let state = std::fs::read_to_string(path).ok();
    discard(path);
    state.filter(|s| fresh && !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_state_is_given_back_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("resume.json");
        assert!(!pending(&file));
        assert_eq!(take(&file), None);
        save(&file, r#"{"page":"library"}"#).unwrap();
        assert!(pending(&file));
        assert_eq!(take(&file).as_deref(), Some(r#"{"page":"library"}"#));
        assert!(!pending(&file));
        assert_eq!(take(&file), None);
    }

    #[test]
    fn old_state_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("resume.json");
        save(&file, "{}").unwrap();
        let later = SystemTime::now() + MAX_AGE + Duration::from_secs(60);
        assert!(!pending_at(&file, later));
    }
}
