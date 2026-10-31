//! Automatic backups of the library: the database, `settings.json` and the `images` folder in
//! one zip archive per backup, rotated in a folder, and restoring one of them on the next start.
//!
//! Restoring never edits the data dir in place: the archive is unpacked into a staging folder
//! first, and only a complete staging folder (marked ready) is swapped in, at the next start,
//! before anything opens the database.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::db::{Database, LATEST_VERSION};
use crate::error::{CoreError, Result};
use crate::paths::DataPaths;

/// Archive names: `questkeep-backup-<UTC time>.zip`, which sort by time.
const PREFIX: &str = "questkeep-backup-";
const EXTENSION: &str = ".zip";
/// An archive being written; renamed to `.zip` once complete.
const PARTIAL: &str = ".partial";
const MANIFEST: &str = "questkeep-backup.json";
const DB_ENTRY: &str = "library.sqlite";
const SETTINGS_ENTRY: &str = "settings.json";
const IMAGES_ENTRY: &str = "images";
/// Staging folder of a restore, inside the data dir.
const RESTORE_DIR: &str = "restore-pending";
/// Written last into the staging folder: only then is it swapped in.
const READY: &str = "READY";
/// Where the replaced files go during the swap (deleted after it).
const REPLACED_DIR: &str = "restore-replaced";
/// The settings key whose value a restore keeps from the current install.
const SETTINGS_KEY: &str = "backups";

/// How often an automatic backup runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BackupFrequency {
    Daily,
    #[default]
    Weekly,
    /// Every time Questkeep starts.
    EveryStart,
}

/// The backup settings (kept from the current install when a backup is restored).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct BackupSettings {
    pub enabled: bool,
    /// Folder for the archives; `None` is `<data dir>/backups`.
    pub folder: Option<String>,
    pub frequency: BackupFrequency,
    /// Archives kept; older ones are deleted after a new one is written.
    pub keep: u32,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            folder: None,
            frequency: BackupFrequency::default(),
            keep: 5,
        }
    }
}

impl BackupSettings {
    /// The folder archives go to.
    pub fn folder(&self, paths: &DataPaths) -> PathBuf {
        self.folder
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map_or_else(|| paths.root().join("backups"), PathBuf::from)
    }
}

/// One archive in the backup folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupInfo {
    /// Full path of the archive.
    pub path: String,
    pub created_at: DateTime<Utc>,
    #[ts(type = "number")]
    pub size: u64,
    /// The Questkeep version that wrote it (unknown for unreadable archives).
    pub app_version: Option<String>,
}

/// What an archive says about itself (`questkeep-backup.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    app_version: String,
    schema_version: u32,
    created_at: DateTime<Utc>,
}

/// Archives in `dir`, newest first. Files that aren't readable archives are left out.
pub fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut list: Vec<BackupInfo> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_archive_name(p))
        .filter_map(|path| {
            let size = fs::metadata(&path).ok()?.len();
            let manifest = read_manifest(&path).ok();
            let created_at = manifest.as_ref().map(|m| m.created_at).or_else(|| {
                fs::metadata(&path)
                    .ok()?
                    .modified()
                    .ok()
                    .map(DateTime::from)
            })?;
            Some(BackupInfo {
                path: path.to_string_lossy().into_owned(),
                created_at,
                size,
                app_version: manifest.map(|m| m.app_version),
            })
        })
        .collect();
    list.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    list
}

fn is_archive_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(EXTENSION))
}

fn read_manifest(archive: &Path) -> Result<Manifest> {
    let mut zip = open_archive(archive)?;
    let mut text = String::new();
    zip.by_name(MANIFEST)
        .map_err(|_| unreadable(archive))?
        .read_to_string(&mut text)
        .map_err(|_| unreadable(archive))?;
    serde_json::from_str(&text).map_err(|_| unreadable(archive))
}

fn open_archive(path: &Path) -> Result<ZipArchive<File>> {
    let file = File::open(path).map_err(|e| io_error("open", path, &e))?;
    ZipArchive::new(file).map_err(|_| unreadable(path))
}

