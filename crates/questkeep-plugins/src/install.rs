//! Installing a plugin: its folder (or zip) is copied into `<data>/plugins/<id>/`, and
//! `<data>/plugins/plugins.json` remembers where it came from.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{PluginError, Result};
use crate::manifest::{MANIFEST_FILE, PluginManifest};

/// Name of the state file in the plugins folder.
pub const STATE_FILE: &str = "plugins.json";
/// A plugin may not be larger than this, packed or not.
const MAX_PLUGIN_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PLUGIN_FILES: usize = 20_000;

/// One installed plugin in `plugins.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEntry {
    pub id: String,
    /// The folder or zip it was installed from (absolute).
    pub source: String,
    pub enabled: bool,
    /// Reload whenever the source folder changes.
    #[serde(default)]
    pub watch: bool,
}

/// Reads `plugins.json`; a missing file is an empty list, a broken one is logged and empty.
pub fn load_state(plugins_dir: &Path) -> Vec<StateEntry> {
    let file = plugins_dir.join(STATE_FILE);
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            tracing::error!(file = %file.display(), %err, "could not read the plugin list");
            return Vec::new();
        }
    };
    match serde_json::from_str(&text) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::error!(file = %file.display(), %err, "the plugin list is not valid");
            Vec::new()
        }
    }
}

