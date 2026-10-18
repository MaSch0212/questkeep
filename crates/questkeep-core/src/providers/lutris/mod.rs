//! Lutris provider (Linux): reads the installed games from Lutris' `pga.db` SQLite database and
//! launches them with Lutris' `lutris:rungame/<slug>` URI.
//!
//! Schema (verified against Lutris' `lutris/database/schema.py`): table `games` with `slug`,
//! `name`, `runner`, `directory`, `installed`, `lastplayed` (unix seconds) and `playtime`
//! (hours, REAL). Games of the `steam` runner are skipped because the Steam provider has them.
//! The database is opened read-only and, if that fails (locked), read from a temporary copy.
//! Covers come from Lutris' local `coverart`/`banners` caches, never from the network.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags};

use super::{ImageCandidate, LibraryProvider, ProviderError};
use crate::model::{ImportedGame, PlayAction, TrackingMode};

pub const LUTRIS_SOURCE: &str = "lutris";

const FLATPAK_DATA: &str = ".var/app/net.lutris.Lutris/data/lutris";
const FLATPAK_CACHE: &str = ".var/app/net.lutris.Lutris/cache/lutris";
const IMAGE_EXTENSIONS: [&str; 3] = ["jpg", "png", "webp"];

#[derive(Debug, Clone, Default)]
pub struct LutrisProvider {
    /// Lutris data directories (containing `pga.db`, `coverart/`, `banners/`).
    data_dirs: Vec<PathBuf>,
    /// Lutris cache directories (newer versions keep cover art there).
    cache_dirs: Vec<PathBuf>,
    home: Option<PathBuf>,
    /// Lutris only exists on Linux; elsewhere the provider reports itself as not scannable.
    enabled: bool,
}

impl LutrisProvider {
    /// Looks at the real environment (`HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`).
    pub fn detect() -> Self {
        let Some(home) = dirs::home_dir() else {
            return Self::default();
        };
        let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from);
        let mut p = Self::discover(&home, data.as_deref(), cache.as_deref());
        p.enabled = cfg!(not(windows));
        p
    }

    /// Finds the existing Lutris directories below `home` and the given XDG base directories.
    pub fn discover(home: &Path, xdg_data: Option<&Path>, xdg_cache: Option<&Path>) -> Self {
        let existing = |candidates: Vec<PathBuf>| {
            let mut out: Vec<PathBuf> = Vec::new();
            for c in candidates {
                if c.is_dir() && !out.contains(&c) {
                    out.push(c);
                }
            }
            out
        };
        let abs = |p: Option<&Path>| p.filter(|p| p.is_absolute()).map(Path::to_path_buf);
        let mut data: Vec<PathBuf> = abs(xdg_data)
            .map(|d| d.join("lutris"))
            .into_iter()
            .collect();
        data.push(home.join(".local/share/lutris"));
        data.push(home.join(FLATPAK_DATA));
        let mut cache: Vec<PathBuf> = abs(xdg_cache)
            .map(|d| d.join("lutris"))
            .into_iter()
            .collect();
        cache.push(home.join(".cache/lutris"));
        cache.push(home.join(FLATPAK_CACHE));
        Self {
            data_dirs: existing(data),
            cache_dirs: existing(cache),
            home: Some(home.to_path_buf()),
            enabled: true,
        }
    }

    fn image_candidates(&self, kind: &str, slug: &str) -> Vec<ImageCandidate> {
        if !is_safe_slug(slug) {
            return Vec::new();
        }
        let roots = self.data_dirs.iter().chain(&self.cache_dirs);
        let mut out = Vec::new();
        for root in roots {
            for ext in IMAGE_EXTENSIONS {
                let file = root.join(kind).join(format!("{slug}.{ext}"));
                if file.is_file() {
                    out.push(ImageCandidate::File(file));
                }
            }
        }
        out
    }

    fn game_from_row(&self, row: LutrisRow) -> ImportedGame {
        let mut game = ImportedGame::installed(&row.slug, &row.name);
        // A game folder equal to the home folder or `/` would match every process.
        let dir = row.directory.filter(|d| Path::new(d).is_dir()).filter(|d| {
            let p = Path::new(d);
            p.parent().is_some() && self.home.as_deref() != Some(p)
        });
        let mut action = PlayAction::uri(format!("lutris:rungame/{}", percent_encode(&row.slug)));
        if let Some(dir) = &dir {
            action.tracking = TrackingMode::Directory;
            action.tracking_path = Some(dir.clone());
        }
        game.install_dir = dir;
        game.play_action = Some(action);
        game.playtime_seconds = row
            .playtime
            .filter(|h| h.is_finite() && *h > 0.0)
            .map(|h| (h * 3600.0).round() as u64);
        game.last_played = row
            .lastplayed
            .filter(|t| *t > 0)
            .and_then(|t| DateTime::<Utc>::from_timestamp(t, 0));
        game
    }
}