fn unreadable(path: &Path) -> CoreError {
    CoreError::Invalid(format!("{} is not a Questkeep backup.", path.display()))
}

fn io_error(action: &str, path: &Path, error: &dyn std::fmt::Display) -> CoreError {
    CoreError::Invalid(format!("Could not {action} {} ({error}).", path.display()))
}

/// Whether an automatic backup is due: never made one in `dir`, or the newest is older than the
/// frequency allows.
pub fn is_due(settings: &BackupSettings, dir: &Path, now: DateTime<Utc>) -> bool {
    if !settings.enabled {
        return false;
    }
    let every = match settings.frequency {
        BackupFrequency::EveryStart => return true,
        BackupFrequency::Daily => Duration::days(1),
        BackupFrequency::Weekly => Duration::weeks(1),
    };
    list_backups(dir)
        .first()
        .is_none_or(|newest| now - newest.created_at >= every)
}

/// Deletes all but the newest `keep` archives (at least one is kept). Failures only log.
pub fn prune(dir: &Path, keep: u32) {
    let keep = usize::try_from(keep.max(1)).unwrap_or(usize::MAX);
    for old in list_backups(dir).into_iter().skip(keep) {
        if let Err(err) = fs::remove_file(&old.path) {
            tracing::warn!(path = %old.path, %err, "could not delete an old backup");
        }
    }
}

/// Every file under `dir`, relative to it, with `/` separators.
fn files_under(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(dir) {
                let name = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((path, name));
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// The backup was cancelled (quitting): the partial archive is gone.
pub fn cancelled() -> CoreError {
    CoreError::Invalid("The backup was cancelled.".into())
}

/// Writes a new archive into `dir` and prunes old ones. `progress(done, total)` is called after
/// every file; `cancel` stops the backup between files and deletes the partial archive.
pub fn create_backup(
    db: &Database,
    paths: &DataPaths,
    dir: &Path,
    keep: u32,
    cancel: &AtomicBool,
    progress: &dyn Fn(u32, u32),
) -> Result<BackupInfo> {
    fs::create_dir_all(dir).map_err(|e| io_error("create the backup folder", dir, &e))?;
    let now = Utc::now();
    let name = format!("{PREFIX}{}{EXTENSION}", now.format("%Y%m%dT%H%M%S%3fZ"));
    let target = dir.join(&name);
    let partial = dir.join(format!("{name}{PARTIAL}"));
    let snapshot = dir.join(format!("{name}{PARTIAL}.sqlite"));
    let result = write_archive(db, paths, &partial, &snapshot, now, cancel, progress);
    let _ = fs::remove_file(&snapshot);
    if let Err(err) = result {
        let _ = fs::remove_file(&partial);
        return Err(err);
    }
    fs::rename(&partial, &target).map_err(|e| io_error("finish", &target, &e))?;
    prune(dir, keep);
    let size = fs::metadata(&target).map(|m| m.len()).unwrap_or_default();
    Ok(BackupInfo {
        path: target.to_string_lossy().into_owned(),
        created_at: now,
        size,
        app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
    })
}

fn write_archive(
    db: &Database,
    paths: &DataPaths,
    partial: &Path,
    snapshot: &Path,
    now: DateTime<Utc>,
    cancel: &AtomicBool,
    progress: &dyn Fn(u32, u32),
) -> Result<()> {
    let images = files_under(&paths.images_dir());
    let total = u32::try_from(images.len() + 2).unwrap_or(u32::MAX);
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err(cancelled())
        } else {
            Ok(())
        }
    };
    let fail = |e: &dyn std::fmt::Display| io_error("write", partial, e);

    // A consistent copy of the open database, WAL included.
    db.copy_to(snapshot)?;
    check()?;
    let file = File::create(partial).map_err(|e| fail(&e))?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let packed = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    // Images are compressed already.
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(true);

    let manifest = Manifest {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: LATEST_VERSION,
        created_at: now,
    };
    zip.start_file(MANIFEST, packed).map_err(|e| fail(&e))?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)
        .map_err(|e| fail(&e))?;

    let mut add = |name: &str, source: &Path, options: SimpleFileOptions| -> Result<()> {
        let mut input = File::open(source).map_err(|e| io_error("read", source, &e))?;
        zip.start_file(name, options).map_err(|e| fail(&e))?;
        io::copy(&mut input, &mut zip).map_err(|e| fail(&e))?;
        Ok(())
    };
    add(DB_ENTRY, snapshot, packed)?;
    progress(1, total);
    let settings = paths.settings_file();
    if settings.is_file() {
        add(SETTINGS_ENTRY, &settings, packed)?;
    }
    progress(2, total);
    for (index, (source, relative)) in images.iter().enumerate() {
        check()?;
        add(&format!("{IMAGES_ENTRY}/{relative}"), source, stored)?;
        progress(u32::try_from(index + 3).unwrap_or(u32::MAX), total);
    }
    zip.finish()
        .map_err(|e| fail(&e))?
        .flush()
        .map_err(|e| fail(&e))?;
    Ok(())
}