/// Writes `plugins.json` (to a temporary file first, then renamed over the old one).
pub fn save_state(plugins_dir: &Path, entries: &[StateEntry]) -> Result<()> {
    std::fs::create_dir_all(plugins_dir).map_err(|e| PluginError::io(plugins_dir, e))?;
    let file = plugins_dir.join(STATE_FILE);
    let temp = plugins_dir.join(format!("{STATE_FILE}.{}.tmp", unique()));
    let text = serde_json::to_string_pretty(entries)
        .map_err(|e| PluginError::Invalid(format!("Could not write the plugin list: {e}.")))?;
    std::fs::write(&temp, text).map_err(|e| PluginError::io(&temp, e))?;
    rename(&temp, &file).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// A checked copy of a plugin, ready to take its place among the installed ones.
pub struct Staged {
    staging: Staging,
    manifest: PluginManifest,
    target: PathBuf,
}

impl Staged {
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Puts the copy at `<plugins_dir>/<id>/`, replacing an installed copy with the same id:
    /// the old copy steps aside, the new one takes its place, or everything stays as it was.
    pub fn commit(self) -> Result<PluginManifest> {
        let staged = self.staging.path.join(STAGED_PLUGIN);
        let old = self.staging.path.join("old");
        let had_old = self.target.exists();
        if had_old {
            rename(&self.target, &old)?;
        }
        if let Err(err) = rename(&staged, &self.target) {
            if had_old && let Err(restore) = rename(&old, &self.target) {
                tracing::error!(plugin = %self.manifest.id, %restore, "could not restore the old copy");
            }
            return Err(err);
        }
        Ok(self.manifest)
    }
}

const STAGED_PLUGIN: &str = "plugin";

/// Copies the plugin in `source` (a folder, or a zip of one) next to the installed plugins and
/// checks it. Nothing installed changes until [`Staged::commit`]; dropping the result throws
/// the copy away. With `expect_id` (a reload) the source must still be that plugin.
pub fn stage(source: &Path, plugins_dir: &Path, expect_id: Option<&str>) -> Result<Staged> {
    std::fs::create_dir_all(plugins_dir).map_err(|e| PluginError::io(plugins_dir, e))?;
    if inside(source, plugins_dir) {
        return Err(PluginError::Invalid(
            "This is Questkeep's own copy of a plugin. Install the plugin from its original \
             folder or file."
                .into(),
        ));
    }
    let staging = Staging::create(plugins_dir)?;
    let staged = staging.path.join(STAGED_PLUGIN);
    let manifest = if source.is_dir() {
        let manifest = PluginManifest::load(source)?;
        copy_plugin(source, &staged)?;
        manifest
    } else if source.is_file() {
        let unpacked = staging.path.join("unpacked");
        unzip(source, &unpacked)?;
        let root = plugin_root(&unpacked)?;
        let manifest = PluginManifest::load(&root)?;
        rename(&root, &staged)?;
        manifest
    } else {
        return Err(PluginError::Invalid(format!(
            "{} does not exist. Choose a plugin folder or a .zip file.",
            source.display()
        )));
    };
    if let Some(expected) = expect_id.filter(|id| *id != manifest.id) {
        return Err(PluginError::Invalid(format!(
            "{} now contains the plugin \"{}\" instead of \"{expected}\".",
            source.display(),
            manifest.id
        )));
    }
    for provider in &manifest.providers {
        if !staged.join(&provider.wasm).is_file() {
            return Err(PluginError::Invalid(format!(
                "The plugin's store file \"{}\" is missing. If this is a plugin's source \
                 folder, build it first.",
                provider.wasm
            )));
        }
    }
    let target = plugins_dir.join(&manifest.id);
    Ok(Staged {
        staging,
        manifest,
        target,
    })
}

/// [`stage`] and [`Staged::commit`] in one step: the copy is complete or not there at all.
pub fn install(
    source: &Path,
    plugins_dir: &Path,
    expect_id: Option<&str>,
) -> Result<PluginManifest> {
    stage(source, plugins_dir, expect_id)?.commit()
}

/// Deletes the installed copy.
pub fn uninstall(plugins_dir: &Path, id: &str) -> Result<()> {
    remove_dir(&plugins_dir.join(id))
}

/// Deletes a folder; a missing one is fine.
pub fn remove_dir(dir: &Path) -> Result<()> {
    match retry(|| std::fs::remove_dir_all(dir)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(PluginError::io(dir, err)),
    }
}

/// A scratch folder next to the installed plugins (same volume, so renames are atomic); deleted
/// when dropped.
struct Staging {
    path: PathBuf,
}

impl Staging {
    fn create(plugins_dir: &Path) -> Result<Self> {
        let path = plugins_dir.join(format!(".staging-{}", unique()));
        std::fs::create_dir_all(&path).map_err(|e| PluginError::io(&path, e))?;
        Ok(Self { path })
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if let Err(err) = retry(|| std::fs::remove_dir_all(&self.path)) {
            tracing::warn!(path = %self.path.display(), %err, "could not remove a temporary folder");
        }
    }
}

/// Removes staging folders a crash left behind.
pub fn clean_leftovers(plugins_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".staging-") {
            let _ = std::fs::remove_dir_all(entry.path());
        } else if name.starts_with(STATE_FILE) && name.ends_with(".tmp") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn unique() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    format!(
        "{}-{}-{nanos:x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Windows refuses to rename or delete what a virus scanner or indexer is looking at: try a few
/// times before giving up.
fn retry<T>(mut action: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut attempt = 0;
    loop {
        match action() {
            Err(err)
                if attempt < 5
                    && (matches!(
                        err.kind(),
                        std::io::ErrorKind::PermissionDenied
                            | std::io::ErrorKind::DirectoryNotEmpty
                            | std::io::ErrorKind::ResourceBusy
                    ) || is_sharing_violation(&err)) =>
            {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(40 * attempt));
            }
            other => return other,
        }
    }
}

/// Windows: "The process cannot access the file because it is being used by another process."
fn is_sharing_violation(err: &std::io::Error) -> bool {
    cfg!(windows) && err.raw_os_error() == Some(32)
}

fn rename(from: &Path, to: &Path) -> Result<()> {
    retry(|| std::fs::rename(from, to)).map_err(|e| PluginError::io(to, e))
}

/// Whether `path` is `dir` or inside it.
fn inside(path: &Path, dir: &Path) -> bool {
    match (std::fs::canonicalize(path), std::fs::canonicalize(dir)) {
        (Ok(path), Ok(dir)) => path.starts_with(dir),
        _ => false,
    }
}

/// Folders of a plugin's source that are not part of the plugin: build output, dependencies and
/// hidden folders (`.git`). The source watcher ignores the same ones.
pub fn is_ignored_dir(name: &str) -> bool {
    name.starts_with('.') || name == "target" || name == "node_modules"
}

#[derive(Default)]
struct Budget {
    bytes: u64,
    files: usize,
}

impl Budget {
    fn add_file(&mut self) -> Result<()> {
        self.files += 1;
        if self.files > MAX_PLUGIN_FILES {
            return Err(PluginError::Invalid(format!(
                "The plugin has more than {MAX_PLUGIN_FILES} files."
            )));
        }
        Ok(())
    }

    fn add_bytes(&mut self, bytes: u64) -> Result<()> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > MAX_PLUGIN_BYTES {
            return Err(PluginError::Invalid(format!(
                "The plugin is larger than {} MB.",
                MAX_PLUGIN_BYTES / (1024 * 1024)
            )));
        }
        Ok(())
    }
}

