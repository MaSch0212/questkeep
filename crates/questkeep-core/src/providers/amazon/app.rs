//! The Amazon Games app (Windows): installed games and product art from the app's own SQLite
//! databases below `%LOCALAPPDATA%\Amazon Games\Data\Games\Sql\`. Verified against a real
//! installation:
//!
//! - `GameInstallInfo.sqlite`, table `DbSet`: `Id` (the Amazon product id, a UUID, the same id as
//!   Nile's `product.id` and Heroic's `app_name`), `InstallDirectory`, `Installed` (0/1),
//!   `ProductTitle`, `ProductAsin`, `InstallVersion*`, `ManifestSignature*`. Rows stay after an
//!   uninstall with `Installed = 0`.
//! - `ProductDetails.sqlite`, table `game_product_info(context, key, value)`: a cache of the
//!   store pages the user looked at, one row per locale and product (`context` is the locale, `key`
//!   the product id, `value` a JSON document with `ProductIconUrl`, `Background`, `Background2`,
//!   `ProductLogoUrl`, `Screenshots`, ...). It holds no portrait art.
//! - `Entitlements.sqlite`, table `game_entitlements`: DPAPI-encrypted blobs per user id. The
//!   owned library cannot be read from it, so there is no owned scan here.
//!
//! The app is started through the `amazon-games://` URI scheme it registers (`play/<id>`,
//! `install/<id>`). Questkeep never deletes anything itself: Uninstall opens the app, where the
//! user removes the game (the app has no uninstall URI).

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite::types::Value;

use super::super::ProviderError;
use super::super::sqlite::read_foreign;
use crate::model::{ImportedGame, PlayAction, TrackingMode};

const INSTALL_DB: &str = "GameInstallInfo.sqlite";
const DETAILS_DB: &str = "ProductDetails.sqlite";
const CLIENT_EXE: &str = "Amazon Games.exe";

/// Where the Amazon Games app keeps its data and where its executable is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmazonAppPaths {
    /// `...\Amazon Games\Data\Games\Sql`
    sql_dir: PathBuf,
    /// `...\Amazon Games\App\Amazon Games.exe`
    client: PathBuf,
}

impl AmazonAppPaths {
    /// The per-user locations below `local_app_data` (the app installs per user).
    pub fn below(local_app_data: &Path) -> Self {
        let root = local_app_data.join("Amazon Games");
        Self {
            sql_dir: root.join("Data").join("Games").join("Sql"),
            client: root.join("App").join(CLIENT_EXE),
        }
    }

    /// The real `%LOCALAPPDATA%`; `None` when it is unknown.
    pub fn detect() -> Option<Self> {
        dirs::data_local_dir().map(|d| Self::below(&d))
    }

    /// Whether the app has ever stored data here.
    pub fn has_data(&self) -> bool {
        self.sql_dir.is_dir()
    }

    pub fn has_client(&self) -> bool {
        self.client.is_file()
    }

    /// Starts the game through the app.
    pub fn play_action(id: &str, install_dir: &str) -> Option<PlayAction> {
        let mut action = PlayAction::uri(uri("play", id)?);
        action.tracking = TrackingMode::Directory;
        action.tracking_path = Some(install_dir.to_string());
        Some(action)
    }

    /// Asks the app to install a game (it only reacts once its services are running; otherwise
    /// it just opens its window).
    pub fn install_action(&self, id: &str) -> Option<PlayAction> {
        self.has_client().then_some(())?;
        Some(PlayAction::uri(uri("install", id)?))
    }

    /// Starts the app itself.
    pub fn launcher_action(&self) -> Option<PlayAction> {
        self.has_client()
            .then(|| PlayAction::file(self.client.to_string_lossy().into_owned()))
    }

    /// Opens the app; the game is removed there by the user.
    pub fn uninstall_action(&self) -> Option<PlayAction> {
        self.launcher_action()
    }

    /// Installed games from `GameInstallInfo.sqlite`, sorted by name. Rows whose install folder
    /// is gone are skipped.
    pub fn installed_games(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if !self.has_data() {
            return Err(ProviderError::NotFound(
                "the Amazon Games app was not found (no data folder below %LOCALAPPDATA%\\Amazon Games)"
                    .into(),
            ));
        }
        let file = self.sql_dir.join(INSTALL_DB);
        if !file.is_file() {
            return Ok(Vec::new());
        }
        let rows = read_foreign(&file, "Amazon Games install database", read_installed)?;
        let mut games: Vec<ImportedGame> = Vec::new();
        for row in rows {
            let id = row.id.trim();
            let dir = row.install_dir.as_deref().map(str::trim).unwrap_or("");
            if id.is_empty() || games.iter().any(|g| g.source_game_id == id) {
                continue;
            }
            if dir.is_empty() || !Path::new(dir).is_dir() {
                tracing::warn!("Amazon game {id} has no existing install folder, skipped");
                continue;
            }
            let Some(action) = Self::play_action(id, dir) else {
                tracing::warn!("ignoring Amazon game with an unusable id {id:?}");
                continue;
            };
            let name = clean_name(row.title.as_deref().unwrap_or(""));
            let name = if name.is_empty() {
                Path::new(dir)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| id.to_string())
            } else {
                name
            };
            let mut game = ImportedGame::installed(id, name);
            game.install_dir = Some(dir.to_string());
            game.play_action = Some(action);
            games.push(game);
        }
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    /// Cached store art of a product, when the app has it (see the module docs).
    pub fn product_art(&self, id: &str) -> ProductArt {
        let file = self.sql_dir.join(DETAILS_DB);
        if !file.is_file() {
            return ProductArt::default();
        }
        match read_foreign(&file, "Amazon Games product database", read_details) {
            Ok(rows) => art_for(&rows, id),
            Err(e) => {
                tracing::debug!("no Amazon product art: {e}");
                ProductArt::default()
            }
        }
    }
}

