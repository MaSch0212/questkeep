//! What the Meta Horizon Link app caches about the signed-in user: the apps they own (entitlements),
//! the store details of those apps (names, art, descriptions) and PC playtime.
//!
//! The app keeps a GraphQL cache per session in `%APPDATA%\Oculus\Sessions\<session>\data.sqlite`
//! (`_oaf` holds what it needs offline, including the entitlements; the user's own session folder
//! holds more apps). Table `Objects(hashkey, typename, value)` stores one object per row in the
//! format read by [`crate::blob`]. The PC app only syncs PC (Rift) entitlements; Quest apps can
//! still show up as `Application` objects (e.g. from friends' activity) with a `platform` like
//! `ANDROID_6DOF`, so those are left out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use questkeep_store_api::ProviderError;

use crate::blob::{self, Object};
use crate::manifest::plausible_app_id;

/// The object types read from the cache.
const TYPES: &str = "'Application','Entitlement','OafOfflineData','UserAppPlayTime'";

/// One row of `Objects`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub type_name: String,
    pub value: Vec<u8>,
}

/// Store art the app has URLs for (they are signed and expire after a while).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Images {
    pub cover_square: Option<String>,
    pub cover_landscape: Option<String>,
    pub small_landscape: Option<String>,
    pub icon: Option<String>,
}

/// What the cache says about one app.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct App {
    pub id: String,
    pub name: Option<String>,
    pub canonical_name: Option<String>,
    /// `PC`, `ANDROID_6DOF`, ...; often missing on the PC app's full app objects.
    pub platform: Option<String>,
    /// Whether its binary is a `PCBinary`.
    pub pc_binary: bool,
    /// `GAMES`, `ENTERTAINMENT`, `APPS`, ...
    pub category: Option<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub developer: Option<String>,
    /// 0-5 stars.
    pub rating: Option<f64>,
    pub images: Images,
}

impl App {
    /// Whether the app runs on this PC: a PC platform or binary, or nothing saying otherwise.
    pub fn is_pc(&self) -> bool {
        match self.platform.as_deref() {
            Some(p) => p.eq_ignore_ascii_case("PC") || self.pc_binary,
            None => true,
        }
    }

    /// Whether the store files it as something else than a game; `None` when it doesn't say.
    pub fn is_app(&self) -> Option<bool> {
        self.category
            .as_deref()
            .map(|c| !c.eq_ignore_ascii_case("GAMES"))
    }

    /// Fills what `self` lacks from `other` (the same app from another cache).
    fn merge(&mut self, other: App) {
        fn keep<T>(a: &mut Option<T>, b: Option<T>) {
            if a.is_none() {
                *a = b;
            }
        }
        keep(&mut self.name, other.name);
        keep(&mut self.canonical_name, other.canonical_name);
        keep(&mut self.platform, other.platform);
        self.pc_binary |= other.pc_binary;
        keep(&mut self.category, other.category);
        keep(&mut self.description, other.description);
        if self.genres.is_empty() {
            self.genres = other.genres;
        }
        keep(&mut self.developer, other.developer);
        keep(&mut self.rating, other.rating);
        keep(&mut self.images.cover_square, other.images.cover_square);
        keep(
            &mut self.images.cover_landscape,
            other.images.cover_landscape,
        );
        keep(
            &mut self.images.small_landscape,
            other.images.small_landscape,
        );
        keep(&mut self.images.icon, other.images.icon);
    }
}

/// An owned app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entitlement {
    pub user: Option<String>,
    pub app_id: String,
}

/// Everything read from the caches.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub apps: HashMap<String, App>,
    pub entitlements: Vec<Entitlement>,
    /// The user last signed in to the app, when the cache says.
    pub user: Option<String>,
    /// App id -> seconds played on this PC.
    pub playtime: HashMap<String, u64>,
}

