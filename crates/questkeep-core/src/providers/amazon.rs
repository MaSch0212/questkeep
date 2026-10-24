//! Amazon Games through the Heroic Games Launcher (Linux): installed games and the owned library
//! come from Heroic's Nile runner data, see [`super::heroic`]. Heroic is signed in to Amazon
//! itself, so no account is needed here.
//!
//! The game id is the Amazon product id (`product.id` in Nile's library, Heroic's `app_name`), so
//! a future Windows Amazon provider can use the same `source_game_id`.

use super::heroic::{self, HeroicPaths};
use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::model::ImportedGame;

pub const AMAZON_SOURCE: &str = "amazon";

#[derive(Debug, Clone, Default)]
pub struct AmazonProvider {
    paths: HeroicPaths,
    /// Only Linux has Heroic; elsewhere the provider reports itself as not scannable.
    enabled: bool,
}

impl AmazonProvider {
    /// Looks at the real environment (see [`HeroicPaths::detect`]).
    pub fn detect() -> Self {
        Self {
            paths: HeroicPaths::detect(),
            enabled: cfg!(not(windows)),
        }
    }

    pub fn with_paths(paths: HeroicPaths) -> Self {
        Self {
            paths,
            enabled: true,
        }
    }
}

impl LibraryProvider for AmazonProvider {
    fn id(&self) -> &str {
        AMAZON_SOURCE
    }

    fn name(&self) -> &str {
        "Amazon Games"
    }

    fn can_scan(&self) -> bool {
        self.enabled
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if !self.enabled {
            return Err(ProviderError::NotFound(
                "Amazon Games are only read from Heroic, which is only available on Linux".into(),
            ));
        }
        heroic::nile_games(&self.paths, false)
    }

    fn can_scan_owned(&self) -> bool {
        self.enabled && !heroic::nile_library(&self.paths).is_empty()
    }

    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        if !self.enabled {
            return Err(ProviderError::Unsupported);
        }
        heroic::nile_games(&self.paths, true)
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        heroic::nile_library(&self.paths)
            .into_iter()
            .find(|e| e.id == source_game_id)
            .and_then(|e| e.cover)
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
            .map(ImageCandidate::Url)
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn scans_installed_owned_and_covers() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let hero = home.join(".config/heroic");
        let fez = home.join("Games/Fez");
        fs::create_dir_all(&fez).unwrap();
        write(
            &hero.join("nile_config/nile/installed.json"),
            &serde_json::json!([{"id": "p1", "version": "1", "path": fez}]).to_string(),
        );
        write(
            &hero.join("store_cache/nile_library.json"),
            r#"{"library": [
                {"app_name": "p1", "title": "Fez", "art_cover": "https://i/fez.jpg"},
                {"app_name": "p2", "title": "Ball", "art_cover": "file:///x.jpg"}
            ]}"#,
        );
        let p = AmazonProvider::with_paths(HeroicPaths::discover(home, None, None));
        assert_eq!((p.id(), p.name()), ("amazon", "Amazon Games"));
        assert!(p.can_scan() && p.can_scan_owned());
        assert_eq!(p.scan_installed().unwrap().len(), 1);
        let owned = p.scan_owned(&serde_json::Value::Null).unwrap();
        assert_eq!(owned.len(), 2);
        assert!(!owned[0].installed && owned[1].installed);
        assert_eq!(
            p.cover_candidates("p1"),
            vec![ImageCandidate::Url("https://i/fez.jpg".into())]
        );
        assert!(p.cover_candidates("p2").is_empty());
        assert!(p.cover_candidates("nope").is_empty());
    }

    #[test]
    fn without_heroic_or_data() {
        let p = AmazonProvider::default();
        assert!(!p.can_scan() && !p.can_scan_owned());
        assert!(matches!(
            p.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
        assert!(matches!(
            p.scan_owned(&serde_json::Value::Null),
            Err(ProviderError::Unsupported)
        ));
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".config/heroic")).unwrap();
        let p = AmazonProvider::with_paths(HeroicPaths::discover(tmp.path(), None, None));
        assert!(!p.can_scan_owned());
        assert!(p.scan_installed().unwrap().is_empty());
    }
}
