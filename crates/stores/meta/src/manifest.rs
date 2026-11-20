//! Library folders and the app manifests in them.
//!
//! A library folder (e.g. `D:\Oculus\Software`) holds `Manifests\<canonicalName>.json` (and a
//! smaller `.json.mini` copy without the full file list) per installed package, and the files in
//! `Software\<canonicalName>\`. The app's own packages (`oculus-*`, `isCore`, `packageType`
//! `CORE_APP`) and the asset bundles of store art (`*_assets`, `ASSET_BUNDLE`) live in the same
//! kind of folders and are skipped.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The fields of a manifest that matter here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Manifest {
    pub canonical_name: String,
    pub app_id: Option<String>,
    pub launch_file: Option<String>,
    pub launch_parameters: Option<String>,
    pub package_type: Option<String>,
    pub is_core: bool,
    pub third_party: bool,
    pub parent_canonical_name: Option<String>,
}

impl Manifest {
    pub fn parse(json: &str) -> Option<Self> {
        // Some manifests start with a byte order mark.
        serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()
    }

    /// Whether this is a store app of the user's (a game or VR app) rather than a part of the
    /// Meta app, a DLC, store art, or a desktop program the app noticed running.
    pub fn is_store_app(&self) -> bool {
        let app_type = self
            .package_type
            .as_deref()
            .is_none_or(|t| t.eq_ignore_ascii_case("APP"));
        app_type
            && !self.is_core
            && !self.third_party
            && self.parent_canonical_name.is_none()
            && !self.canonical_name.is_empty()
            && !self.canonical_name.starts_with("oculus-")
            && !self.canonical_name.ends_with("_assets")
            && self.app_id.as_deref().is_some_and(plausible_app_id)
            && self
                .launch_file
                .as_deref()
                .is_some_and(|f| !f.trim().is_empty())
    }
}

/// Store ids are decimal numbers; anything else is not passed on to URIs or paths.
pub fn plausible_app_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 24 && id.bytes().all(|b| b.is_ascii_digit())
}

/// An installed app found in a library folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    pub manifest: Manifest,
    /// `<library>\Software\<canonicalName>`.
    pub dir: PathBuf,
    pub exe: PathBuf,
}

/// A path inside `dir` from a manifest's `/`- or `\`-separated relative path; `None` when it
/// would leave `dir`.
fn inside(dir: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = dir.to_path_buf();
    for part in relative.split(['/', '\\']).filter(|p| !p.is_empty()) {
        if part == ".." || part == "." || part.contains(':') {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

/// The manifests in `<library>\Manifests`, preferring the small `.json.mini` copy of each.
fn manifests(library: &Path) -> Vec<Manifest> {
    let Ok(entries) = std::fs::read_dir(library.join("Manifests")) else {
        return Vec::new();
    };
    let mut names: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let lower = file.to_ascii_lowercase();
        let (canonical, mini) = if let Some(stem) = lower.strip_suffix(".json.mini") {
            (stem.to_string(), true)
        } else if let Some(stem) = lower.strip_suffix(".json") {
            (stem.to_string(), false)
        } else {
            continue;
        };
        if canonical.ends_with("_assets") {
            continue;
        }
        match names.iter_mut().find(|(c, _)| *c == canonical) {
            Some(existing) if mini => existing.1 = entry.path(),
            Some(_) => {}
            None => names.push((canonical, entry.path())),
        }
    }
    names.sort();
    names
        .into_iter()
        .filter_map(|(_, path)| {
            let bytes = std::fs::read(&path).ok()?;
            let manifest = Manifest::parse(&String::from_utf8_lossy(&bytes));
            if manifest.is_none() {
                tracing::debug!("unreadable Meta manifest {}", path.display());
            }
            manifest
        })
        .collect()
}

/// The installed store apps of one library folder whose executable exists.
pub fn installs(library: &Path) -> Vec<Install> {
    manifests(library)
        .into_iter()
        .filter(Manifest::is_store_app)
        .filter_map(|manifest| {
            let dir = inside(&library.join("Software"), &manifest.canonical_name)?;
            let exe = inside(&dir, manifest.launch_file.as_deref()?)?;
            exe.is_file().then_some(Install { manifest, dir, exe })
        })
        .collect()
}

/// `(appId, canonicalName)` of the store art bundles in `<base>\CoreData\Manifests`: one per app
/// the Meta app shows in its library, installed or not.
pub fn asset_bundles(core_data: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(core_data.join("Manifests")) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.ends_with("_assets.json")
        })
        .filter_map(|e| {
            let bytes = std::fs::read(e.path()).ok()?;
            let manifest = Manifest::parse(&String::from_utf8_lossy(&bytes))?;
            let id = manifest.app_id.filter(|id| plausible_app_id(id))?;
            if manifest.third_party {
                return None;
            }
            let canonical = manifest
                .canonical_name
                .strip_suffix("_assets")
                .unwrap_or(&manifest.canonical_name)
                .to_string();
            (!canonical.is_empty()).then_some((id, canonical))
        })
        .collect();
    out.sort();
    out
}

/// `\\?\C:\x` -> `C:\x`, `\\?\UNC\server\share` -> `\\server\share`; other paths unchanged.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