/// Strips trademark signs and surplus whitespace from a name.
fn clean_name(name: &str) -> String {
    name.replace(['\u{2122}', '\u{00ae}', '\u{00a9}'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn image(o: &Object, field: &str) -> Option<String> {
    o.object(field)?
        .str("uri")
        .filter(|u| u.starts_with("https://"))
        .map(str::to_string)
}

fn read_app(o: &Object) -> Option<App> {
    let id = o.str("id").filter(|id| plausible_app_id(id))?.to_string();
    let pc_binary = ["latest_supported_binary", "latest_released_binary"]
        .iter()
        .any(|f| o.type_of(f) == Some("PCBinary"));
    Some(App {
        id,
        name: o
            .str("display_name")
            .map(clean_name)
            .filter(|n| !n.is_empty()),
        canonical_name: o.str("canonical_name").map(str::to_string),
        platform: o.str("platform").map(str::to_string),
        pc_binary,
        category: o.str("category").map(str::to_string),
        description: o
            .str("display_long_description")
            .or_else(|| o.str("display_short_description"))
            .map(|d| d.trim().to_string()),
        genres: o.strs("genres"),
        developer: o
            .object("organization")
            .and_then(|org| org.str("name"))
            .map(|n| n.trim().to_string()),
        rating: o.float("quality_rating_aggregate").filter(|r| *r > 0.0),
        images: Images {
            cover_square: image(o, "cover_square_image"),
            cover_landscape: image(o, "cover_landscape_image"),
            small_landscape: image(o, "small_landscape_image"),
            icon: image(o, "icon_image"),
        },
    })
}

/// `<user>:<app>` entitlement ids; the app id comes from the `item` reference.
fn read_entitlement(o: &Object) -> Option<Entitlement> {
    if o.bool("is_active") == Some(false) {
        return None;
    }
    if o.type_of("item") != Some("Application") {
        return None;
    }
    let app_id = o.ref_key("item").filter(|id| plausible_app_id(id))?;
    let user = o
        .str("id")
        .and_then(|id| id.split_once(':'))
        .map(|(user, _)| user.to_string());
    Some(Entitlement {
        user,
        app_id: app_id.to_string(),
    })
}

/// Builds the catalog from the rows of all caches. Rows that can't be read are skipped.
pub fn from_rows(rows: impl IntoIterator<Item = Row>) -> Catalog {
    let mut catalog = Catalog::default();
    let mut skipped = 0usize;
    for row in rows {
        let object = match blob::parse(&row.value) {
            Ok(o) => o,
            Err(err) => {
                skipped += 1;
                tracing::debug!(key = row.key, kind = row.type_name, %err, "unreadable Meta cache row");
                continue;
            }
        };
        match object.type_name.as_str() {
            "Application" => {
                if let Some(app) = read_app(&object) {
                    match catalog.apps.get_mut(&app.id) {
                        Some(known) => known.merge(app),
                        None => {
                            catalog.apps.insert(app.id.clone(), app);
                        }
                    }
                }
            }
            "Entitlement" => {
                if let Some(e) = read_entitlement(&object)
                    && !catalog.entitlements.contains(&e)
                {
                    catalog.entitlements.push(e);
                }
            }
            "OafOfflineData" => {
                let user = object
                    .object("last_logged_in_viewer")
                    .and_then(|v| v.ref_key("user"))
                    .filter(|u| !u.is_empty());
                if let Some(user) = user {
                    catalog.user = Some(user.to_string());
                }
            }
            "UserAppPlayTime" => {
                // `<user>_<app id>`; desktop programs use a path instead of an app id.
                let id = object.str("id").and_then(|id| id.split_once('_'));
                let seconds = object.int("play_time").and_then(|s| u64::try_from(s).ok());
                if let (Some((_, app)), Some(seconds)) = (id, seconds)
                    && plausible_app_id(app)
                {
                    let entry = catalog.playtime.entry(app.to_string()).or_default();
                    *entry = (*entry).max(seconds);
                }
            }
            _ => {}
        }
    }
    if skipped > 0 {
        tracing::debug!(skipped, "Meta cache rows skipped");
    }
    catalog
}

impl Catalog {
    /// The ids of the owned PC apps, in a stable order: entitlements of the user last signed in
    /// (all, when it can't be told), without apps known to be for Quest only.
    pub fn owned(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for e in &self.entitlements {
            let same_user = match (&self.user, &e.user) {
                (Some(user), Some(owner)) => user == owner,
                _ => true,
            };
            let pc = self.apps.get(&e.app_id).is_none_or(App::is_pc);
            if same_user && pc && !ids.contains(&e.app_id) {
                ids.push(e.app_id.clone());
            }
        }
        ids.sort();
        ids
    }
}

/// The `data.sqlite` of every session folder below `sessions`, `_oaf` first.
pub fn session_databases(sessions: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(sessions) else {
        return Vec::new();
    };
    let mut dbs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path().join("data.sqlite"))
        .filter(|p| p.is_file())
        .collect();
    dbs.sort_by_key(|p| {
        let oaf = p
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n.eq_ignore_ascii_case("_oaf"));
        (!oaf, p.clone())
    });
    dbs
}

fn read_rows(conn: &Connection) -> rusqlite::Result<Vec<Row>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT hashkey, typename, value FROM Objects WHERE typename IN ({TYPES})"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok(Row {
            key: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            type_name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            value: r.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default(),
        })
    })?;
    rows.collect()
}