/// Unpacks `archive` into the staging folder for the next start. Refuses archives from a newer
/// Questkeep (their database is newer than this build understands).
pub fn prepare_restore(archive: &Path, paths: &DataPaths) -> Result<()> {
    let manifest = read_manifest(archive)?;
    if manifest.schema_version > LATEST_VERSION {
        return Err(CoreError::Invalid(format!(
            "This backup was made by Questkeep {}, which is newer than this version. Update \
             Questkeep, then restore it.",
            manifest.app_version
        )));
    }
    let staging = paths.root().join(RESTORE_DIR);
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| io_error("clear", &staging, &e))?;
    }
    fs::create_dir_all(&staging).map_err(|e| io_error("create", &staging, &e))?;
    let result = unpack(archive, &staging);
    if let Err(err) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(err);
    }
    if !staging.join(DB_ENTRY).is_file() {
        let _ = fs::remove_dir_all(&staging);
        return Err(unreadable(archive));
    }
    File::create(staging.join(READY)).map_err(|e| io_error("prepare", &staging, &e))?;
    Ok(())
}

/// Extracts the database, settings and images of an archive; anything else (and any path that
/// would leave `target`) is skipped.
fn unpack(archive: &Path, target: &Path) -> Result<()> {
    let mut zip = open_archive(archive)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|_| unreadable(archive))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        let wanted = name == DB_ENTRY
            || name == SETTINGS_ENTRY
            || name.starts_with(&format!("{IMAGES_ENTRY}/"));
        if !wanted || entry.is_dir() {
            continue;
        }
        let out = target.join(&relative);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error("create", parent, &e))?;
        }
        let mut file = File::create(&out).map_err(|e| io_error("write", &out, &e))?;
        io::copy(&mut entry, &mut file).map_err(|e| io_error("write", &out, &e))?;
    }
    Ok(())
}

