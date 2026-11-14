//! Themes the user made in the theme editor: one JSON file each in `<data>/themes`, and their
//! export as a theme plugin folder.
//!
//! A user theme is a base theme plus overrides of `--qk-*` variables (for both schemes, or light
//! or dark only) and free CSS. The UI turns that into the theme's CSS; the core only stores and
//! validates it, so nothing that could break out of a CSS declaration ends up on disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{CoreError, Result};
use crate::paths::DataPaths;

/// What every user theme id starts with.
pub const USER_THEME_PREFIX: &str = "user-";
/// Characters after the prefix in a user theme id.
const MAX_ID_SUFFIX: usize = 48;
const MAX_NAME_CHARS: usize = 80;
const MAX_BASE_CHARS: usize = 128;
const MAX_VARIABLE_CHARS: usize = 96;
const MAX_VALUE_CHARS: usize = 512;
const MAX_OVERRIDES: usize = 2000;
/// Free CSS of a theme.
const MAX_CSS_BYTES: usize = 256 * 1024;
/// Generated CSS of an exported theme (base theme + overrides + free CSS).
const MAX_EXPORT_CSS_BYTES: usize = 1024 * 1024;

/// A theme made in the theme editor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct UserTheme {
    /// `user-<kebab-name>`; also the file name.
    pub id: String,
    pub name: String,
    /// The id of the theme it derives from (its CSS and parameters come first).
    pub base: String,
    pub overrides: ThemeOverrides,
    /// Free CSS, applied only while the theme is active.
    pub css: String,
}

/// Variable overrides: CSS variable name (with the `--qk-` prefix) to value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ThemeOverrides {
    /// In light and dark mode.
    pub both: BTreeMap<String, String>,
    /// In light mode only (wins over `both`).
    pub light: BTreeMap<String, String>,
    /// In dark mode only (wins over `both`).
    pub dark: BTreeMap<String, String>,
}

/// What the UI generated for an export: the theme's whole CSS (written against the plugin's theme
/// id), its parameters and preview colors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportedTheme {
    pub css: String,
    /// `ThemeParam` objects as in `ui/src/lib/themes.ts`, with literal labels.
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    /// `{ light: [bg, surface, text, accent], dark: [...] }`, or null.
    #[ts(type = "unknown")]
    pub preview: serde_json::Value,
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Invalid(message.into())
}

fn is_slug_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

/// `user-` followed by 1 to 48 lower-case letters, digits or dashes.
fn validate_id(id: &str) -> Result<()> {
    let ok = id.strip_prefix(USER_THEME_PREFIX).is_some_and(|rest| {
        !rest.is_empty() && rest.len() <= MAX_ID_SUFFIX && rest.chars().all(is_slug_char)
    });
    if ok {
        Ok(())
    } else {
        Err(invalid(format!("'{id}' is not a valid theme id")))
    }
}

/// `--qk-` followed by lower-case letters, digits or dashes.
fn validate_variable(name: &str) -> Result<()> {
    let ok = name.len() <= MAX_VARIABLE_CHARS
        && name
            .strip_prefix("--qk-")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(is_slug_char));
    if ok {
        Ok(())
    } else {
        Err(invalid(format!("'{name}' is not a theme variable")))
    }
}

/// A value must stay inside its declaration: no braces, no semicolon, no markup, one line.
fn validate_value(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(invalid(format!("The value of {name} is empty")));
    }
    if value.chars().count() > MAX_VALUE_CHARS {
        return Err(invalid(format!("The value of {name} is too long")));
    }
    if value.contains(['{', '}', ';'])
        || value.contains("</")
        || value.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "The value of {name} contains characters that are not allowed ({{ }} ; </)"
        )));
    }
    Ok(())
}

