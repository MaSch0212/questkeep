//! App updates: one check shortly after start (and whenever Settings checks), the result kept
//! here for the title bar and the tray, and installing it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::commands::CmdResult;

/// Event with the newer version (`UpdateInfo`), sent when a check finds one.
pub const AVAILABLE_EVENT: &str = "update-available";
/// Event asking the window to show the update dialog (the tray's "Update" entry).
pub const SHOW_EVENT: &str = "show-update";
/// Event with the download progress (`UpdateProgress`) while an update installs.
pub const PROGRESS_EVENT: &str = "update-progress";

/// Where a downloaded copy sends the user for a new version.
const RELEASES_URL: &str = "https://github.com/MaSch0212/questkeep/releases";

/// Wait after start before checking, so the check doesn't compete with the first scan.
const STARTUP_DELAY: Duration = Duration::from_secs(5);

/// A newer version than the running one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UpdateInfo {
    pub version: String,
    /// Release notes (markdown) from the update manifest, if any.
    pub notes: Option<String>,
    /// Publication date as RFC 3339, if the manifest has one.
    pub date: Option<String>,
    /// This copy can't install it (a downloaded exe, not an installed copy): the user downloads
    /// it from the release page.
    pub manual: bool,
}

/// The newest update found, and whether the tray asked the window to show it before the window
/// could listen (it was in the tray).
#[derive(Default)]
pub struct Updates {
    available: Mutex<Option<UpdateInfo>>,
    show_requested: AtomicBool,
    /// What the window showed when the install was asked for; kept while a backup holds it.
    resume: Mutex<Option<String>>,
}

impl Updates {
    pub fn available(&self) -> Option<UpdateInfo> {
        self.available
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether the tray asked to show the update dialog since the last call.
    pub fn take_show_request(&self) -> bool {
        self.show_requested.swap(false, Ordering::SeqCst)
    }
}

/// Checks for an update in the background a few seconds after start.
pub fn check_on_startup(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            std::thread::sleep(STARTUP_DELAY);
            // Failures are logged in `find`; there is nothing to show for a background check.
            let _ = tauri::async_runtime::block_on(check(&app));
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the update check");
    }
}

/// Asks the update endpoint for a newer release and remembers the answer.
pub async fn check(app: &AppHandle) -> CmdResult<Option<UpdateInfo>> {
    let info = if let Some(fake) = fake_update() {
        Some(fake)
    } else {
        find(app).await?.map(|update| UpdateInfo {
            version: update.version.clone(),
            notes: update.body.clone().filter(|b| !b.trim().is_empty()),
            date: update
                .date
                .and_then(|d| chrono::DateTime::from_timestamp(d.unix_timestamp(), 0))
                .map(|d| d.to_rfc3339()),
            manual: !crate::install_kind::self_updating(),
        })
    };
    remember(app, info.clone());
    Ok(info)
}

fn remember(app: &AppHandle, info: Option<UpdateInfo>) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let changed = {
        let mut available = updates
            .available
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let changed = *available != info;
        available.clone_from(&info);
        changed
    };
    if !changed {
        return;
    }
    crate::tray::set_update(info.as_ref().map(|i| i.version.clone()));
    if let Some(info) = info {
        tracing::info!(version = %info.version, "an update is available");
        if let Err(err) = app.emit(AVAILABLE_EVENT, info) {
            tracing::warn!(%err, "could not announce the update");
        }
    }
}

/// The tray's "Update" entry: opens the window and its update dialog.
pub fn show(app: &AppHandle) {
    if let Some(updates) = app.try_state::<Updates>() {
        updates.show_requested.store(true, Ordering::SeqCst);
    }
    crate::window::show_main_window_async(app);
    // A window that is already loaded hears the event; a new one asks `take_show_request`.
    let _ = app.emit(SHOW_EVENT, ());
}

/// Debug builds never see real updates; `QUESTKEEP_FAKE_UPDATE=<version>` pretends one exists, to
/// try the title bar and tray entries (installing it does nothing).
fn fake_update() -> Option<UpdateInfo> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let version = std::env::var("QUESTKEEP_FAKE_UPDATE").ok()?;
    Some(UpdateInfo {
        version,
        notes: Some("A pretend update for testing (QUESTKEEP_FAKE_UPDATE).".into()),
        date: None,
        manual: !crate::install_kind::self_updating(),
    })
}

/// Asks the update endpoint for a newer release. Always `None` in debug builds.
async fn find(app: &AppHandle) -> CmdResult<Option<tauri_plugin_updater::Update>> {
    use tauri_plugin_updater::UpdaterExt;
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let updater = app.updater().map_err(|e| {
        tracing::warn!(error = %e, "updater is not available");
        format!("Updates are not available: {e}")
    })?;
    updater.check().await.map_err(|e| {
        tracing::warn!(error = %e, "update check failed");
        format!("Could not check for updates: {e}")
    })
}