fn copy_plugin(from: &Path, to: &Path) -> Result<()> {
    copy_dir(from, to, &mut Budget::default())
}

fn copy_dir(from: &Path, to: &Path, budget: &mut Budget) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|e| PluginError::io(to, e))?;
    let entries = std::fs::read_dir(from).map_err(|e| PluginError::io(from, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| PluginError::io(from, e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| PluginError::io(&path, e))?;
        let name = entry.file_name();
        let target = to.join(&name);
        if kind.is_dir() {
            if !is_ignored_dir(&name.to_string_lossy()) {
                copy_dir(&path, &target, budget)?;
            }
        } else if kind.is_file() {
            budget.add_file()?;
            let size = entry
                .metadata()
                .map_err(|e| PluginError::io(&path, e))?
                .len();
            budget.add_bytes(size)?;
            std::fs::copy(&path, &target).map_err(|e| PluginError::io(&path, e))?;
        }
        // Links are not followed: a plugin is plain files.
    }
    Ok(())
}

fn unzip(archive: &Path, to: &Path) -> Result<()> {
    let not_a_zip = |err: &dyn std::fmt::Display| {
        PluginError::Invalid(format!(
            "{} is not a plugin: it is neither a folder nor a readable .zip file ({err}).",
            archive.display()
        ))
    };
    let file = std::fs::File::open(archive).map_err(|e| PluginError::io(archive, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| not_a_zip(&e))?;
    let mut budget = Budget::default();
    std::fs::create_dir_all(to).map_err(|e| PluginError::io(to, e))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| not_a_zip(&e))?;
        // Zip slip: never write outside the target folder, whatever the entry is called.
        let Some(relative) = entry.enclosed_name() else {
            return Err(PluginError::Invalid(format!(
                "The archive contains \"{}\", which points outside the plugin folder.",
                String::from_utf8_lossy(entry.name_raw())
            )));
        };
        let target = to.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| PluginError::io(&target, e))?;
            continue;
        }
        if entry.is_symlink() {
            continue;
        }
        budget.add_file()?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::io(parent, e))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| PluginError::io(&target, e))?;
        // The declared size can lie: count what really comes out.
        let left = MAX_PLUGIN_BYTES.saturating_sub(budget.bytes);
        let written = std::io::copy(&mut (&mut entry).take(left + 1), &mut out)
            .map_err(|e| PluginError::io(&target, e))?;
        budget.add_bytes(written)?;
    }
    Ok(())
}