/// Image URLs from the product details cache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductArt {
    /// `ProductIconUrl`.
    pub icon: Option<String>,
    /// `Background` (wide), else `Background2`.
    pub background: Option<String>,
}

struct InstallRow {
    id: String,
    install_dir: Option<String>,
    title: Option<String>,
}

fn read_installed(conn: &Connection) -> rusqlite::Result<Vec<InstallRow>> {
    let mut st =
        conn.prepare("SELECT Id, InstallDirectory, ProductTitle FROM DbSet WHERE Installed = 1")?;
    st.query_map([], |r| {
        Ok(InstallRow {
            id: r.get(0)?,
            install_dir: r.get(1)?,
            title: r.get(2)?,
        })
    })?
    .collect()
}

/// `(product id, JSON document)` of every cached product.
fn read_details(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut st = conn.prepare("SELECT key, value FROM game_product_info")?;
    let rows = st.query_map([], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Value>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (key, value) = row?;
        let text = match value {
            Value::Blob(b) => String::from_utf8_lossy(&b).into_owned(),
            Value::Text(t) => t,
            _ => continue,
        };
        if let Some(key) = key {
            out.push((key, text));
        }
    }
    Ok(out)
}

fn art_for(rows: &[(String, String)], id: &str) -> ProductArt {
    let mut art = ProductArt::default();
    for (key, json) in rows {
        if !key.eq_ignore_ascii_case(id) {
            continue;
        }
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(json) else {
            continue;
        };
        let url = |field: &str| {
            doc.get(field)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
                .map(str::to_string)
        };
        // Several locales: the first one that has a value wins.
        art.icon = art.icon.or_else(|| url("ProductIconUrl"));
        art.background = art
            .background
            .or_else(|| url("Background"))
            .or_else(|| url("Background2"));
    }
    art
}

/// `amazon-games://<verb>/<id>`; `None` for ids that are not plain identifiers.
fn uri(verb: &str, id: &str) -> Option<String> {
    let plain = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    plain.then(|| format!("amazon-games://{verb}/{id}"))
}

