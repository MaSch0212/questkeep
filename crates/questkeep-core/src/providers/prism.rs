//! Prism Launcher (Minecraft): every instance is a game.
//!
//! Prism keeps its settings in `prismlauncher.cfg` in its data folder (`%APPDATA%\PrismLauncher`,
//! `~/.local/share/PrismLauncher`, the Flatpak folder, or the program folder of a portable
//! install). `InstanceDir` and `IconsDir` there may move the instances and icons anywhere
//! (relative paths are relative to the data folder). An instance is a folder with an
//! `instance.cfg` (`[General]`: `name`, `iconKey`, `totalTimePlayed` in seconds, `lastLaunchTime`
//! in milliseconds); its id is the folder name, which `prismlauncher --launch <id>` takes.
//!
//! Prism has no command-line option for its "Add Instance" dialog, so the add-game option opens
//! Prism and presses the dialog's shortcut (Ctrl+N) once Prism's main window is in front.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::DateTime;

use super::{AddGameOption, ImageCandidate, LibraryProvider, ProviderError};
use crate::model::{ImportedGame, PlayAction, TrackingMode};

pub const PRISM_SOURCE: &str = "prism";
const NEW_INSTANCE: &str = "new-instance";
/// How long to wait for Prism's window before giving up on pressing Ctrl+N.
const WINDOW_TIMEOUT: Duration = Duration::from_secs(20);
/// Image types the icon pipeline can decode, in Prism's order of preference.
const ICON_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "ico"];

/// How Prism is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrismCommand {
    pub program: PathBuf,
    /// Arguments before Prism's own (`run org.prismlauncher.PrismLauncher` for Flatpak).
    pub prefix: Vec<String>,
}

/// Where Prism lives on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrismPaths {
    pub data_dir: Option<PathBuf>,
    pub command: Option<PrismCommand>,
}

impl PrismPaths {
    /// Looks at the usual places for this OS.
    pub fn detect() -> Self {
        let mut command = None;
        let mut data_dir = None;
        #[cfg(windows)]
        {
            let exe = crate::platform::uninstall_entries()
                .into_iter()
                .find(|e| {
                    e.key == "PrismLauncher" || e.display_name.as_deref() == Some("Prism Launcher")
                })
                .and_then(|e| e.install_location)
                .map(|dir| PathBuf::from(dir).join("prismlauncher.exe"))
                .filter(|exe| exe.is_file())
                .or_else(|| {
                    let exe =
                        dirs::data_local_dir()?.join(r"Programs\PrismLauncher\prismlauncher.exe");
                    exe.is_file().then_some(exe)
                });
            if let Some(exe) = exe {
                // A portable install keeps its data next to the program.
                if let Some(dir) = exe.parent().filter(|d| d.join("portable.txt").is_file()) {
                    data_dir = Some(dir.to_path_buf());
                }
                command = Some(PrismCommand {
                    program: exe,
                    prefix: Vec::new(),
                });
            }
            data_dir = data_dir.or_else(|| dirs::data_dir().map(|d| d.join("PrismLauncher")));
        }
        #[cfg(not(windows))]
        {
            let home = dirs::home_dir();
            let flatpak = home
                .as_ref()
                .map(|h| h.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher"));
            let native = dirs::data_dir().map(|d| d.join("PrismLauncher"));
            let on_path = std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join("prismlauncher"))
                    .find(|p| p.is_file())
            });
            #[cfg(target_os = "macos")]
            let on_path = on_path.or_else(|| {
                let app =
                    PathBuf::from("/Applications/Prism Launcher.app/Contents/MacOS/prismlauncher");
                app.is_file().then_some(app)
            });
            if let Some(program) = on_path {
                command = Some(PrismCommand {
                    program,
                    prefix: Vec::new(),
                });
                data_dir = native.filter(|d| d.is_dir());
            } else if let Some(dir) = flatpak.filter(|d| d.is_dir()) {
                command = Some(PrismCommand {
                    program: PathBuf::from("flatpak"),
                    prefix: vec!["run".into(), "org.prismlauncher.PrismLauncher".into()],
                });
                data_dir = Some(dir);
            } else {
                data_dir = native.filter(|d| d.is_dir());
            }
        }
        Self {
            data_dir: data_dir.filter(|d| d.is_dir()),
            command,
        }
    }
}