/// Swaps a prepared restore in, before the database is opened. Returns whether one was applied.
/// The backup settings of the current install are kept. If moving the restored files in fails,
/// the replaced ones are moved back.
pub fn apply_pending_restore(paths: &DataPaths) -> Result<bool> {
    let staging = paths.root().join(RESTORE_DIR);
    if !staging.join(READY).is_file() {
        if staging.exists() {
            // An unfinished preparation: never swap it in.
            let _ = fs::remove_dir_all(&staging);
        }
        return Ok(false);
    }
    keep_backup_settings(&paths.settings_file(), &staging.join(SETTINGS_ENTRY));

    let replaced = paths.root().join(REPLACED_DIR);
    if replaced.exists() {
        fs::remove_dir_all(&replaced).map_err(|e| io_error("clear", &replaced, &e))?;
    }
    fs::create_dir_all(&replaced).map_err(|e| io_error("create", &replaced, &e))?;

    let db = paths.database_file();
    let wal = sibling(&db, "-wal");
    let shm = sibling(&db, "-shm");
    let swaps: Vec<(PathBuf, PathBuf, PathBuf)> = vec![
        (db.clone(), replaced.join(DB_ENTRY), staging.join(DB_ENTRY)),
        (
            paths.settings_file(),
            replaced.join(SETTINGS_ENTRY),
            staging.join(SETTINGS_ENTRY),
        ),
        (
            paths.images_dir(),
            replaced.join(IMAGES_ENTRY),
            staging.join(IMAGES_ENTRY),
        ),
    ];
    // The old database's journal belongs to the old database.
    for journal in [&wal, &shm] {
        if journal.exists() {
            let name = journal.file_name().unwrap_or_default();
            fs::rename(journal, replaced.join(name)).map_err(|e| io_error("move", journal, &e))?;
        }
    }
    let mut moved_out: Vec<&(PathBuf, PathBuf, PathBuf)> = Vec::new();
    let mut moved_in: Vec<&(PathBuf, PathBuf, PathBuf)> = Vec::new();
    let mut outcome = Ok(());
    for swap in &swaps {
        let (current, aside, _) = swap;
        if current.exists() {
            if let Err(e) = fs::rename(current, aside) {
                outcome = Err(io_error("move", current, &e));
                break;
            }
            moved_out.push(swap);
        }
    }
    if outcome.is_ok() {
        for swap in &swaps {
            let (current, _, restored) = swap;
            if restored.exists() {
                if let Err(e) = fs::rename(restored, current) {
                    outcome = Err(io_error("move", restored, &e));
                    break;
                }
                moved_in.push(swap);
            }
        }
    }
    if let Err(err) = outcome {
        // Put everything back as it was.
        for (current, _, restored) in moved_in {
            let _ = fs::rename(current, restored);
        }
        for (current, aside, _) in moved_out {
            let _ = fs::rename(aside, current);
        }
        for journal in [&wal, &shm] {
            let name = journal.file_name().unwrap_or_default();
            let _ = fs::rename(replaced.join(name), journal);
        }
        let _ = fs::remove_file(staging.join(READY));
        return Err(err);
    }
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&replaced);
    Ok(true)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Copies the `backups` settings of the current install into the restored settings file.
