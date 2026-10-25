//! EA app provider, Windows only.
//!
//! Games installed by the EA app (or Origin before it) register an entry in the Windows "Apps"
//! list whose uninstaller is EA's `Cleanup.exe` under `EAInstaller`. Their install folder holds
//! `__Installer\installerdata.xml`, the game's manifest: its content ids (the first is the game's
//! id here), its titles per locale, and its launchers (`<runtime><launcher>`), whose `filePath`
//! usually starts with a registry value in brackets, e.g.
//! `[HKEY_LOCAL_MACHINE\SOFTWARE\BioWare\Dragon Age Inquisition\Install Dir]DragonAgeInquisition.exe`.
//! Trial launchers are skipped.
//!
//! Games start through their executable, which may hand over to the EA app first, so sessions
//! are tracked by install folder. Uninstalling runs the entry's `Cleanup.exe` command, which
//! asks for confirmation. The EA app's own database is encrypted, so owned games that are not
//! installed are not available.

use std::path::{Path, PathBuf};

use super::{LibraryProvider, ProviderError};
use crate::model::{ImportedGame, PlayAction, TrackingMode};
use crate::platform::UninstallEntry;

pub const EA_SOURCE: &str = "ea";

/// What `installerdata.xml` says about a game.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub content_ids: Vec<String>,
    /// The `en_US` title, else the first one.
    pub title: Option<String>,
    /// `filePath` of the launcher to start (not a trial; 64-bit preferred).
    pub launcher_path: Option<String>,
}

/// The text of every `<tag ...>text</tag>` in `xml`, with the opening tag's attributes.
fn elements<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        // `<launcher` must not match `<launcherX`.
        if !after.starts_with(['>', ' ', '\t', '\r', '\n']) {
            rest = after;
            continue;
        }
        let Some(head_end) = after.find('>') else {
            break;
        };
        let attrs = &after[..head_end];
        let body = &after[head_end + 1..];
        if attrs.ends_with('/') {
            out.push((attrs, ""));
            rest = body;
            continue;
        }
        let Some(end) = body.find(&close) else {
            break;
        };
        out.push((attrs, &body[..end]));
        rest = &body[end + close.len()..];
    }
    out
}