/// The folder of a library from its registry `Path` (often a volume path such as
/// `\\?\Volume{...}\Games\Oculus`) and `OriginalPath` (the drive-letter path the user picked).
/// Volume paths are turned into drive-letter paths, so game paths and process tracking match the
/// paths Windows reports for running games.
pub fn resolve_library(path: &str, original: Option<&str>) -> Option<PathBuf> {
    let path = path.trim().trim_end_matches(['\\', '/']);
    let original = original
        .map(|o| o.trim().trim_end_matches(['\\', '/']))
        .filter(|o| !o.is_empty())
        .map(PathBuf::from);
    if path.is_empty() {
        return original.filter(|o| o.is_dir());
    }
    let raw = PathBuf::from(path);
    if !path.starts_with(r"\\?\") {
        return Some(raw);
    }
    // Resolving the volume path gives `\\?\<drive>:\...` when the volume has a letter.
    if let Ok(resolved) = std::fs::canonicalize(&raw) {
        let resolved = strip_verbatim(resolved);
        if !resolved.to_string_lossy().starts_with(r"\\?\") {
            return Some(resolved);
        }
    }
    if let Some(original) = original.filter(|o| o.is_dir()) {
        return Some(original);
    }
    Some(raw)
}

/// A readable name from a canonical name: `mighty-coconut-walkabout-mini-golf` ->
/// `Mighty Coconut Walkabout Mini Golf`.
pub fn name_from_canonical(canonical: &str) -> String {
    canonical
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/library/Manifests")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn parses_manifests_and_skips_the_apps_own_packages() {
        let game = Manifest::parse(&fixture("moonlit-studio-moon-garden.json.mini")).unwrap();
        assert_eq!(game.canonical_name, "moonlit-studio-moon-garden");
        assert_eq!(game.app_id.as_deref(), Some("2000000000000002"));
        assert_eq!(game.launch_file.as_deref(), Some("MoonGarden.exe"));
        assert_eq!(game.launch_parameters.as_deref(), Some("-vrmode oculus"));
        assert!(game.is_store_app());

        let core = Manifest::parse(&fixture("oculus-runtime.json.mini")).unwrap();
        assert!(core.is_core && !core.is_store_app());
        let dlc = Manifest::parse(&fixture("moonlit-studio-moon-garden-dlc.json")).unwrap();
        assert!(!dlc.is_store_app());
        let bom = format!(
            "\u{feff}{}",
            fixture("moonlit-studio-moon-garden.json.mini")
        );
        assert_eq!(Manifest::parse(&bom), Some(game));
        assert_eq!(Manifest::parse("not json"), None);
    }

    #[test]
    fn finds_installed_apps_whose_executable_exists() {
        let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/library");
        let found = installs(&library);
        let names: Vec<_> = found
            .iter()
            .map(|i| i.manifest.canonical_name.as_str())
            .collect();
        // `starfall-arcade` has no executable (half installed); the rest are not store apps.
        assert_eq!(names, ["moonlit-studio-moon-garden", "tiny-forge-sky-loop"]);
        let moon = &found[0];
        assert_eq!(
            moon.dir,
            library.join("Software").join("moonlit-studio-moon-garden")
        );
        assert_eq!(moon.exe, moon.dir.join("MoonGarden.exe"));
        // A launch file in a subfolder, written with forward slashes.
        assert_eq!(
            found[1].exe,
            library
                .join("Software")
                .join("tiny-forge-sky-loop")
                .join("bin")
                .join("SkyLoop.exe")
        );
        assert!(installs(&library.join("missing")).is_empty());
    }

    #[test]
    fn launch_files_stay_inside_the_install_folder() {
        let dir = Path::new("lib");
        assert_eq!(inside(dir, "a/b.exe"), Some(dir.join("a").join("b.exe")));
        assert_eq!(inside(dir, r"..\evil.exe"), None);
        assert_eq!(inside(dir, r"C:\evil.exe"), None);
    }

    #[test]
    fn lists_asset_bundles_by_app_id() {
        let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/CoreData");
        assert_eq!(
            asset_bundles(&core),
            [
                (
                    "2000000000000002".to_string(),
                    "moonlit-studio-moon-garden".to_string()
                ),
                (
                    "2000000000000003".to_string(),
                    "starfall-arcade".to_string()
                ),
            ]
        );
    }

    #[test]
    fn names_from_canonical_names() {
        assert_eq!(
            name_from_canonical("mighty-coconut-walkabout-mini-golf"),
            "Mighty Coconut Walkabout Mini Golf"
        );
        assert_eq!(name_from_canonical("a--b_c"), "A B C");
        assert_eq!(name_from_canonical(""), "");
    }

    #[test]
    fn resolves_library_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let original = tmp.path().to_string_lossy().into_owned();
        // A plain path is used as it is.
        assert_eq!(
            resolve_library(r"D:\Oculus\Software\", None),
            Some(PathBuf::from(r"D:\Oculus\Software"))
        );
        // A volume that doesn't exist falls back to the existing original path.
        let missing = r"\\?\Volume{00000000-0000-0000-0000-000000000000}\Oculus\Software";
        assert_eq!(
            resolve_library(missing, Some(&original)),
            Some(tmp.path().to_path_buf())
        );
        assert_eq!(
            resolve_library(missing, Some(r"Z:\gone")),
            Some(PathBuf::from(missing))
        );
        assert_eq!(
            resolve_library("", Some(&original)),
            Some(tmp.path().to_path_buf())
        );
        assert_eq!(resolve_library("", None), None);
    }

    #[test]
    fn strips_verbatim_prefixes() {
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\F:\Oculus")),
            PathBuf::from(r"F:\Oculus")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\UNC\nas\games")),
            PathBuf::from(r"\\nas\games")
        );
        let volume = PathBuf::from(r"\\?\Volume{1}\x");
        assert_eq!(strip_verbatim(volume.clone()), volume);
    }

    // Resolving a real volume path needs Windows.
    #[cfg(windows)]
    #[test]
    fn resolves_a_verbatim_drive_path_to_the_drive_letter() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(tmp.path()).unwrap();
        let resolved = resolve_library(&canonical.to_string_lossy(), None).unwrap();
        assert!(!resolved.to_string_lossy().starts_with(r"\\?\"));
        assert!(resolved.is_dir());
    }
}
