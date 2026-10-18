//! Xbox / Microsoft Store provider (PC Game Pass and Store games), Windows only.
//!
//! Installed games are found from the current user's registered AppX/MSIX packages (registry, no
//! PowerShell and no Xbox app involved). A package is a game when its folder contains
//! `MicrosoftGame.config` (GDK titles installed to `XboxGames\<Game>\Content`). Legacy UWP games
//! below `C:\Program Files\WindowsApps` are not readable without elevation and are skipped.
//! No account login: the owned library is not available (see `can_scan_owned`).

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

mod manifest;
mod scan;

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::model::{ImportedGame, PlayAction};
use crate::platform::{self, AppxPackage};

pub const XBOX_SOURCE: &str = "xbox";

/// Where the provider gets packages from; replaceable in tests.
type PackageSource = fn() -> Vec<AppxPackage>;

#[derive(Debug, Clone)]
pub struct XboxProvider {
    packages: PackageSource,
}

impl XboxProvider {
    /// Reads the packages registered on this machine.
    pub fn detect() -> Self {
        Self::with_source(platform::appx_packages)
    }

    fn with_source(packages: PackageSource) -> Self {
        Self { packages }
    }

    fn entries(&self) -> Vec<scan::GameEntry> {
        scan::scan_packages(&(self.packages)())
    }
}

impl LibraryProvider for XboxProvider {
    fn id(&self) -> &str {
        XBOX_SOURCE
    }

    fn name(&self) -> &str {
        "Xbox"
    }

    fn can_scan(&self) -> bool {
        cfg!(windows)
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if !cfg!(windows) {
            return Err(ProviderError::NotFound(
                "Xbox games are only available on Windows.".into(),
            ));
        }
        Ok(self.entries().into_iter().map(|e| e.game).collect())
    }

    /// Always `false`: listing owned (not installed) games needs the Xbox Live / Store web APIs
    /// and an account login, which Questkeep does not do yet.
    fn can_scan_owned(&self) -> bool {
        false
    }

    /// Opens the game's Microsoft Store page, from where it can be installed; harmless for games
    /// that are already installed.
    fn install_action(&self, source_game_id: &str) -> Option<PlayAction> {
        let valid = !source_game_id.is_empty()
            && source_game_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        valid.then(|| PlayAction::uri(format!("ms-windows-store://pdp/?PFN={source_game_id}")))
    }

    /// The package's logo tile. These are square (150x150 up to 480x480), not 2:3 covers, so
    /// they are only a stopgap: real portrait art needs the Xbox Store catalog API (future).
    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        let Some(entry) = self
            .entries()
            .into_iter()
            .find(|e| e.game.source_game_id == source_game_id)
        else {
            return Vec::new();
        };
        let mut out: Vec<ImageCandidate> = Vec::new();
        for declared in &entry.logos {
            if let Some(file) = scan::resolve_logo(declared) {
                let candidate = ImageCandidate::File(file);
                if !out.contains(&candidate) {
                    out.push(candidate);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn fixture_packages() -> Vec<AppxPackage> {
        // Leaked temp dir path: fn pointers cannot capture, so tests use a fixed layout below.
        let root = std::env::temp_dir().join("questkeep-xbox-provider-test");
        let content = root.join("Content");
        let _ = fs::create_dir_all(content.join("Assets"));
        let _ = fs::write(
            content.join("MicrosoftGame.config"),
            r#"<Game><ShellVisuals DefaultDisplayName="Racer" Square150x150Logo="Assets\Logo.png"/></Game>"#,
        );
        let _ = fs::write(content.join("Assets").join("Logo.scale-100.png"), "x");
        let _ = fs::write(content.join("Assets").join("Logo.scale-200.png"), "x");
        vec![AppxPackage {
            full_name: "Contoso.Racer_1.0.0.0_x64__8wekyb3d8bbwe".into(),
            root: content,
            app_ids: vec!["Game".into()],
        }]
    }

    #[test]
    fn describes_itself() {
        let p = XboxProvider::with_source(Vec::new);
        assert_eq!(p.id(), "xbox");
        assert_eq!(p.name(), "Xbox");
        assert!(!p.can_scan_owned());
        assert_eq!(p.can_scan(), cfg!(windows));
    }

    #[test]
    fn install_action_opens_store_page() {
        let p = XboxProvider::with_source(Vec::new);
        let a = p.install_action("Contoso.Racer_8wekyb3d8bbwe").unwrap();
        assert_eq!(
            a.target,
            "ms-windows-store://pdp/?PFN=Contoso.Racer_8wekyb3d8bbwe"
        );
        assert!(p.install_action("").is_none());
        assert!(p.install_action("a&b=c").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn scans_and_finds_covers() {
        let p = XboxProvider::with_source(fixture_packages);
        let games = p.scan_installed().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Racer");
        let covers = p.cover_candidates("Contoso.Racer_8wekyb3d8bbwe");
        assert_eq!(covers.len(), 1);
        match &covers[0] {
            ImageCandidate::File(f) => assert!(f.ends_with("Logo.scale-200.png")),
            other => panic!("unexpected {other:?}"),
        }
        assert!(p.cover_candidates("Nope_abc").is_empty());
    }

    #[cfg(not(windows))]
    #[test]
    fn scan_is_unavailable_off_windows() {
        let p = XboxProvider::with_source(fixture_packages);
        assert!(matches!(
            p.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
    }

    /// Read-only look at this machine: `cargo test -p questkeep-core xbox -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn live_scan() {
        let packages = platform::appx_packages();
        println!("{} registered packages", packages.len());
        let p = XboxProvider::detect();
        match p.scan_installed() {
            Ok(games) => {
                println!("{} Xbox games", games.len());
                for g in games {
                    println!(
                        "{} | {} | {:?} | {:?}",
                        g.name,
                        g.source_game_id,
                        g.install_dir,
                        g.play_action.map(|a| a.target)
                    );
                }
            }
            Err(e) => println!("error: {e}"),
        }
    }
}