/// Slugs become file names, so anything that could escape the folder is rejected.
fn is_safe_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('.')
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// One row of the `games` table.
#[derive(Debug, Clone, PartialEq)]
struct LutrisRow {
    slug: String,
    name: String,
    runner: Option<String>,
    directory: Option<String>,
    /// Hours.
    playtime: Option<f64>,
    lastplayed: Option<i64>,
}

fn text(v: Value) -> Option<String> {
    match v {
        Value::Text(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        _ => None,
    }
}

fn number(v: Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(i as f64),
        Value::Real(r) => Some(r),
        Value::Text(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn read_rows(conn: &Connection) -> rusqlite::Result<Vec<LutrisRow>> {
    let mut stmt = conn.prepare(
        "SELECT slug, name, runner, directory, playtime, lastplayed FROM games WHERE installed = 1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Value>(0)?,
            r.get::<_, Value>(1)?,
            r.get::<_, Value>(2)?,
            r.get::<_, Value>(3)?,
            r.get::<_, Value>(4)?,
            r.get::<_, Value>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (slug, name, runner, directory, playtime, lastplayed) = row?;
        let (Some(slug), Some(name)) = (text(slug), text(name)) else {
            continue;
        };
        out.push(LutrisRow {
            slug,
            name,
            runner: text(runner),
            directory: text(directory),
            playtime: number(playtime),
            lastplayed: number(lastplayed).map(|n| n as i64),
        });
    }
    Ok(out)
}

fn read_db_readonly(path: &Path) -> rusqlite::Result<Vec<LutrisRow>> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    read_rows(&conn)
}

/// Reads the installed games of `pga.db`; falls back to a temporary copy when the original can't
/// be read (for example because Lutris holds a lock).
fn read_db(path: &Path) -> Result<Vec<LutrisRow>, ProviderError> {
    let first = match read_db_readonly(path) {
        Ok(rows) => return Ok(rows),
        Err(e) => e,
    };
    tracing::debug!(
        "reading {} directly failed ({first}), trying a copy",
        path.display()
    );
    let copy = std::env::temp_dir().join(format!("questkeep-lutris-{}.db", uuid::Uuid::new_v4()));
    let result = std::fs::copy(path, &copy)
        .map_err(|e| e.to_string())
        .and_then(|_| read_db_readonly(&copy).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&copy);
    result.map_err(|e| {
        ProviderError::Parse(format!(
            "could not read the Lutris database {}: {first}; copy: {e}",
            path.display()
        ))
    })
}

impl LibraryProvider for LutrisProvider {
    fn id(&self) -> &str {
        LUTRIS_SOURCE
    }

    fn name(&self) -> &str {
        "Lutris"
    }

    fn can_scan(&self) -> bool {
        self.enabled
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        if !self.enabled {
            return Err(ProviderError::NotFound(
                "Lutris is only available on Linux".into(),
            ));
        }
        let dbs: Vec<PathBuf> = self
            .data_dirs
            .iter()
            .map(|d| d.join("pga.db"))
            .filter(|p| p.is_file())
            .collect();
        if dbs.is_empty() {
            return Err(ProviderError::NotFound(
                "Lutris database not found (is Lutris installed and has it been started once?)"
                    .into(),
            ));
        }
        let mut games: Vec<ImportedGame> = Vec::new();
        let mut last_error = None;
        let mut any_ok = false;
        for db in dbs {
            let rows = match read_db(&db) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("{e}");
                    last_error = Some(e);
                    continue;
                }
            };
            any_ok = true;
            for row in rows {
                if row
                    .runner
                    .as_deref()
                    .is_some_and(|r| r.eq_ignore_ascii_case("steam"))
                {
                    continue;
                }
                if games.iter().any(|g| g.source_game_id == row.slug) {
                    continue;
                }
                games.push(self.game_from_row(row));
            }
        }
        if !any_ok && let Some(e) = last_error {
            return Err(e);
        }
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
    }

    fn cover_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.image_candidates("coverart", source_game_id)
    }

    fn background_candidates(&self, source_game_id: &str) -> Vec<ImageCandidate> {
        self.image_candidates("banners", source_game_id)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::ActionKind;

    struct Env {
        tmp: tempfile::TempDir,
    }

    impl Env {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            fs::create_dir_all(tmp.path().join(".local/share/lutris")).unwrap();
            Self { tmp }
        }
        fn home(&self) -> &Path {
            self.tmp.path()
        }
        fn db(&self) -> Connection {
            let conn = Connection::open(self.home().join(".local/share/lutris/pga.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE games (id INTEGER PRIMARY KEY, name TEXT, slug TEXT, runner TEXT,
                    directory TEXT, installed INTEGER, lastplayed INTEGER, playtime REAL);",
            )
            .unwrap();
            conn
        }
        fn provider(&self) -> LutrisProvider {
            LutrisProvider::discover(self.home(), None, None)
        }
    }

    fn insert(
        conn: &Connection,
        slug: &str,
        name: &str,
        runner: &str,
        dir: Option<&Path>,
        installed: i64,
    ) {
        conn.execute(
            "INSERT INTO games (name, slug, runner, directory, installed, lastplayed, playtime)
             VALUES (?1, ?2, ?3, ?4, ?5, 1700000000, 1.5)",
            rusqlite::params![
                name,
                slug,
                runner,
                dir.map(|d| d.display().to_string()),
                installed
            ],
        )
        .unwrap();
    }

    #[test]
    fn reads_installed_games_and_skips_steam_and_uninstalled() {
        let env = Env::new();
        let dir = env.home().join("Games/hades");
        fs::create_dir_all(&dir).unwrap();
        let conn = env.db();
        insert(&conn, "hades", "Hades", "wine", Some(&dir), 1);
        insert(&conn, "portal", "Portal", "steam", None, 1);
        insert(&conn, "uninstalled", "Not here", "linux", None, 0);
        insert(&conn, "native", "Alpha Native", "linux", None, 1);
        insert(&conn, "hades", "Hades Duplicate", "wine", None, 1);
        conn.execute(
            "INSERT INTO games (name, slug, installed) VALUES ('', 'noname', 1)",
            [],
        )
        .unwrap();
        drop(conn);

        let games = env.provider().scan_installed().unwrap();
        let ids: Vec<_> = games.iter().map(|g| g.source_game_id.as_str()).collect();
        assert_eq!(ids, ["native", "hades"]);

        let hades = &games[1];
        assert_eq!(hades.name, "Hades");
        assert_eq!(hades.playtime_seconds, Some(5400));
        assert_eq!(hades.last_played.unwrap().timestamp(), 1_700_000_000);
        assert_eq!(
            hades.install_dir.as_deref(),
            Some(dir.display().to_string().as_str())
        );
        let action = hades.play_action.as_ref().unwrap();
        assert_eq!(action.kind, ActionKind::Uri);
        assert_eq!(action.target, "lutris:rungame/hades");
        assert_eq!(action.tracking, TrackingMode::Directory);
        assert_eq!(action.tracking_path, hades.install_dir);

        let native = &games[0];
        assert!(native.install_dir.is_none());
        assert_eq!(
            native.play_action.as_ref().unwrap().tracking,
            TrackingMode::Default
        );
    }

    #[test]
    fn home_directory_is_not_used_for_tracking() {
        let env = Env::new();
        let conn = env.db();
        insert(&conn, "a", "A", "wine", Some(env.home()), 1);
        drop(conn);
        let games = env.provider().scan_installed().unwrap();
        assert!(games[0].install_dir.is_none());
    }

    #[test]
    fn missing_database_and_disabled_provider_are_not_found() {
        let env = Env::new();
        assert!(matches!(
            env.provider().scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
        let off = LutrisProvider::default();
        assert!(!off.can_scan());
        assert!(matches!(
            off.scan_installed(),
            Err(ProviderError::NotFound(_))
        ));
        assert!(env.provider().can_scan());
    }

    #[test]
    fn malformed_database_is_an_error() {
        let env = Env::new();
        fs::write(
            env.home().join(".local/share/lutris/pga.db"),
            "not a database",
        )
        .unwrap();
        assert!(matches!(
            env.provider().scan_installed(),
            Err(ProviderError::Parse(_))
        ));
        // A database without the table.
        let env = Env::new();
        Connection::open(env.home().join(".local/share/lutris/pga.db"))
            .unwrap()
            .execute_batch("CREATE TABLE other (x INTEGER);")
            .unwrap();
        assert!(matches!(
            env.provider().scan_installed(),
            Err(ProviderError::Parse(_))
        ));
    }

    #[test]
    fn tolerates_odd_value_types() {
        let env = Env::new();
        let conn = env.db();
        conn.execute(
            "INSERT INTO games (name, slug, runner, installed, playtime, lastplayed)
             VALUES ('Odd', 'odd', 'linux', 1, 'abc', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO games (name, slug, runner, installed, playtime, lastplayed)
             VALUES ('Text', 'text', 'linux', 1, '2', 'x')",
            [],
        )
        .unwrap();
        drop(conn);
        let games = env.provider().scan_installed().unwrap();
        let odd = games.iter().find(|g| g.source_game_id == "odd").unwrap();
        assert_eq!(odd.playtime_seconds, None);
        assert_eq!(odd.last_played, None);
        let text = games.iter().find(|g| g.source_game_id == "text").unwrap();
        assert_eq!(text.playtime_seconds, Some(7200));
    }

    #[test]
    fn covers_and_banners_from_data_and_cache_dirs() {
        let env = Env::new();
        let h = env.home();
        let data = h.join(".local/share/lutris");
        fs::create_dir_all(data.join("coverart")).unwrap();
        fs::create_dir_all(data.join("banners")).unwrap();
        fs::create_dir_all(h.join(".cache/lutris/coverart")).unwrap();
        fs::write(data.join("coverart/hades.jpg"), "x").unwrap();
        fs::write(data.join("banners/hades.png"), "x").unwrap();
        fs::write(h.join(".cache/lutris/coverart/hades.webp"), "x").unwrap();
        let p = env.provider();
        assert_eq!(
            p.cover_candidates("hades"),
            vec![
                ImageCandidate::File(data.join("coverart/hades.jpg")),
                ImageCandidate::File(h.join(".cache/lutris/coverart/hades.webp")),
            ]
        );
        assert_eq!(
            p.background_candidates("hades"),
            vec![ImageCandidate::File(data.join("banners/hades.png"))]
        );
        assert!(p.cover_candidates("other").is_empty());
        assert!(p.cover_candidates("../hades").is_empty());
    }

    #[test]
    fn discovers_flatpak_and_xdg_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let h = tmp.path();
        let xdg = h.join("xdg-data");
        fs::create_dir_all(xdg.join("lutris")).unwrap();
        fs::create_dir_all(h.join(FLATPAK_DATA)).unwrap();
        fs::create_dir_all(h.join(FLATPAK_CACHE)).unwrap();
        let p = LutrisProvider::discover(h, Some(&xdg), None);
        assert_eq!(p.data_dirs, vec![xdg.join("lutris"), h.join(FLATPAK_DATA)]);
        assert_eq!(p.cache_dirs, vec![h.join(FLATPAK_CACHE)]);
    }

    #[test]
    fn provider_metadata() {
        let p = LutrisProvider::default();
        assert_eq!(p.id(), "lutris");
        assert_eq!(p.name(), "Lutris");
        assert!(!p.can_scan_owned());
        assert!(p.install_action("x").is_none());
    }
}