fn validate(theme: &UserTheme) -> Result<()> {
    validate_id(&theme.id)?;
    let name = theme.name.trim();
    if name.is_empty() {
        return Err(invalid("The theme needs a name"));
    }
    if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(invalid("The theme's name is too long"));
    }
    let base = &theme.base;
    if base.is_empty()
        || base.chars().count() > MAX_BASE_CHARS
        || base
            .chars()
            .any(|c| c.is_control() || c == '"' || c == '\\')
    {
        return Err(invalid("The theme's base theme is not valid"));
    }
    let overrides = &theme.overrides;
    let maps = [&overrides.both, &overrides.light, &overrides.dark];
    if maps.iter().map(|m| m.len()).sum::<usize>() > MAX_OVERRIDES {
        return Err(invalid("The theme has too many overrides"));
    }
    for (name, value) in maps.into_iter().flatten() {
        validate_variable(name)?;
        validate_value(name, value)?;
    }
    if theme.css.len() > MAX_CSS_BYTES {
        return Err(invalid(
            "The theme's custom CSS is too large (256 KiB at most)",
        ));
    }
    Ok(())
}

/// The id of the plugin a user theme is exported as: its id without `user-`, plus `-theme`.
pub fn plugin_id(theme_id: &str) -> String {
    let stem = theme_id.strip_prefix(USER_THEME_PREFIX).unwrap_or(theme_id);
    format!("{stem}-theme")
}

/// The user's themes on disk.
#[derive(Debug, Clone)]
pub struct UserThemes {
    dir: PathBuf,
}