/// One value of Prism's INI files: optional surrounding quotes and backslash escapes.
fn ini_value(raw: &str) -> String {
    let raw = raw.trim();
    let inner = raw
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The keys of the `[General]` section (or of a file without sections).
pub fn parse_ini(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut section = String::from("General");
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            continue;
        }
        if section != "General" {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            out.insert(key.trim().to_string(), ini_value(value));
        }
    }
    out
}

/// One instance folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// The folder name.
    pub id: String,
    pub dir: PathBuf,
    pub name: String,
    pub icon_key: Option<String>,
    pub total_seconds: Option<u64>,
    pub last_launch_ms: Option<i64>,
}

pub fn read_instance(dir: &Path) -> Option<Instance> {
    let id = dir.file_name()?.to_str()?.to_string();
    if id.starts_with(['.', '_']) {
        return None;
    }
    let cfg = parse_ini(&std::fs::read_to_string(dir.join("instance.cfg")).ok()?);
    let text = |key: &str| {
        cfg.get(key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Some(Instance {
        name: text("name").unwrap_or_else(|| id.clone()),
        icon_key: text("iconKey").filter(|k| k != "default"),
        total_seconds: text("totalTimePlayed").and_then(|v| v.parse().ok()),
        last_launch_ms: text("lastLaunchTime")
            .and_then(|v| v.parse().ok())
            .filter(|&ms| ms > 0),
        dir: dir.to_path_buf(),
        id,
    })
}

#[derive(Debug, Clone, Default)]
pub struct PrismProvider {
    paths: PrismPaths,
}

impl PrismProvider {
    pub fn detect() -> Self {
        Self::with_paths(PrismPaths::detect())
    }

    pub fn with_paths(paths: PrismPaths) -> Self {
        Self { paths }
    }

    fn settings(&self) -> HashMap<String, String> {
        self.paths
            .data_dir
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.join("prismlauncher.cfg")).ok())
            .map(|text| parse_ini(&text))
            .unwrap_or_default()
    }

    /// A folder setting of `prismlauncher.cfg`, relative to the data folder unless absolute.
    fn folder(
        &self,
        settings: &HashMap<String, String>,
        key: &str,
        default: &str,
    ) -> Option<PathBuf> {
        let data = self.paths.data_dir.as_ref()?;
        let value = settings
            .get(key)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .unwrap_or(default);
        let path = PathBuf::from(value);
        Some(if path.is_absolute() {
            path
        } else {
            data.join(path)
        })
    }

    fn instances(&self) -> Result<Vec<Instance>, ProviderError> {
        let settings = self.settings();
        let Some(root) = self.folder(&settings, "InstanceDir", "instances") else {
            return Err(ProviderError::NotFound(
                "Prism Launcher was not found".into(),
            ));
        };
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(ProviderError::NotFound(format!(
                    "Could not read the Prism Launcher instances in {}: {err}",
                    root.display()
                )));
            }
        };
        let mut instances: Vec<Instance> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| read_instance(&e.path()))
            .collect();
        instances.sort_by_cached_key(|i| (i.name.to_lowercase(), i.id.clone()));
        Ok(instances)
    }

    fn play_action(&self, instance: &Instance) -> Option<PlayAction> {
        let command = self.paths.command.as_ref()?;
        let mut args: Vec<String> = command.prefix.clone();
        args.push("--launch".into());
        args.push(instance.id.clone());
        let mut action = PlayAction::file(command.program.to_string_lossy());
        action.args = Some(
            args.iter()
                .map(|a| {
                    if a.contains([' ', '"']) {
                        format!("\"{}\"", a.replace('"', "\\\""))
                    } else {
                        a.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        );
        // Prism hands the launch to its running window and exits, and Minecraft runs in Java,
        // so the session is the Java process (Prism also records the playtime itself).
        action.tracking = TrackingMode::Name;
        action.tracking_path = Some(if cfg!(windows) { "javaw.exe" } else { "java" }.into());
        Some(action)
    }
}

impl LibraryProvider for PrismProvider {
    fn id(&self) -> &str {
        PRISM_SOURCE
    }

    fn name(&self) -> &str {
        "Prism Launcher"
    }

    fn icon_svg(&self) -> &'static str {
        crate::providers::icons::PRISM
    }

    fn can_scan(&self) -> bool {
        self.paths.data_dir.is_some()
    }

    /// Starts Prism itself (with the Flatpak prefix, if any), when the program was found.
    fn launcher(&self) -> Option<PlayAction> {
        let command = self.paths.command.as_ref()?;
        let mut action = PlayAction::file(command.program.to_string_lossy());
        let args = command
            .prefix
            .iter()
            .map(|a| {
                if a.contains([' ', '"']) {
                    format!("\"{}\"", a.replace('"', "\\\""))
                } else {
                    a.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        action.args = Some(args).filter(|a| !a.is_empty());
        Some(action)
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        Ok(self
            .instances()?
            .into_iter()
            .map(|instance| ImportedGame {
                install_dir: Some(instance.dir.to_string_lossy().into_owned()),
                play_action: self.play_action(&instance),
                playtime_seconds: instance.total_seconds,
                last_played: instance
                    .last_launch_ms
                    .and_then(DateTime::from_timestamp_millis),
                ..ImportedGame::installed(instance.id.clone(), instance.name.clone())
            })
            .collect())
    }

    /// The instance's own icon: the file `iconKey` names in the icons folder, else an icon file
    /// in the instance folder. Prism's built-in icons live inside the program, not on disk.
    fn icon_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        let settings = self.settings();
        let Some(root) = self.folder(&settings, "InstanceDir", "instances") else {
            return Vec::new();
        };
        let Some(instance) = read_instance(&root.join(source_game_id)) else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        if let (Some(key), Some(icons)) = (
            &instance.icon_key,
            self.folder(&settings, "IconsDir", "icons"),
        ) {
            candidates.extend(
                ICON_EXTENSIONS
                    .iter()
                    .map(|ext| icons.join(format!("{key}.{ext}")))
                    .filter(|p| p.is_file()),
            );
        }
        candidates.extend(
            ICON_EXTENSIONS
                .iter()
                .map(|ext| instance.dir.join(format!("icon.{ext}")))
                .filter(|p| p.is_file()),
        );
        candidates.into_iter().map(ImageCandidate::File).collect()
    }

    fn add_game_options(&self) -> Vec<AddGameOption> {
        if self.paths.command.is_none() {
            return Vec::new();
        }
        vec![AddGameOption {
            id: NEW_INSTANCE.into(),
            label: "New Minecraft instance".into(),
        }]
    }

    fn run_add_game_option(&self, option_id: &str) -> Result<(), ProviderError> {
        let Some(command) = self
            .paths
            .command
            .as_ref()
            .filter(|_| option_id == NEW_INSTANCE)
        else {
            return Err(ProviderError::Unsupported);
        };
        let mut args: Vec<&str> = command.prefix.iter().map(String::as_str).collect();
        args.push("--show-window");
        crate::platform::start_in_front(&command.program, &args)
            .map_err(|e| ProviderError::NotFound(format!("Could not start Prism Launcher: {e}")))?;
        // Ctrl+N is Prism's "Add Instance" shortcut; only pressed while its main window (title
        // "Prism Launcher <version>") is in front.
        std::thread::spawn(|| {
            if !crate::platform::press_ctrl_key_in(
                "prismlauncher.exe",
                "Prism Launcher",
                b'N',
                WINDOW_TIMEOUT,
            ) {
                tracing::info!(
                    "Prism Launcher's window did not come to the front; not opening Add Instance"
                );
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn setup(instances_setting: Option<&str>) -> (tempfile::TempDir, PrismProvider, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("PrismLauncher");
        let instances = match instances_setting {
            Some(dir) => PathBuf::from(dir),
            None => data.join("instances"),
        };
        let cfg = match instances_setting {
            Some(dir) => {
                format!("[General]\nConfigVersion=1.3\nInstanceDir={dir}\nIconsDir=icons2\n")
            }
            None => "[General]\nConfigVersion=1.3\n".into(),
        };
        write(&data.join("prismlauncher.cfg"), &cfg);
        let provider = PrismProvider::with_paths(PrismPaths {
            data_dir: Some(data),
            command: Some(PrismCommand {
                program: PathBuf::from("prismlauncher.exe"),
                prefix: Vec::new(),
            }),
        });
        (tmp, provider, instances)
    }

    #[test]
    fn reads_ini_values() {
        let cfg =
            parse_ini("[General]\nname=\"FTB \\\"StoneBlock\\\" 4\"\nempty=\n[UI]\nname=ignored\n");
        assert_eq!(cfg["name"], "FTB \"StoneBlock\" 4");
        assert_eq!(cfg["empty"], "");
        assert_eq!(parse_ini("a=b")["a"], "b");
    }

    #[test]
    fn scans_instances_with_playtime_and_launch_command() {
        let (_tmp, provider, instances) = setup(None);
        write(
            &instances.join("FTB StoneBlock 4/instance.cfg"),
            "[General]\nname=FTB StoneBlock 4\niconKey=curseforge_ftb\nlastLaunchTime=1768417817735\ntotalTimePlayed=364124\nInstanceType=OneSix\n[UI]\nx=1\n",
        );
        write(
            &instances.join("bare/instance.cfg"),
            "[General]\nInstanceType=OneSix\n",
        );
        write(
            &instances.join(".hidden/instance.cfg"),
            "[General]\nname=Hidden\n",
        );
        fs::create_dir_all(instances.join("not an instance")).unwrap();
        write(&instances.join("instgroups.json"), "{}");

        let games = provider.scan_installed().unwrap();
        let ids: Vec<_> = games
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str()))
            .collect();
        assert_eq!(
            ids,
            [("bare", "bare"), ("FTB StoneBlock 4", "FTB StoneBlock 4")]
        );
        let ftb = &games[1];
        assert!(ftb.installed);
        assert_eq!(ftb.playtime_seconds, Some(364_124));
        assert_eq!(
            ftb.last_played.unwrap().timestamp_millis(),
            1_768_417_817_735
        );
        let action = ftb.play_action.as_ref().unwrap();
        assert_eq!(action.target, "prismlauncher.exe");
        assert_eq!(
            action.args.as_deref(),
            Some("--launch \"FTB StoneBlock 4\"")
        );
        assert_eq!(action.tracking, TrackingMode::Name);
        assert_eq!(games[0].playtime_seconds, None);
    }

    #[test]
    fn honours_moved_instance_and_icon_folders() {
        let tmp_instances = tempfile::tempdir().unwrap();
        let dir = tmp_instances.path().to_string_lossy().replace('\\', "/");
        let (_tmp, provider, instances) = setup(Some(&dir));
        write(
            &instances.join("ATM10/instance.cfg"),
            "[General]\nname=All the Mods 10\niconKey=curseforge_1\n",
        );
        write(
            &instances.join("Awd/instance.cfg"),
            "[General]\nname=Awd Pack\n",
        );
        write(&instances.join("Awd/icon.ico"), "ico");
        let data = provider.paths.data_dir.clone().unwrap();
        write(&data.join("icons2/curseforge_1.jpg"), "jpg");

        assert_eq!(provider.scan_installed().unwrap().len(), 2);
        assert_eq!(
            provider.icon_candidates("ATM10"),
            [ImageCandidate::File(data.join("icons2/curseforge_1.jpg"))]
        );
        assert_eq!(
            provider.icon_candidates("Awd"),
            [ImageCandidate::File(instances.join("Awd/icon.ico"))]
        );
        assert!(provider.icon_candidates("missing").is_empty());
    }

    #[test]
    fn launcher_starts_the_program_with_its_prefix() {
        let (_tmp, provider, _) = setup(None);
        let action = provider.launcher().unwrap();
        assert_eq!(action.target, "prismlauncher.exe");
        assert_eq!(action.args, None);
        let flatpak = PrismProvider::with_paths(PrismPaths {
            data_dir: None,
            command: Some(PrismCommand {
                program: PathBuf::from("flatpak"),
                prefix: vec!["run".into(), "org.prismlauncher.PrismLauncher".into()],
            }),
        });
        assert_eq!(
            flatpak.launcher().unwrap().args.as_deref(),
            Some("run org.prismlauncher.PrismLauncher")
        );
        assert!(PrismProvider::default().launcher().is_none());
    }

    #[test]
    fn offers_a_new_instance_option_only_with_the_program() {
        let (_tmp, provider, _) = setup(None);
        assert_eq!(provider.add_game_options().len(), 1);
        assert!(matches!(
            provider.run_add_game_option("other"),
            Err(ProviderError::Unsupported)
        ));
        let no_program = PrismProvider::with_paths(PrismPaths {
            data_dir: provider.paths.data_dir.clone(),
            command: None,
        });
        assert!(no_program.add_game_options().is_empty());
        assert!(PrismProvider::default().scan_installed().is_err());
    }
}
