//! Lutris provider (Linux): reads the installed games from Lutris' `pga.db` SQLite database and
//! launches them with Lutris' `lutris:rungame/<slug>` URI.
//!
//! Schema (verified against Lutris' `lutris/database/schema.py`): table `games` with `slug`,
//! `name`, `runner`, `directory`, `installed`, `lastplayed` (unix seconds) and `playtime`
//! (hours, REAL). Games of the `steam` runner are skipped because the Steam provider has them.
//! The database is opened read-only and, if that fails (locked), read from a temporary copy.
//! Service libraries (`service_games`: `service`, `appid`, `name`, ...) give owned-only games, see
//! `scan_owned` and `owned_for_service`.
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
/// Lutris services whose games another Questkeep provider owns (`steam`, `egs` -> `epic`,
/// `gog`, `amazon`).
const COVERED_SERVICES: [&str; 4] = ["steam", "egs", "gog", "amazon"];

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

    fn db_files(&self) -> Vec<PathBuf> {
        self.data_dirs
            .iter()
            .map(|d| d.join("pga.db"))
            .filter(|p| p.is_file())
            .collect()
    }

    /// The not-installed games of all Lutris service libraries (deduplicated by service and id).
    pub fn service_games(&self) -> Result<Vec<ServiceGame>, ProviderError> {
        if !self.enabled {
            return Err(ProviderError::NotFound(
                "Lutris is only available on Linux".into(),
            ));
        }
        let dbs = self.db_files();
        if dbs.is_empty() {
            return Err(ProviderError::NotFound(
                "Lutris database not found (is Lutris installed and has it been started once?)"
                    .into(),
            ));
        }
        let mut out: Vec<ServiceGame> = Vec::new();
        let mut last_error = None;
        let mut any_ok = false;
        for db in dbs {
            match read_db(&db, read_service_games) {
                Ok(list) => {
                    any_ok = true;
                    for g in list {
                        if !out
                            .iter()
                            .any(|o| o.service == g.service && o.appid == g.appid)
                        {
                            out.push(g);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("{e}");
                    last_error = Some(e);
                }
            }
        }
        if !any_ok && let Some(e) = last_error {
            return Err(e);
        }
        Ok(out)
    }

    /// The owned-but-not-installed games of one Lutris service as plain `installed: false` games
    /// whose id is the service's own id. For `egs` that is the Epic `AppName` and for `gog` the
    /// GOG product id, the ids the `epic` and `gog` providers use, so the Epic and GOG providers
    /// can merge these into their owned scans.
    pub fn owned_for_service(&self, service: &str) -> Result<Vec<ImportedGame>, ProviderError> {
        let mut games: Vec<ImportedGame> = self
            .service_games()?
            .into_iter()
            .filter(|g| g.service == service)
            .map(|g| {
                let mut game = ImportedGame::installed(g.appid, g.name);
                game.installed = false;
                game
            })
            .collect();
        games.sort_by_cached_key(|g| (g.name.to_lowercase(), g.source_game_id.clone()));
        Ok(games)
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

/// A game of a Lutris service library (`service_games`) that is not installed through Lutris.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceGame {
    /// Lutris' service id: `egs`, `gog`, `humblebundle`, `itchio`, ...
    pub service: String,
    pub appid: String,
    pub name: String,
}

/// The service libraries minus the games already installed through Lutris (`games.service_id`
/// of an installed game), which the main list has. Databases from before Lutris had services
/// (no `service_games` table) give an empty list.
fn read_service_games(conn: &Connection) -> rusqlite::Result<Vec<ServiceGame>> {
    let has_table: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'service_games'",
        [],
        |r| r.get(0),
    )?;
    if has_table == 0 {
        return Ok(Vec::new());
    }
    // `games.service`/`service_id` may be missing in old schemas: then nothing is linked.
    let mut linked: Vec<(String, String)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT service, service_id FROM games WHERE installed = 1")
    {
        let rows = stmt.query_map([], |r| Ok((r.get::<_, Value>(0)?, r.get::<_, Value>(1)?)))?;
        for (service, id) in rows.flatten() {
            if let (Some(s), Some(i)) = (text(service), text(id)) {
                linked.push((s, i));
            }
        }
    }
    let mut stmt = conn.prepare("SELECT service, appid, name FROM service_games")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Value>(0)?,
            r.get::<_, Value>(1)?,
            r.get::<_, Value>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for (service, appid, name) in rows.flatten() {
        // `appid` is text, but be lenient with integers written by other tools.
        let appid = match appid {
            Value::Integer(i) => Some(i.to_string()),
            other => text(other),
        };
        let (Some(service), Some(appid), Some(name)) = (text(service), appid, text(name)) else {
            continue;
        };
        if linked.contains(&(service.clone(), appid.clone())) {
            continue;
        }
        out.push(ServiceGame {
            service,
            appid,
            name,
        });
    }
    Ok(out)
}

fn read_db_readonly<T>(
    path: &Path,
    read: fn(&Connection) -> rusqlite::Result<T>,
) -> rusqlite::Result<T> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    read(&conn)
}