/// Downloads the newest release (reporting progress to the window), remembers what the window
/// shows for the start after it (`resume`, opaque JSON from the UI) and installs it; the app then
/// starts again. Does nothing (and returns normally) when no newer release exists. A downloaded
/// copy opens the release page instead.
pub async fn install(app: &AppHandle, resume: Option<String>) -> CmdResult<()> {
    if !crate::install_kind::self_updating() {
        return open_release_page(app);
    }
    if let Some(updates) = app.try_state::<Updates>()
        && resume.is_some()
    {
        // A deferred install (after a running backup) picks it up again.
        *updates
            .resume
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = resume;
    }
    if crate::backups::hold_quit(app, crate::backups::PendingQuit::Update) {
        return Ok(());
    }
    let Some(update) = find(app).await? else {
        return Ok(());
    };
    tracing::info!(version = %update.version, "downloading update");
    let mut progress = Progress::default();
    let bytes = update
        .download(
            |chunk, total| {
                if let Some(report) = progress.add(chunk as u64, total) {
                    let _ = app.emit(PROGRESS_EVENT, report);
                }
            },
            || {},
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "update download failed");
            format!("Could not download the update: {e}")
        })?;
    let size = bytes.len() as u64;
    let _ = app.emit(
        PROGRESS_EVENT,
        UpdateProgress {
            downloaded: size,
            total: Some(size),
            installing: true,
        },
    );
    let resume_file = app
        .try_state::<Arc<AppState>>()
        .map(|state| state.library.paths().resume_file());
    let resume = app.try_state::<Updates>().and_then(|u| {
        u.resume
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    });
    if let (Some(file), Some(resume)) = (&resume_file, &resume)
        && let Err(err) = questkeep_core::resume::save(file, resume)
    {
        tracing::warn!(%err, "could not remember the open page for after the update");
    }
    tracing::info!(version = %update.version, "installing update");
    if let Err(err) = install_bytes(app, &update, &bytes) {
        if let Some(file) = &resume_file {
            questkeep_core::resume::discard(file);
        }
        tracing::error!(%err, "update installation failed");
        return Err(format!("Could not install the update: {err}"));
    }
    Ok(())
}

/// Download progress for the update dialog (`update-progress` event).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    /// The download is done and the installer starts.
    pub installing: bool,
}

/// Sums downloaded chunks and decides when the window hears about it: once per percent (or per
/// 256 KiB when the size is unknown), so the event stream stays small.
#[derive(Default)]
struct Progress {
    downloaded: u64,
    reported: Option<u64>,
}

impl Progress {
    const STEP_UNKNOWN: u64 = 256 * 1024;

    fn add(&mut self, chunk: u64, total: Option<u64>) -> Option<UpdateProgress> {
        self.downloaded += chunk;
        let mark = match total {
            Some(total) if total > 0 => self.downloaded.saturating_mul(100) / total,
            _ => self.downloaded / Self::STEP_UNKNOWN,
        };
        if self.reported == Some(mark) {
            return None;
        }
        self.reported = Some(mark);
        Some(UpdateProgress {
            downloaded: self.downloaded,
            total,
            installing: false,
        })
    }
}