/// Reads every session cache below `sessions`. `NotConfigured` when there is none (the app was
/// never signed in on this PC); a cache that can't be read is skipped when another one could be.
pub fn read(sessions: &Path) -> Result<Catalog, ProviderError> {
    let dbs = session_databases(sessions);
    if dbs.is_empty() {
        return Err(ProviderError::NotConfigured(
            "Sign in to the Meta Horizon Link app to list the apps you own.".into(),
        ));
    }
    let mut rows = Vec::new();
    let mut first_error = None;
    let mut read_any = false;
    for db in &dbs {
        match questkeep_store_api::sqlite::read_foreign(db, "Meta Horizon Link cache", read_rows) {
            Ok(r) => {
                read_any = true;
                rows.extend(r);
            }
            Err(err) => {
                tracing::warn!(%err, "could not read a Meta Horizon Link cache");
                first_error.get_or_insert(err);
            }
        }
    }
    match (read_any, first_error) {
        (false, Some(err)) => Err(err),
        _ => Ok(from_rows(rows)),
    }
}

/// Values used by tests of the whole provider.
#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::blob::Value;
    use crate::blob::encode::{encode, image, object, r, s};

    pub fn row(o: &Object) -> Row {
        Row {
            key: o.str("id").unwrap_or_default().to_string(),
            type_name: o.type_name.clone(),
            value: encode(o),
        }
    }

    pub fn entitlement(user: &str, app: &str) -> Object {
        object(
            "Entitlement",
            vec![
                ("active_state", s("PERMANENT")),
                ("id", s(&format!("{user}:{app}"))),
                ("is_active", Value::Bool(true)),
                ("item", r("Application", app)),
            ],
        )
    }

    /// A full app object as the PC app caches it.
    pub fn pc_app(id: &str, name: &str, canonical: &str) -> Object {
        object(
            "Application",
            vec![
                ("canonical_name", s(canonical)),
                ("category", s("GAMES")),
                (
                    "cover_landscape_image",
                    image(&format!("https://cdn.invalid/{id}/wide.png")),
                ),
                (
                    "cover_square_image",
                    image(&format!("https://cdn.invalid/{id}/square.png")),
                ),
                ("display_long_description", s("Grow a garden on the moon.")),
                ("display_name", s(name)),
                (
                    "genres",
                    Value::Strs(vec!["ADVENTURE".into(), "CASUAL".into()]),
                ),
                (
                    "icon_image",
                    image(&format!("https://cdn.invalid/{id}/icon.png")),
                ),
                ("id", s(id)),
                ("latest_supported_binary", r("PCBinary", "900")),
                (
                    "organization",
                    Value::Object(object("Organization", vec![("name", s("Moonlit Studio"))])),
                ),
                ("quality_rating_aggregate", Value::Float(4.5)),
            ],
        )
    }

    /// The short app object of the user's session (with a platform).
    pub fn short_app(id: &str, name: &str, platform: &str) -> Object {
        object(
            "Application",
            vec![
                ("canonical_name", s(&name.to_lowercase().replace(' ', "-"))),
                ("display_name", s(name)),
                ("id", s(id)),
                ("platform", s(platform)),
            ],
        )
    }

    pub fn offline_data(user: &str) -> Object {
        object(
            "OafOfflineData",
            vec![
                ("id", s("__OAF_OFFLINE_DATA_KEY__")),
                (
                    "last_logged_in_viewer",
                    Value::Object(object("Viewer", vec![("user", r("User", user))])),
                ),
            ],
        )
    }

    pub fn playtime(user: &str, app: &str, seconds: i64) -> Object {
        object(
            "UserAppPlayTime",
            vec![
                ("id", s(&format!("{user}_{app}"))),
                ("play_time", Value::Int(seconds)),
            ],
        )
    }

    /// Writes `objects` as a cache database like the app's.
    pub fn write_db(path: &Path, objects: &[Object]) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE Objects (hashkey TEXT, typename TEXT, value BLOB, PRIMARY KEY(hashkey, typename));
             CREATE TABLE Version (major INTEGER, minor INTEGER, PRIMARY KEY(major, minor));",
        )
        .unwrap();
        for o in objects {
            let row = row(o);
            conn.execute(
                "INSERT INTO Objects VALUES (?1, ?2, ?3)",
                rusqlite::params![row.key, row.type_name, row.value],
            )
            .unwrap();
        }
        // Rows of other types are never read.
        conn.execute("INSERT INTO Objects VALUES ('x', 'RoomSolo', x'00')", [])
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::blob::Value;

    const USER: &str = "1000000000001";
    const OTHER: &str = "1000000000099";

    #[test]
    fn lists_owned_pc_apps_of_the_signed_in_user() {
        let rows = [
            offline_data(USER),
            pc_app(
                "2000000000000002",
                "Moon Garden",
                "moonlit-studio-moon-garden",
            ),
            entitlement(USER, "2000000000000002"),
            entitlement(USER, "2000000000000003"),
            // A Quest-only app the cache knows: left out.
            short_app("2000000000000005", "Pocket Quest", "ANDROID_6DOF"),
            entitlement(USER, "2000000000000005"),
            // Another account on this PC.
            entitlement(OTHER, "2000000000000006"),
            entitlement(USER, "2000000000000002"),
        ]
        .iter()
        .map(row)
        .collect::<Vec<_>>();
        let catalog = from_rows(rows);
        assert_eq!(catalog.user.as_deref(), Some(USER));
        assert_eq!(catalog.owned(), ["2000000000000002", "2000000000000003"]);
        let moon = &catalog.apps["2000000000000002"];
        assert_eq!(moon.name.as_deref(), Some("Moon Garden"));
        assert_eq!(moon.developer.as_deref(), Some("Moonlit Studio"));
        assert_eq!(moon.genres, ["ADVENTURE", "CASUAL"]);
        assert_eq!(moon.rating, Some(4.5));
        assert!(moon.pc_binary && moon.is_pc());
        assert_eq!(moon.is_app(), Some(false));
        assert_eq!(
            moon.images.cover_square.as_deref(),
            Some("https://cdn.invalid/2000000000000002/square.png")
        );
    }

    #[test]
    fn without_a_known_user_every_entitlement_counts() {
        let catalog = from_rows(
            [
                entitlement(USER, "2000000000000002"),
                entitlement(OTHER, "2000000000000006"),
            ]
            .iter()
            .map(row),
        );
        assert_eq!(catalog.owned(), ["2000000000000002", "2000000000000006"]);
    }

    #[test]
    fn merges_apps_from_several_caches_and_reads_playtime() {
        let mut short = short_app("2000000000000002", "Moon Garden", "PC");
        short.fields.retain(|(n, _)| n != "display_name");
        let catalog = from_rows(
            [
                short,
                pc_app(
                    "2000000000000002",
                    "Moon Garden",
                    "moonlit-studio-moon-garden",
                ),
                playtime(USER, "2000000000000002", 3600),
                playtime(USER, "F_Games_Custom_Thing", 99),
            ]
            .iter()
            .map(row),
        );
        let moon = &catalog.apps["2000000000000002"];
        assert_eq!(moon.platform.as_deref(), Some("PC"));
        assert_eq!(moon.name.as_deref(), Some("Moon Garden"));
        assert_eq!(moon.canonical_name.as_deref(), Some("moon-garden"));
        assert_eq!(catalog.playtime.len(), 1);
        assert_eq!(catalog.playtime["2000000000000002"], 3600);
    }

    #[test]
    fn skips_damaged_rows_and_inactive_entitlements() {
        let mut inactive = entitlement(USER, "2000000000000007");
        inactive.fields.retain(|(n, _)| n != "is_active");
        inactive
            .fields
            .push(("is_active".into(), Value::Bool(false)));
        let mut rows: Vec<Row> = [inactive, entitlement(USER, "2000000000000002")]
            .iter()
            .map(row)
            .collect();
        rows.push(Row {
            key: "broken".into(),
            type_name: "Entitlement".into(),
            value: vec![4, 0, 0],
        });
        assert_eq!(from_rows(rows).owned(), ["2000000000000002"]);
    }

    #[test]
    fn reads_every_session_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions = tmp.path().join("Sessions");
        write_db(
            &sessions.join("_oaf").join("data.sqlite"),
            &[offline_data(USER), entitlement(USER, "2000000000000002")],
        );
        write_db(
            &sessions.join(format!("{USER}-1")).join("data.sqlite"),
            &[short_app("2000000000000002", "Moon Garden", "PC")],
        );
        std::fs::create_dir_all(sessions.join("empty")).unwrap();
        let dbs = session_databases(&sessions);
        assert_eq!(dbs.len(), 2);
        assert!(dbs[0].starts_with(sessions.join("_oaf")));
        let catalog = read(&sessions).unwrap();
        assert_eq!(catalog.owned(), ["2000000000000002"]);
        assert_eq!(
            catalog.apps["2000000000000002"].name.as_deref(),
            Some("Moon Garden")
        );
        assert!(matches!(
            read(&tmp.path().join("nothing")),
            Err(ProviderError::NotConfigured(_))
        ));
    }
}