/// Runs `read` on `pga.db`; falls back to a temporary copy when the original can't be read (for
/// example because Lutris holds a lock).
fn read_db<T>(
    path: &Path,
    read: fn(&Connection) -> rusqlite::Result<T>,
) -> Result<T, ProviderError> {
    let first = match read_db_readonly(path, read) {
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
        .and_then(|_| read_db_readonly(&copy, read).map_err(|e| e.to_string()));
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
        let dbs = self.db_files();
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
            let rows = match read_db(&db, read_rows) {
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

    fn can_scan_owned(&self) -> bool {
        self.enabled && !self.db_files().is_empty()
    }

    /// Service library games that no other provider covers, with the id `<service>:<appid>`.
    /// Steam (own provider), Epic and GOG (see [`LutrisProvider::owned_for_service`]) and Amazon
    /// (Heroic) are left out so they don't appear twice under different sources.
    fn scan_owned(&self, _config: &serde_json::Value) -> Result<Vec<ImportedGame>, ProviderError> {
        let mut games: Vec<ImportedGame> = self
            .service_games()?
            .into_iter()
            .filter(|g| !COVERED_SERVICES.contains(&g.service.as_str()))
            .map(|g| {
                let mut game =
                    ImportedGame::installed(format!("{}:{}", g.service, g.appid), g.name);
                game.installed = false;
                game
            })
            .collect();
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

    fn service_db(env: &Env) -> Connection {
        let conn = env.db();
        conn.execute_batch(
            "ALTER TABLE games ADD COLUMN service TEXT;
             ALTER TABLE games ADD COLUMN service_id TEXT;
             CREATE TABLE service_games (id INTEGER PRIMARY KEY, service TEXT, appid TEXT,
                name TEXT, slug TEXT, icon TEXT, logo TEXT, url TEXT, details TEXT,
                lutris_slug TEXT);
             INSERT INTO service_games (service, appid, name, details) VALUES
                ('egs', 'Sugar', 'Sugar Game', '{}'),
                ('egs', 'Installed', 'Already Installed', '{}'),
                ('gog', '1207658924', 'The Witcher', '{}'),
                ('humblebundle', 'abc', 'Humble Thing', '{}'),
                ('itchio', '77', 'Itch Thing', '{}'),
                ('steam', '440', 'Team Fortress', '{}'),
                ('amazon', 'amzn1', 'Amazon Thing', '{}'),
                ('itchio', '', 'No id', '{}');
             INSERT INTO games (name, slug, runner, installed, service, service_id)
                VALUES ('Already Installed', 'already-installed', 'wine', 1, 'egs', 'Installed');
             INSERT INTO games (name, slug, runner, installed, service, service_id)
                VALUES ('Humble Installed', 'humble-installed', 'linux', 0, 'humblebundle', 'abc');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn owned_games_of_services_without_their_own_provider() {
        let env = Env::new();
        drop(service_db(&env));
        let p = env.provider();
        assert!(p.can_scan_owned());
        let owned = p.scan_owned(&serde_json::Value::Null).unwrap();
        let view: Vec<_> = owned
            .iter()
            .map(|g| (g.source_game_id.as_str(), g.name.as_str(), g.installed))
            .collect();
        assert_eq!(
            view,
            [
                ("humblebundle:abc", "Humble Thing", false),
                ("itchio:77", "Itch Thing", false)
            ]
        );
        assert!(owned[0].play_action.is_none());
    }

    #[test]
    fn owned_games_per_service_use_the_store_ids() {
        let env = Env::new();
        drop(service_db(&env));
        let p = env.provider();
        let epic = p.owned_for_service("egs").unwrap();
        // The game installed through Lutris is not repeated as an owned-only game.
        assert_eq!(epic.len(), 1);
        assert_eq!(epic[0].source_game_id, "Sugar");
        assert!(!epic[0].installed);
        let gog = p.owned_for_service("gog").unwrap();
        assert_eq!(gog[0].source_game_id, "1207658924");
        assert!(p.owned_for_service("nope").unwrap().is_empty());
    }

    #[test]
    fn databases_without_services_have_no_owned_games() {
        let env = Env::new();
        drop(env.db());
        let p = env.provider();
        assert!(p.scan_owned(&serde_json::Value::Null).unwrap().is_empty());
        // `service_games` present, `games` without the link columns.
        let env = Env::new();
        env.db()
            .execute_batch(
                "CREATE TABLE service_games (service TEXT, appid TEXT, name TEXT);
                 INSERT INTO service_games VALUES ('itchio', '1', 'One');",
            )
            .unwrap();
        let owned = env.provider().scan_owned(&serde_json::Value::Null).unwrap();
        assert_eq!(owned.len(), 1);
        // Not found / disabled.
        let env = Env::new();
        assert!(matches!(
            env.provider().scan_owned(&serde_json::Value::Null),
            Err(ProviderError::NotFound(_))
        ));
        assert!(!env.provider().can_scan_owned());
        assert!(matches!(
            LutrisProvider::default().service_games(),
            Err(ProviderError::NotFound(_))
        ));
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