fn first_text(xml: &str, tag: &str) -> Option<String> {
    elements(xml, tag)
        .into_iter()
        .map(|(_, text)| unescape(text.trim()))
        .find(|t| !t.is_empty())
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Strips trademark signs and surplus whitespace from a title.
fn clean_name(name: &str) -> String {
    name.replace(['\u{2122}', '\u{00ae}', '\u{00a9}'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_manifest(xml: &str) -> Manifest {
    let content_ids = elements(xml, "contentIDs")
        .first()
        .map(|(_, ids)| {
            elements(ids, "contentID")
                .into_iter()
                .map(|(_, id)| id.trim().to_string())
                .filter(|id| !id.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let titles = elements(xml, "gameTitles")
        .first()
        .map(|(_, titles)| elements(titles, "gameTitle"))
        .unwrap_or_default();
    let title = titles
        .iter()
        .find(|(attrs, _)| attrs.contains("\"en_US\""))
        .or_else(|| titles.first())
        .map(|(_, t)| clean_name(&unescape(t)))
        .filter(|t| !t.is_empty());
    let launchers: Vec<_> = elements(xml, "runtime")
        .first()
        .map(|(_, runtime)| elements(runtime, "launcher"))
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, l)| first_text(l, "trial").as_deref() != Some("1"))
        .collect();
    let launcher_path = launchers
        .iter()
        .find(|(_, l)| first_text(l, "requires64BitOS").as_deref() == Some("1"))
        .or_else(|| launchers.first())
        .and_then(|(_, l)| first_text(l, "filePath"));
    Manifest {
        content_ids,
        title,
        launcher_path,
    }
}

/// Resolves a launcher `filePath`: a leading `[registry value]` is replaced by that value (an
/// install folder), anything else is relative to `install_dir`.
fn resolve_launcher(
    path: &str,
    install_dir: &Path,
    registry: &dyn Fn(&str) -> Option<String>,
) -> PathBuf {
    let (base, rest) = match path.strip_prefix('[').and_then(|p| p.split_once(']')) {
        Some((key, rest)) => (
            registry(key)
                .map(|v| PathBuf::from(v.trim()))
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| install_dir.to_path_buf()),
            rest,
        ),
        None => (install_dir.to_path_buf(), path),
    };
    base.join(rest.trim_start_matches(['\\', '/']).replace('/', "\\"))
}

/// Whether an "Apps" entry belongs to the EA app (or Origin).
fn is_ea_entry(entry: &UninstallEntry) -> bool {
    !entry.key.starts_with("Steam App ")
        && entry
            .uninstall_string
            .as_deref()
            .is_some_and(|u| u.to_ascii_lowercase().contains("eainstaller"))
}

/// One installed game, read from its entry and manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Install {
    id: String,
    name: String,
    dir: PathBuf,
    exe: Option<PathBuf>,
    uninstall: Option<String>,
}

fn read_install(
    entry: &UninstallEntry,
    registry: &dyn Fn(&str) -> Option<String>,
) -> Option<Install> {
    let dir = PathBuf::from(
        entry
            .install_location
            .as_deref()?
            .trim_end_matches(['\\', '/']),
    );
    let xml = std::fs::read(dir.join("__Installer").join("installerdata.xml")).ok()?;
    let manifest = parse_manifest(&String::from_utf8_lossy(&xml));
    let id = manifest.content_ids.first()?.clone();
    let name = manifest
        .title
        .or_else(|| entry.display_name.as_deref().map(clean_name))
        .unwrap_or_else(|| id.clone());
    let exe = manifest
        .launcher_path
        .map(|p| resolve_launcher(&p, &dir, registry));
    Some(Install {
        id,
        name,
        exe,
        uninstall: entry.uninstall_string.clone(),
        dir,
    })
}

/// Where the "Apps" entries and registry values come from.
enum System {
    Live,
    /// Fixed entries and no registry, for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(Vec<UninstallEntry>),
}

pub struct EaProvider {
    system: System,
}

impl EaProvider {
    pub fn detect() -> Self {
        Self {
            system: System::Live,
        }
    }

    /// Uses fixed "Apps" entries (tests and tools).
    pub fn with_entries(entries: Vec<UninstallEntry>) -> Self {
        Self {
            system: System::Fixed(entries),
        }
    }

    fn installs(&self) -> Vec<Install> {
        fn no_registry(_: &str) -> Option<String> {
            None
        }
        type Lookup = fn(&str) -> Option<String>;
        let (entries, registry): (Vec<UninstallEntry>, Lookup) = match &self.system {
            System::Live => (
                crate::platform::uninstall_entries(),
                crate::platform::registry_value,
            ),
            System::Fixed(entries) => (entries.clone(), no_registry),
        };
        let mut installs: Vec<Install> = Vec::new();
        for install in entries
            .iter()
            .filter(|e| is_ea_entry(e))
            .filter_map(|e| read_install(e, &registry))
        {
            // The 64- and 32-bit views may list the same game twice.
            if !installs.iter().any(|i| i.id == install.id) {
                installs.push(install);
            }
        }
        installs
    }
}

impl LibraryProvider for EaProvider {
    fn id(&self) -> &str {
        EA_SOURCE
    }

    fn name(&self) -> &str {
        "EA app"
    }

    fn icon_svg(&self) -> &'static str {
        crate::providers::icons::EA
    }

    fn can_scan(&self) -> bool {
        cfg!(windows) || matches!(self.system, System::Fixed(_))
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if !self.can_scan() {
            return Err(ProviderError::NotFound(
                "The EA app is not installed".into(),
            ));
        }
        let mut games: Vec<ImportedGame> = self
            .installs()
            .into_iter()
            .map(|install| {
                let dir = install.dir.to_string_lossy().into_owned();
                let play_action = install.exe.map(|exe| {
                    let mut action = PlayAction::file(exe.to_string_lossy());
                    action.working_dir = exe.parent().map(|p| p.to_string_lossy().into_owned());
                    // The executable may restart through the EA app; follow the folder instead.
                    action.tracking = TrackingMode::Directory;
                    action.tracking_path = Some(dir.clone());
                    action
                });
                ImportedGame {
                    install_dir: Some(dir),
                    play_action,
                    ..ImportedGame::installed(install.id, install.name)
                }
            })
            .collect();
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    fn can_uninstall(&self) -> bool {
        self.can_scan()
    }

    /// The game's `Cleanup.exe uninstall_game` command, which asks for confirmation.
    fn uninstall_action(&self, source_game_id: &str) -> Option<PlayAction> {
        let install = self
            .installs()
            .into_iter()
            .find(|i| i.id == source_game_id)?;
        let (program, args) = crate::platform::split_command_line(install.uninstall.as_deref()?)?;
        let mut action = PlayAction::file(program);
        action.args = Some(args).filter(|a| !a.is_empty());
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAI: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<DiPManifest version="4.0">
  <buildMetaData><requirements osMinVersion="6.0" osReqs64Bit="true"/></buildMetaData>
  <contentIDs>
    <contentID>1000659</contentID>
    <contentID>1023462</contentID>
  </contentIDs>
  <gameTitles>
    <gameTitle locale="de_DE">Dragon Age™: Inquisition DE</gameTitle>
    <gameTitle locale="en_US">Dragon Age™: Inquisition &amp; More</gameTitle>
  </gameTitles>
  <runtime>
    <launcher uid="1-1">
      <name locale="en_US">Dragon Age™: Inquisition (x64)</name>
      <filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\BioWare\Dragon Age Inquisition\Install Dir]DragonAgeInquisition.exe</filePath>
      <parameters></parameters>
      <requires64BitOS>1</requires64BitOS>
    </launcher>
  </runtime>
  <touchup><filePath>/__Installer/touchup.exe</filePath></touchup>
</DiPManifest>"#;

    const MEA: &str = r#"<DiPManifest>
<contentIDs><contentID>1019025</contentID></contentIDs>
<gameTitles><gameTitle locale="en_US">Mass Effect™: Andromeda</gameTitle></gameTitles>
<runtime>
<launcher uid="1-2"><filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\BioWare\Mass Effect Andromeda\Install Dir]MassEffectAndromedaTrial.exe</filePath><requires64BitOS>1</requires64BitOS><trial>1</trial></launcher>
<launcher uid="2-2"><filePath>Game/MassEffectAndromeda.exe</filePath><requires64BitOS>1</requires64BitOS><trial>0</trial></launcher>
</runtime>
</DiPManifest>"#;

    #[test]
    fn parses_manifests() {
        let dai = parse_manifest(DAI);
        assert_eq!(dai.content_ids, ["1000659", "1023462"]);
        assert_eq!(dai.title.as_deref(), Some("Dragon Age: Inquisition & More"));
        assert_eq!(
            dai.launcher_path.as_deref(),
            Some(
                r"[HKEY_LOCAL_MACHINE\SOFTWARE\BioWare\Dragon Age Inquisition\Install Dir]DragonAgeInquisition.exe"
            )
        );
        let mea = parse_manifest(MEA);
        assert_eq!(
            mea.launcher_path.as_deref(),
            Some("Game/MassEffectAndromeda.exe")
        );
        assert_eq!(parse_manifest("<x/>"), Manifest::default());
    }

    // Install paths use Windows separators.
    #[cfg(windows)]
    #[test]
    fn resolves_launcher_paths() {
        let dir = Path::new(r"F:\EA Games\DAI");
        let registry = |key: &str| {
            (key == r"HKEY_LOCAL_MACHINE\SOFTWARE\BioWare\DAI\Install Dir")
                .then(|| r"D:\Games\DAI\".to_string())
        };
        assert_eq!(
            resolve_launcher(
                r"[HKEY_LOCAL_MACHINE\SOFTWARE\BioWare\DAI\Install Dir]DAI.exe",
                dir,
                &registry
            ),
            Path::new(r"D:\Games\DAI\DAI.exe")
        );
        assert_eq!(
            resolve_launcher(
                r"[HKEY_LOCAL_MACHINE\Missing\Install Dir]DAI.exe",
                dir,
                &registry
            ),
            dir.join("DAI.exe")
        );
        assert_eq!(
            resolve_launcher("/bin/x.exe", dir, &registry),
            dir.join(r"bin\x.exe")
        );
    }

    #[cfg(windows)]
    fn entry(key: &str, dir: &Path, uninstall: &str) -> UninstallEntry {
        UninstallEntry {
            key: key.into(),
            display_name: Some("Dragon Age T: Inquisition".into()),
            publisher: Some("Electronic Arts".into()),
            install_location: Some(format!("{}\\", dir.display())),
            uninstall_string: Some(uninstall.into()),
            display_icon: None,
        }
    }

    #[cfg(windows)]
    #[test]
    fn scans_ea_installs_and_uninstalls_through_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let write = |name: &str, xml: &str| {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(dir.join("__Installer")).unwrap();
            std::fs::write(dir.join("__Installer/installerdata.xml"), xml).unwrap();
            dir
        };
        let dai = write("DAI", DAI);
        let mea = write("MEA", MEA);
        let steam = write("SteamCopy", MEA);
        let cleanup = r#""C:\Program Files\Common Files\EAInstaller\DAI\Cleanup.exe" uninstall_game -autologging"#;
        let provider = EaProvider::with_entries(vec![
            entry("{DC4C36DC}", &dai, cleanup),
            entry("{DC4C36DC}", &dai, cleanup),
            entry(
                "{MEA}",
                &mea,
                r#""C:\x\EAInstaller\MEA\Cleanup.exe" uninstall_game"#,
            ),
            entry("Steam App 1238000", &steam, "steam://uninstall/1238000"),
            entry(
                "Other",
                tmp.path(),
                r#""C:\other\EAInstaller\x\Cleanup.exe""#,
            ),
        ]);

        let games = provider.scan_installed().unwrap();
        let ids: Vec<_> = games
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                ("1000659", "Dragon Age: Inquisition & More"),
                ("1019025", "Mass Effect: Andromeda")
            ]
        );
        let action = games[1].play_action.as_ref().unwrap();
        assert_eq!(
            Path::new(&action.target),
            mea.join(r"Game\MassEffectAndromeda.exe")
        );
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(action.tracking_path, games[1].install_dir);
        // Without the registry value, the BioWare path falls back to the install folder.
        assert_eq!(
            Path::new(&games[0].play_action.as_ref().unwrap().target),
            dai.join("DragonAgeInquisition.exe")
        );

        let uninstall = provider.uninstall_action("1000659").unwrap();
        assert_eq!(
            uninstall.target,
            r"C:\Program Files\Common Files\EAInstaller\DAI\Cleanup.exe"
        );
        assert_eq!(
            uninstall.args.as_deref(),
            Some("uninstall_game -autologging")
        );
        assert!(provider.uninstall_action("404").is_none());
    }
}