impl UserThemes {
    pub fn new(paths: &DataPaths) -> Self {
        Self {
            dir: paths.themes_dir(),
        }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Every valid theme, by name. Files that are not a valid theme (or whose id is not their
    /// file name) are skipped and logged, so one broken file never hides the others.
    pub fn list(&self) -> Result<Vec<UserTheme>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err.into()),
        };
        let mut themes = Vec::new();
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match read(&path) {
                Ok(theme) => themes.push(theme),
                Err(err) => {
                    tracing::warn!(%err, path = %path.display(), "skipping an invalid theme file");
                }
            }
        }
        themes.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
        Ok(themes)
    }

    /// One theme by id.
    pub fn get(&self, id: &str) -> Result<UserTheme> {
        validate_id(id)?;
        let path = self.file(id);
        if !path.is_file() {
            return Err(invalid(format!("The theme '{id}' does not exist")));
        }
        read(&path)
    }

    /// Validates and writes a theme (atomically: temp file + rename), replacing the one with the
    /// same id.
    pub fn save(&self, theme: &UserTheme) -> Result<()> {
        validate(theme)?;
        let mut theme = theme.clone();
        theme.name = theme.name.trim().to_string();
        std::fs::create_dir_all(&self.dir)?;
        let path = self.file(&theme.id);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&theme)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Removes a theme; one that is already gone is fine.
    pub fn delete(&self, id: &str) -> Result<()> {
        validate_id(id)?;
        match std::fs::remove_file(self.file(id)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Writes the theme as a plugin folder `<target_dir>/<plugin id>` with `plugin.json` and
    /// `theme.css`, and returns that folder. Refuses to write into an existing folder that is
    /// not empty.
    pub fn export_plugin(
        &self,
        id: &str,
        target_dir: &Path,
        exported: &ExportedTheme,
    ) -> Result<PathBuf> {
        let theme = self.get(id)?;
        if exported.css.len() > MAX_EXPORT_CSS_BYTES {
            return Err(invalid("The theme's CSS is too large to export"));
        }
        if !exported.params.is_array() {
            return Err(invalid("The theme's parameters are not a list"));
        }
        if !target_dir.is_dir() {
            return Err(invalid(format!(
                "The folder {} does not exist",
                target_dir.display()
            )));
        }
        let plugin = plugin_id(&theme.id);
        let folder = target_dir.join(&plugin);
        if folder.exists() {
            let empty = folder.is_dir() && std::fs::read_dir(&folder)?.next().is_none();
            if !empty {
                return Err(invalid(format!(
                    "{} already exists and is not empty",
                    folder.display()
                )));
            }
        }
        std::fs::create_dir_all(&folder)?;
        let mut entry = serde_json::json!({
            "id": plugin,
            "name": theme.name,
            "css": "theme.css",
            "params": exported.params,
        });
        if !exported.preview.is_null()
            && let Some(map) = entry.as_object_mut()
        {
            map.insert("preview".to_string(), exported.preview.clone());
        }
        let manifest = serde_json::json!({
            "id": plugin,
            "name": theme.name,
            "version": "1.0.0",
            "apiVersion": 1,
            "themes": [entry],
        });
        std::fs::write(folder.join("theme.css"), &exported.css)?;
        std::fs::write(
            folder.join("plugin.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        Ok(folder)
    }
}

/// Reads and validates one theme file; its id must be its file name.
fn read(path: &Path) -> Result<UserTheme> {
    let theme: UserTheme = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    validate(&theme)?;
    if path.file_stem().and_then(|s| s.to_str()) != Some(theme.id.as_str()) {
        return Err(invalid(format!(
            "The theme id '{}' does not match its file name",
            theme.id
        )));
    }
    Ok(theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> (tempfile::TempDir, UserThemes) {
        let tmp = tempfile::tempdir().unwrap();
        let themes = UserThemes::new(&DataPaths::new(tmp.path().join("data")));
        (tmp, themes)
    }

    fn theme(id: &str, name: &str) -> UserTheme {
        UserTheme {
            id: id.into(),
            name: name.into(),
            base: "questkeep".into(),
            overrides: ThemeOverrides {
                both: BTreeMap::from([("--qk-radius-scale".to_string(), "2".to_string())]),
                light: BTreeMap::from([("--qk-color-bg".to_string(), "#ffffff".to_string())]),
                dark: BTreeMap::from([(
                    "--qk-color-bg".to_string(),
                    "color-mix(in oklab, #000 80%, red)".to_string(),
                )]),
            },
            css: ".btn { text-transform: uppercase; }".into(),
        }
    }

    fn exported() -> ExportedTheme {
        ExportedTheme {
            css: ":root[data-theme=\"neon-theme\"] { --qk-color-bg: #000; }".into(),
            params: serde_json::json!([
                { "key": "accent", "label": "Accent color", "type": "color", "default": "#ff00aa" }
            ]),
            preview: serde_json::json!({
                "light": ["#fff", "#eee", "#000", "#f0a"],
                "dark": ["#000", "#111", "#fff", "#f0a"],
            }),
        }
    }

    #[test]
    fn saves_lists_and_deletes() {
        let (_tmp, themes) = themes();
        assert!(themes.list().unwrap().is_empty());
        themes.save(&theme("user-zeta", "  zeta  ")).unwrap();
        themes.save(&theme("user-neon", "Neon")).unwrap();
        let listed = themes.list().unwrap();
        assert_eq!(
            listed.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["Neon", "zeta"]
        );
        assert_eq!(listed[0], theme("user-neon", "Neon"));
        assert_eq!(themes.get("user-zeta").unwrap().name, "zeta");

        // Saving again replaces; no temp file is left behind.
        let mut changed = theme("user-neon", "Neon lights");
        changed.overrides.both.clear();
        themes.save(&changed).unwrap();
        assert_eq!(themes.get("user-neon").unwrap(), changed);
        let files: Vec<_> = std::fs::read_dir(&themes.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(files.len(), 2, "{files:?}");

        themes.delete("user-neon").unwrap();
        themes.delete("user-neon").unwrap();
        assert_eq!(themes.list().unwrap().len(), 1);
        assert!(themes.get("user-neon").is_err());
    }

    #[test]
    fn rejects_bad_ids_names_variables_and_values() {
        let (_tmp, themes) = themes();
        for id in [
            "neon",
            "user-",
            "user-Neon",
            "user-a b",
            "user-../x",
            "user-ä",
            &format!("user-{}", "a".repeat(49)),
        ] {
            assert!(themes.save(&theme(id, "Neon")).is_err(), "{id}");
            assert!(themes.delete(id).is_err(), "{id}");
            assert!(themes.get(id).is_err(), "{id}");
        }
        themes
            .save(&theme(&format!("user-{}", "a".repeat(48)), "Neon"))
            .unwrap();
        assert!(themes.save(&theme("user-neon", "   ")).is_err());
        assert!(themes.save(&theme("user-neon", &"n".repeat(81))).is_err());

        let with = |name: &str, value: &str| {
            let mut t = theme("user-neon", "Neon");
            t.overrides.dark = BTreeMap::from([(name.to_string(), value.to_string())]);
            t
        };
        for name in [
            "color",
            "--color-bg",
            "--qk-",
            "--qk-Color",
            "--qk-a b",
            "--qk-a:b",
        ] {
            assert!(themes.save(&with(name, "#fff")).is_err(), "{name}");
        }
        for value in [
            "",
            "  ",
            "red; } body { display: none",
            "red }",
            "{",
            "url(x)</style>",
            "a\nb",
            &"x".repeat(513),
        ] {
            assert!(
                themes.save(&with("--qk-color-bg", value)).is_err(),
                "{value}"
            );
        }
        themes
            .save(&with("--qk-color-bg", &"x".repeat(512)))
            .unwrap();

        let mut big = theme("user-neon", "Neon");
        big.css = "a".repeat(256 * 1024 + 1);
        assert!(themes.save(&big).is_err());
        big.css = "a".repeat(256 * 1024);
        themes.save(&big).unwrap();

        let mut base = theme("user-neon", "Neon");
        base.base = String::new();
        assert!(themes.save(&base).is_err());
        base.base = "a\"] body".into();
        assert!(themes.save(&base).is_err());
    }

    #[test]
    fn list_skips_broken_and_misnamed_files() {
        let (_tmp, themes) = themes();
        themes.save(&theme("user-good", "Good")).unwrap();
        std::fs::write(themes.dir.join("user-broken.json"), "{not json").unwrap();
        std::fs::write(themes.dir.join("notes.txt"), "hello").unwrap();
        // A valid theme under another file name, and one with a value that would not be saved.
        std::fs::write(
            themes.dir.join("user-other.json"),
            serde_json::to_string(&theme("user-good", "Copy")).unwrap(),
        )
        .unwrap();
        std::fs::write(
            themes.dir.join("user-evil.json"),
            r##"{"id":"user-evil","name":"Evil","base":"questkeep","overrides":{"both":{"--qk-color-bg":"red; } * { display: none"}}}"##,
        )
        .unwrap();
        let listed = themes.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "user-good");
        // Missing fields of an older or hand-written file take defaults.
        std::fs::write(
            themes.dir.join("user-plain.json"),
            r#"{"id":"user-plain","name":"Plain","base":"fjord"}"#,
        )
        .unwrap();
        let plain = themes.get("user-plain").unwrap();
        assert_eq!(plain.overrides, ThemeOverrides::default());
        assert_eq!(plain.css, "");
    }

    #[test]
    fn exports_a_plugin_folder_in_the_plugin_format() {
        let (tmp, themes) = themes();
        themes.save(&theme("user-neon", "Neon")).unwrap();
        let target = tmp.path().join("out");
        std::fs::create_dir_all(&target).unwrap();

        let folder = themes
            .export_plugin("user-neon", &target, &exported())
            .unwrap();
        assert_eq!(folder, target.join("neon-theme"));
        assert_eq!(
            std::fs::read_to_string(folder.join("theme.css")).unwrap(),
            exported().css
        );
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(folder.join("plugin.json")).unwrap())
                .unwrap();
        assert_eq!(
            manifest,
            serde_json::json!({
                "id": "neon-theme",
                "name": "Neon",
                "version": "1.0.0",
                "apiVersion": 1,
                "themes": [{
                    "id": "neon-theme",
                    "name": "Neon",
                    "css": "theme.css",
                    "params": exported().params,
                    "preview": exported().preview,
                }],
            })
        );

        // Not into a folder that has something in it; an empty one is fine.
        assert!(
            themes
                .export_plugin("user-neon", &target, &exported())
                .is_err()
        );
        std::fs::remove_file(folder.join("plugin.json")).unwrap();
        std::fs::remove_file(folder.join("theme.css")).unwrap();
        let without_preview = ExportedTheme {
            preview: serde_json::Value::Null,
            ..exported()
        };
        themes
            .export_plugin("user-neon", &target, &without_preview)
            .unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(folder.join("plugin.json")).unwrap())
                .unwrap();
        assert!(manifest["themes"][0].get("preview").is_none());

        // Unknown themes, missing target folders and odd parameters are errors.
        assert!(
            themes
                .export_plugin("user-gone", &target, &exported())
                .is_err()
        );
        assert!(
            themes
                .export_plugin("user-neon", &tmp.path().join("missing"), &exported())
                .is_err()
        );
        let odd = ExportedTheme {
            params: serde_json::json!({}),
            ..exported()
        };
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(themes.export_plugin("user-neon", &target, &odd).is_err());
        assert!(!folder.exists());
        assert_eq!(plugin_id("user-my-theme"), "my-theme-theme");
    }
}