/// Strips trademark signs and surplus whitespace from a title.
pub(super) fn clean_name(name: &str) -> String {
    name.replace(['\u{2122}', '\u{00ae}', '\u{00a9}'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::ActionKind;

    /// The table shape of the real `GameInstallInfo.sqlite` (from an Amazon Games 2026 install).
    const INSTALL_SCHEMA: &str = include_str!("../../../tests/fixtures/amazon/install_schema.sql");
    /// Product ids and titles of games seen on a real machine, plus the art fields of one product
    /// details row; no account data.
    const PRODUCT_JSON: &str = include_str!("../../../tests/fixtures/amazon/product_details.json");

    const SUPERHOT: &str = "62f505d5-9210-4784-9094-17cdc868f6da";

    fn app(tmp: &Path) -> AmazonAppPaths {
        let app = AmazonAppPaths::below(tmp);
        fs::create_dir_all(&app.sql_dir).unwrap();
        app
    }

    fn install_db(app: &AmazonAppPaths, rows: &[(&str, &Path, i32, &str)]) {
        let conn = Connection::open(app.sql_dir.join(INSTALL_DB)).unwrap();
        conn.execute_batch(INSTALL_SCHEMA).unwrap();
        for (id, dir, installed, title) in rows {
            conn.execute(
                "INSERT INTO DbSet (Id, InstallDirectory, Installed, ProductTitle) \
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, dir.to_string_lossy(), installed, title],
            )
            .unwrap();
        }
    }

    #[test]
    fn reads_installed_games() {
        let tmp = tempfile::tempdir().unwrap();
        let app = app(tmp.path());
        let hot = tmp.path().join("Library/SUPERHOT");
        let ball = tmp.path().join("Library/Ball");
        let gone = tmp.path().join("Library/Gone");
        fs::create_dir_all(&hot).unwrap();
        fs::create_dir_all(&ball).unwrap();
        install_db(
            &app,
            &[
                (SUPERHOT, &hot, 1, "SUPERHOT\u{2122}"),
                ("b", &ball, 1, "  "),
                ("c", &gone, 1, "Gone"),
                ("d", &hot, 0, "Uninstalled"),
            ],
        );
        let games = app.installed_games().unwrap();
        let ids: Vec<_> = games.iter().map(|g| g.source_game_id.as_str()).collect();
        // The untitled game is named after its folder.
        assert_eq!(ids, ["b", SUPERHOT]);
        assert_eq!(games[0].name, "Ball");
        assert_eq!(games[1].name, "SUPERHOT");
        assert!(games.iter().all(|g| g.installed));
        let action = games[1].play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(action.target, format!("amazon-games://play/{SUPERHOT}"));
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(
            action.tracking_path.as_deref(),
            Some(hot.to_string_lossy().as_ref())
        );
        assert_eq!(games[1].install_dir, action.tracking_path);
    }

    #[test]
    fn missing_data_and_empty_database() {
        let tmp = tempfile::tempdir().unwrap();
        let app = AmazonAppPaths::below(tmp.path());
        assert!(!app.has_data());
        assert!(matches!(
            app.installed_games(),
            Err(ProviderError::NotFound(_))
        ));
        let app = self::app(tmp.path());
        assert!(app.has_data());
        assert!(app.installed_games().unwrap().is_empty());
        install_db(&app, &[]);
        assert!(app.installed_games().unwrap().is_empty());
    }

    #[test]
    fn a_corrupt_database_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let app = app(tmp.path());
        fs::write(app.sql_dir.join(INSTALL_DB), b"not a database at all").unwrap();
        assert!(matches!(
            app.installed_games(),
            Err(ProviderError::Parse(_))
        ));
    }

    fn details_db(app: &AmazonAppPaths, rows: &[(&str, &str, Value)]) {
        let conn = Connection::open(app.sql_dir.join(DETAILS_DB)).unwrap();
        conn.execute_batch(
            "CREATE TABLE game_product_info(context TEXT, key TEXT, value BLOB, \
             CONSTRAINT game_product_info_ctx_key_idx UNIQUE (context, key) ON CONFLICT REPLACE)",
        )
        .unwrap();
        for (ctx, key, value) in rows {
            conn.execute(
                "INSERT INTO game_product_info VALUES (?1, ?2, ?3)",
                rusqlite::params![ctx, key, value],
            )
            .unwrap();
        }
    }

    #[test]
    fn product_art_comes_from_the_details_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let app = app(tmp.path());
        assert_eq!(app.product_art(SUPERHOT), ProductArt::default());
        details_db(
            &app,
            &[
                (
                    "en-US",
                    SUPERHOT,
                    Value::Blob(PRODUCT_JSON.as_bytes().to_vec()),
                ),
                (
                    "de-DE",
                    "other",
                    Value::Text(r#"{"Background2": "ftp://x"}"#.into()),
                ),
            ],
        );
        assert_eq!(
            app.product_art(SUPERHOT),
            ProductArt {
                icon: Some("https://m.media-amazon.com/images/I/81Hm7l8AFVL.jpg".into()),
                background: Some("https://m.media-amazon.com/images/I/71oN8xRyzfL.jpg".into()),
            }
        );
        // Not a web URL, or not cached at all.
        assert_eq!(app.product_art("other"), ProductArt::default());
        assert_eq!(app.product_art("nope"), ProductArt::default());
    }

    #[test]
    fn background2_is_the_fallback_and_locales_merge() {
        let rows = vec![
            (
                "p".to_string(),
                r#"{"Background2": "https://b/2.jpg"}"#.to_string(),
            ),
            (
                "P".to_string(),
                r#"{"ProductIconUrl": "https://i/1.jpg"}"#.to_string(),
            ),
            ("p".to_string(), "not json".to_string()),
        ];
        assert_eq!(
            art_for(&rows, "p"),
            ProductArt {
                icon: Some("https://i/1.jpg".into()),
                background: Some("https://b/2.jpg".into()),
            }
        );
    }

    #[test]
    fn actions_need_the_client_and_plain_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let app = app(tmp.path());
        assert!(app.install_action("x").is_none() && app.uninstall_action().is_none());
        fs::create_dir_all(app.client.parent().unwrap()).unwrap();
        fs::write(&app.client, b"").unwrap();
        let install = app.install_action(SUPERHOT).unwrap();
        assert_eq!(install.kind, ActionKind::Uri);
        assert_eq!(install.target, format!("amazon-games://install/{SUPERHOT}"));
        let remove = app.uninstall_action().unwrap();
        assert_eq!(remove.kind, ActionKind::File);
        assert!(remove.target.ends_with(CLIENT_EXE));
        assert!(app.install_action("a/b").is_none() && app.install_action("").is_none());
        assert!(AmazonAppPaths::play_action("x y", "C:/g").is_none());
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_name(" Fez\u{2122}  \u{00ae} Remix "), "Fez Remix");
    }
}