fn keep_backup_settings(current: &Path, restored: &Path) {
    let read = |path: &Path| -> Option<serde_json::Value> {
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    };
    let Some(mut target) = read(restored) else {
        return;
    };
    let kept = read(current).and_then(|c| c.get(SETTINGS_KEY).cloned());
    let Some(object) = target.as_object_mut() else {
        return;
    };
    match kept {
        Some(value) => {
            object.insert(SETTINGS_KEY.into(), value);
        }
        None => {
            object.remove(SETTINGS_KEY);
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&target)
        && let Err(err) = fs::write(restored, text)
    {
        tracing::warn!(%err, "could not keep the backup settings in the restored settings");
    }
}

/// Whether a backup runs right now, and the switch that cancels it.
#[derive(Debug, Default)]
pub struct BackupRun {
    running: Mutex<bool>,
    finished: Condvar,
    cancel: AtomicBool,
}

impl BackupRun {
    /// Marks a backup as running; `false` when one already runs.
    pub fn start(&self) -> bool {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if *running {
            return false;
        }
        *running = true;
        self.cancel.store(false, Ordering::Relaxed);
        true
    }

    pub fn finish(&self) {
        *self.running.lock().unwrap_or_else(PoisonError::into_inner) = false;
        self.finished.notify_all();
    }

    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }

    /// Asks a running backup to stop (it deletes its partial archive).
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Blocks until no backup runs.
    pub fn wait(&self) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        while *running {
            running = self
                .finished
                .wait(running)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GameQuery, ImportedGame};

    fn library(root: &Path) -> (DataPaths, Database) {
        let paths = DataPaths::new(root);
        paths.ensure_dirs().unwrap();
        let db = Database::open(&paths.database_file()).unwrap();
        (paths, db)
    }

    fn names(db: &Database) -> Vec<String> {
        db.list_games(&GameQuery::default())
            .unwrap()
            .into_iter()
            .map(|g| g.name)
            .collect()
    }

    #[test]
    fn backs_up_rotates_and_restores_keeping_backup_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = library(tmp.path());
        db.upsert_imported(
            "steam",
            &[ImportedGame::installed("1", "Hades")],
            Utc::now(),
        )
        .unwrap();
        fs::create_dir_all(paths.images_dir().join("covers")).unwrap();
        fs::write(paths.images_dir().join("covers/a.jpg"), b"cover").unwrap();
        fs::write(
            paths.settings_file(),
            r#"{"backups":{"keep":2},"scanOnStartup":false}"#,
        )
        .unwrap();
        let dir = tmp.path().join("archives");
        let calls = Mutex::new(Vec::new());
        let never = AtomicBool::new(false);
        let first = create_backup(&db, &paths, &dir, 2, &never, &|done, total| {
            calls.lock().unwrap().push((done, total));
        })
        .unwrap();
        assert_eq!(calls.lock().unwrap().last(), Some(&(3, 3)));
        assert_eq!(list_backups(&dir).len(), 1);
        assert_eq!(list_backups(&dir)[0].path, first.path);
        assert!(!is_due(&BackupSettings::default(), &dir, Utc::now()));
        assert!(is_due(
            &BackupSettings::default(),
            &dir,
            Utc::now() + Duration::days(8)
        ));

        for _ in 0..2 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            create_backup(&db, &paths, &dir, 2, &never, &|_, _| {}).unwrap();
        }
        let kept = list_backups(&dir);
        assert_eq!(kept.len(), 2, "rotation keeps two");
        assert!(kept.iter().all(|b| b.path != first.path), "the oldest went");

        // Change everything, then restore the newest backup.
        db.upsert_imported(
            "steam",
            &[ImportedGame::installed("2", "Celeste")],
            Utc::now(),
        )
        .unwrap();
        fs::write(paths.images_dir().join("covers/a.jpg"), b"changed").unwrap();
        fs::write(
            paths.settings_file(),
            r#"{"backups":{"keep":9},"scanOnStartup":true}"#,
        )
        .unwrap();
        prepare_restore(Path::new(&kept[0].path), &paths).unwrap();
        drop(db);
        assert!(apply_pending_restore(&paths).unwrap());
        assert!(!apply_pending_restore(&paths).unwrap(), "applied once");

        let db = Database::open(&paths.database_file()).unwrap();
        assert_eq!(names(&db), ["Hades"]);
        assert_eq!(
            fs::read(paths.images_dir().join("covers/a.jpg")).unwrap(),
            b"cover"
        );
        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(paths.settings_file()).unwrap()).unwrap();
        assert_eq!(
            settings["scanOnStartup"], false,
            "settings come from the backup"
        );
        assert_eq!(settings["backups"]["keep"], 9, "backup settings stay");
        assert!(!tmp.path().join(RESTORE_DIR).exists());
    }

    #[test]
    fn a_cancelled_backup_leaves_nothing_and_newer_backups_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = library(tmp.path());
        let dir = tmp.path().join("archives");
        let cancel = AtomicBool::new(true);
        assert!(create_backup(&db, &paths, &dir, 5, &cancel, &|_, _| {}).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);

        // An archive written by a newer Questkeep.
        let future = dir.join(format!("{PREFIX}future{EXTENSION}"));
        let mut zip = ZipWriter::new(File::create(&future).unwrap());
        zip.start_file(MANIFEST, SimpleFileOptions::default())
            .unwrap();
        let manifest = Manifest {
            app_version: "99.0.0".into(),
            schema_version: LATEST_VERSION + 1,
            created_at: Utc::now(),
        };
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.finish().unwrap();
        let err = prepare_restore(&future, &paths).unwrap_err().to_string();
        assert!(err.contains("99.0.0"), "{err}");
        assert!(!apply_pending_restore(&paths).unwrap());
    }
}