/// Windows: once this process has quit, runs the installer without a window of its own and
/// starts the app again (not minimized: the resume file makes it show its window). An MSI asks
/// for administrator rights first, as the per-machine package needs them.
#[cfg(windows)]
fn install_bytes(
    app: &AppHandle,
    update: &tauri_plugin_updater::Update,
    bytes: &[u8],
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let Some(kind) = InstallerKind::detect(bytes) else {
        // Not a bare installer (e.g. a zipped legacy artifact): the updater's own install.
        return update.install(bytes).map_err(|e| e.to_string());
    };
    let dir = std::env::temp_dir().join(format!("questkeep-update-{}", update.version));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let installer = dir.join(kind.file_name());
    std::fs::write(&installer, bytes).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = dir.join("install.ps1");
    let text = relaunch_script(std::process::id(), kind, &installer, &exe);
    std::fs::write(&script, text).map_err(|e| e.to_string())?;
    let system_root = std::env::var("SYSTEMROOT").unwrap_or_else(|_| r"C:\Windows".into());
    let powershell =
        std::path::Path::new(&system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    std::process::Command::new(powershell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .args(["-WindowStyle", "Hidden", "-File"])
        .arg(&script)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(lifecycle) = app.try_state::<crate::window::Lifecycle>() {
        lifecycle.request_quit();
    }
    crate::window::save_placement(app);
    // Runs the exit handler: fetchers close and running sessions are saved.
    app.exit(0);
    Ok(())
}

/// Other systems: the updater replaces the package, then the app restarts.
#[cfg(not(windows))]
fn install_bytes(
    app: &AppHandle,
    update: &tauri_plugin_updater::Update,
    bytes: &[u8],
) -> Result<(), String> {
    update.install(bytes).map_err(|e| e.to_string())?;
    crate::window::save_placement(app);
    app.restart()
}

/// Which installer the release ships for this copy.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallerKind {
    Nsis,
    Msi,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl InstallerKind {
    /// Recognizes a bare installer by its first bytes: an executable (NSIS) or a compound
    /// document (MSI).
    fn detect(bytes: &[u8]) -> Option<Self> {
        const MSI: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        if bytes.starts_with(b"MZ") {
            Some(Self::Nsis)
        } else if bytes.starts_with(&MSI) {
            Some(Self::Msi)
        } else {
            None
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::Nsis => "Questkeep-setup.exe",
            Self::Msi => "Questkeep.msi",
        }
    }
}

/// A PowerShell script that waits for the app (`pid`) to quit, runs the installer silently and
/// starts `exe` again, also when the install failed or was declined (the old version still works).
#[cfg_attr(not(windows), allow(dead_code))]
fn relaunch_script(
    pid: u32,
    kind: InstallerKind,
    installer: &std::path::Path,
    exe: &std::path::Path,
) -> String {
    let quote = |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
    let install = match kind {
        // `/UPDATE` keeps the shortcuts and options of the existing install.
        InstallerKind::Nsis => format!(
            "Start-Process -FilePath {} -ArgumentList '/S','/UPDATE' -Wait",
            quote(installer)
        ),
        InstallerKind::Msi => format!(
            "Start-Process -FilePath 'msiexec.exe' -ArgumentList '/i',('\"' + {} + '\"'),'/quiet','/norestart' -Verb RunAs -Wait",
            quote(installer)
        ),
    };
    format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         try {{ Wait-Process -Id {pid} -Timeout 60 }} catch {{ }}\r\n\
         try {{ {install} }} catch {{ }}\r\n\
         Start-Process -FilePath {exe}\r\n",
        exe = quote(exe)
    )
}

/// Opens the page of the newest release found, or the list of releases.
fn open_release_page(app: &AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let url = app
        .try_state::<Updates>()
        .and_then(|u| u.available())
        .map_or_else(
            || format!("{RELEASES_URL}/latest"),
            |info| format!("{RELEASES_URL}/tag/v{}", info.version),
        );
    app.opener().open_url(url, None::<&str>).map_err(|e| {
        tracing::warn!(error = %e, "could not open the release page");
        format!("Could not open the release page: {e}")
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn progress_is_reported_once_per_percent() {
        let mut progress = Progress::default();
        let total = Some(1000);
        assert_eq!(progress.add(5, total).map(|p| p.downloaded), Some(5));
        assert_eq!(progress.add(4, total), None);
        assert_eq!(progress.add(1, total).map(|p| p.downloaded), Some(10));
        let unknown = Progress::default().add(1, None);
        assert_eq!(unknown.map(|p| p.total), Some(None));
    }

    #[test]
    fn installers_are_recognized_by_their_first_bytes() {
        assert_eq!(
            InstallerKind::detect(b"MZ\x90\0"),
            Some(InstallerKind::Nsis)
        );
        let msi = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0];
        assert_eq!(InstallerKind::detect(&msi), Some(InstallerKind::Msi));
        assert_eq!(InstallerKind::detect(b"PK\x03\x04"), None);
    }

    #[test]
    fn relaunch_script_installs_silently_and_starts_the_app() {
        let exe = Path::new(r"C:\Users\O'Neil\Questkeep\questkeep.exe");
        let nsis = relaunch_script(42, InstallerKind::Nsis, Path::new(r"C:\t\setup.exe"), exe);
        assert!(nsis.contains("Wait-Process -Id 42"));
        assert!(nsis.contains("'/S','/UPDATE'"));
        assert!(
            nsis.contains(r"Start-Process -FilePath 'C:\Users\O''Neil\Questkeep\questkeep.exe'")
        );
        let msi = relaunch_script(
            1,
            InstallerKind::Msi,
            Path::new(r"C:\a b\Questkeep.msi"),
            exe,
        );
        assert!(msi.contains(r#"('"' + 'C:\a b\Questkeep.msi' + '"'),'/quiet'"#));
        assert!(msi.contains("-Verb RunAs"));
    }
}