/// The folder with `plugin.json` in an unpacked archive: the top, or its only folder.
fn plugin_root(unpacked: &Path) -> Result<PathBuf> {
    if unpacked.join(MANIFEST_FILE).is_file() {
        return Ok(unpacked.to_path_buf());
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(unpacked)
        .map_err(|e| PluginError::io(unpacked, e))?
        .flatten()
        .map(|e| e.path())
        .collect();
    match entries.as_slice() {
        [only] if only.join(MANIFEST_FILE).is_file() => Ok(only.clone()),
        _ => Err(PluginError::Invalid(format!(
            "This archive is not a plugin: there is no {MANIFEST_FILE} at its top or in its \
             only folder."
        ))),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::io::Write;
    use std::path::Path;

    /// A plugin folder with a manifest and a (fake) store file.
    pub fn write_plugin(dir: &Path, id: &str, version: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let manifest = serde_json::json!({
            "id": id, "name": format!("Plugin {id}"), "version": version, "apiVersion": 1,
            "themes": [{ "id": "t", "name": "T", "css": "theme.css" }],
        });
        std::fs::write(dir.join("plugin.json"), manifest.to_string()).unwrap();
        std::fs::write(dir.join("theme.css"), format!("/* {version} */")).unwrap();
    }

    pub fn write_zip(file: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(file).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            if name.ends_with('/') {
                zip.add_directory(name.trim_end_matches('/'), options)
                    .unwrap();
            } else {
                zip.start_file(*name, options).unwrap();
                zip.write_all(content).unwrap();
            }
        }
        zip.finish().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    const MANIFEST: &[u8] = br#"{"id":"zipped","name":"Zipped","version":"1.0.0","apiVersion":1}"#;

    #[test]
    fn installs_a_folder_and_replaces_it() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("src");
        let plugins = tmp.path().join("data/plugins");
        write_plugin(&source, "my-plugin", "1.0.0");
        std::fs::create_dir_all(source.join("sub")).unwrap();
        std::fs::write(source.join("sub/a.txt"), "a").unwrap();
        std::fs::write(source.join("old.txt"), "old").unwrap();
        for ignored in ["target/debug", ".git", "node_modules/x"] {
            std::fs::create_dir_all(source.join(ignored)).unwrap();
            std::fs::write(source.join(ignored).join("big.bin"), "x").unwrap();
        }

        let manifest = install(&source, &plugins, None).unwrap();
        assert_eq!(manifest.version, "1.0.0");
        let installed = plugins.join("my-plugin");
        assert!(installed.join("plugin.json").is_file());
        assert_eq!(
            std::fs::read_to_string(installed.join("sub/a.txt")).unwrap(),
            "a"
        );
        assert!(!installed.join("target").exists());
        assert!(!installed.join(".git").exists());
        assert!(!installed.join("node_modules").exists());

        // An update replaces the whole copy: files that are gone in the source are gone.
        write_plugin(&source, "my-plugin", "2.0.0");
        std::fs::remove_file(source.join("old.txt")).unwrap();
        let manifest = install(&source, &plugins, Some("my-plugin")).unwrap();
        assert_eq!(manifest.version, "2.0.0");
        assert!(!installed.join("old.txt").exists());
        assert_eq!(
            std::fs::read_to_string(installed.join("theme.css")).unwrap(),
            "/* 2.0.0 */"
        );
        // Nothing is left behind.
        let names: Vec<String> = std::fs::read_dir(&plugins)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["my-plugin"]);

        uninstall(&plugins, "my-plugin").unwrap();
        assert!(!installed.exists());
        uninstall(&plugins, "my-plugin").unwrap();
    }

    #[test]
    fn a_failed_install_keeps_the_old_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("src");
        let plugins = tmp.path().join("plugins");
        write_plugin(&source, "keeper", "1.0.0");
        install(&source, &plugins, None).unwrap();

        // The store file named by the manifest is missing.
        let broken = serde_json::json!({
            "id": "keeper", "name": "K", "version": "2.0.0", "apiVersion": 1,
            "providers": [{ "wasm": "provider.wasm" }],
        });
        std::fs::write(source.join("plugin.json"), broken.to_string()).unwrap();
        let err = install(&source, &plugins, None).unwrap_err().to_string();
        assert!(
            err.contains("provider.wasm") && err.contains("build it first"),
            "{err}"
        );
        // Invalid manifest.
        std::fs::write(source.join("plugin.json"), "{").unwrap();
        assert!(install(&source, &plugins, None).is_err());
        // Another plugin where this one was (reload).
        write_plugin(&source, "other", "1.0.0");
        let err = install(&source, &plugins, Some("keeper")).unwrap_err();
        assert!(err.to_string().contains("instead of \"keeper\""));

        let kept = PluginManifest::load(&plugins.join("keeper")).unwrap();
        assert_eq!(kept.version, "1.0.0");
        assert!(!plugins.join("other").exists());
        assert_eq!(std::fs::read_dir(&plugins).unwrap().count(), 1);
    }

    #[test]
    fn rejects_what_is_not_a_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = tmp.path().join("plugins");
        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let err = install(&empty, &plugins, None).unwrap_err().to_string();
        assert!(err.contains("no plugin.json"), "{err}");
        let err = install(&tmp.path().join("nope"), &plugins, None).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
        let text = tmp.path().join("notes.txt");
        std::fs::write(&text, "hello").unwrap();
        let err = install(&text, &plugins, None).unwrap_err().to_string();
        assert!(
            err.contains("neither a folder nor a readable .zip"),
            "{err}"
        );

        // The installed copy is not a source.
        let source = tmp.path().join("src");
        write_plugin(&source, "self", "1.0.0");
        install(&source, &plugins, None).unwrap();
        let err = install(&plugins.join("self"), &plugins, None).unwrap_err();
        assert!(err.to_string().contains("own copy"));
    }

    #[test]
    fn installs_zips_with_the_manifest_at_the_top_or_in_one_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = tmp.path().join("plugins");
        let flat = tmp.path().join("flat.zip");
        write_zip(&flat, &[("plugin.json", MANIFEST), ("css/t.css", b"x")]);
        assert_eq!(install(&flat, &plugins, None).unwrap().id, "zipped");
        assert!(plugins.join("zipped/css/t.css").is_file());

        let nested = tmp.path().join("nested.zip");
        write_zip(
            &nested,
            &[
                ("zipped-1.0/", b""),
                ("zipped-1.0/plugin.json", MANIFEST),
                ("zipped-1.0/readme.md", b"hi"),
            ],
        );
        install(&nested, &plugins, None).unwrap();
        assert!(plugins.join("zipped/readme.md").is_file());
        assert!(!plugins.join("zipped/css").exists());

        let two = tmp.path().join("two.zip");
        write_zip(&two, &[("a/plugin.json", MANIFEST), ("b/x.txt", b"x")]);
        let err = install(&two, &plugins, None).unwrap_err().to_string();
        assert!(err.contains("not a plugin"), "{err}");
    }

    #[test]
    fn zip_entries_cannot_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = tmp.path().join("data/plugins");
        for evil in ["../evil.txt", "a/../../../evil.txt", "/evil.txt"] {
            let archive = tmp.path().join("evil.zip");
            write_zip(&archive, &[("plugin.json", MANIFEST), (evil, b"boo")]);
            let result = install(&archive, &plugins, None);
            // Refused, or kept inside the plugin folder (the zip library strips a leading `/`).
            if let Err(err) = &result {
                assert!(
                    err.to_string().contains("outside the plugin folder"),
                    "{err}"
                );
                assert!(!plugins.join("zipped").exists());
            }
            assert!(!tmp.path().join("evil.txt").exists(), "{evil}");
            assert!(!tmp.path().join("data/evil.txt").exists(), "{evil}");
            assert!(!plugins.join("evil.txt").exists(), "{evil}");
            let _ = uninstall(&plugins, "zipped");
        }
        let archive = tmp.path().join("evil.zip");
        write_zip(
            &archive,
            &[("plugin.json", MANIFEST), ("../evil.txt", b"boo")],
        );
        assert!(install(&archive, &plugins, None).is_err());
    }

    #[test]
    fn state_round_trips_and_survives_garbage() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = tmp.path().join("plugins");
        assert!(load_state(&plugins).is_empty());
        let entries = vec![
            StateEntry {
                id: "a".into(),
                source: "C:\\src\\a".into(),
                enabled: true,
                watch: true,
            },
            StateEntry {
                id: "b".into(),
                source: "/src/b.zip".into(),
                enabled: false,
                watch: false,
            },
        ];
        save_state(&plugins, &entries).unwrap();
        assert_eq!(load_state(&plugins), entries);
        save_state(&plugins, &entries[..1]).unwrap();
        assert_eq!(load_state(&plugins), entries[..1]);
        assert_eq!(std::fs::read_dir(&plugins).unwrap().count(), 1);
        // `watch` is optional (older files).
        std::fs::write(
            plugins.join(STATE_FILE),
            r#"[{"id":"a","source":"x","enabled":true}]"#,
        )
        .unwrap();
        assert!(!load_state(&plugins)[0].watch);
        std::fs::write(plugins.join(STATE_FILE), "not json").unwrap();
        assert!(load_state(&plugins).is_empty());
    }

    #[test]
    fn leftovers_are_cleaned() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".staging-1/plugin")).unwrap();
        std::fs::write(tmp.path().join("plugins.json.1.tmp"), "x").unwrap();
        std::fs::write(tmp.path().join(STATE_FILE), "[]").unwrap();
        std::fs::create_dir_all(tmp.path().join("kept")).unwrap();
        clean_leftovers(tmp.path());
        let mut names: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["kept", "plugins.json"]);
    }
}
