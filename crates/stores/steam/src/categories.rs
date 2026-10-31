//! The user's Steam collections, as categories.
//!
//! Current Steam keeps them in `userdata/<accountid>/config/cloudstorage/cloud-storage-namespace-1.json`:
//! an array of `[key, {key, timestamp, value, is_deleted?, ...}]`. Entries keyed
//! `user-collections.<id>` carry a JSON string `{"id","name","added":[appids],"removed":[...]}`;
//! dynamic collections have a `filterSpec` instead of fixed members and are skipped, as are the
//! built-in `favorite` and `hidden` collections. Confirmed on a real install (2026-10-25).
//! Older Steam kept tags in `userdata/<accountid>/7/remote/sharedconfig.vdf` under
//! `UserRoamingConfigStore/Software/Valve/Steam/apps/<appid>/tags/{"0": "name", ...}`; that is the
//! fallback when the JSON file doesn't exist (format from documentation, not seen on real data).

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::local;
use super::vdf;

type Categories = HashMap<String, Vec<String>>;

fn add(map: &mut Categories, app_id: &str, name: &str) {
    let name = name.trim();
    let app_id = app_id.trim();
    if name.is_empty() || app_id.is_empty() {
        return;
    }
    let list = map.entry(app_id.to_string()).or_default();
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
}

/// Members and name of one static `user-collections.*` entry, if it is one.
fn static_collection(entry: &Value) -> Option<(String, Vec<String>)> {
    let pair = entry.as_array()?;
    let key = pair.first()?.as_str()?;
    let meta = pair.get(1)?;
    let id = key.strip_prefix("user-collections.")?;
    if id == "favorite" || id == "hidden" {
        return None;
    }
    if meta.get("is_deleted").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let collection: Value = serde_json::from_str(meta.get("value")?.as_str()?).ok()?;
    if collection.get("filterSpec").is_some_and(|f| !f.is_null()) {
        return None;
    }
    let name = collection.get("name")?.as_str()?.to_string();
    let ids = |field: &str| -> Vec<String> {
        collection
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|app| match app {
                Value::Number(n) => Some(n.to_string()),
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    };
    // Members are the apps added and not removed again.
    let removed = ids("removed");
    let apps = ids("added")
        .into_iter()
        .filter(|app| !removed.contains(app))
        .collect();
    Some((name, apps))
}

/// App id -> collection names from a `cloud-storage-namespace-1.json` document; `None` when the
/// text is not the expected array.
pub fn parse_cloud_storage(text: &str) -> Option<Categories> {
    let doc: Value = serde_json::from_str(text).ok()?;
    let mut map = Categories::new();
    for entry in doc.as_array()? {
        if let Some((name, apps)) = static_collection(entry) {
            for app in apps {
                add(&mut map, &app, &name);
            }
        }
    }
    Some(map)
}

/// App id -> tag names from a `sharedconfig.vdf` document.
pub fn parse_shared_config(text: &str) -> Option<Categories> {
    let (_, doc) = vdf::parse(text).ok()?;
    // The parser returns the contents of the root object (`UserRoamingConfigStore`).
    let apps = ["Software", "Valve", "Steam", "apps"]
        .iter()
        .try_fold(&doc, |node, key| node.get(key));
    let mut map = Categories::new();
    let Some(apps) = apps else { return Some(map) };
    for (app_id, app) in apps.entries() {
        let Some(tags) = app.get("tags") else {
            continue;
        };
        for (_, tag) in tags.entries() {
            if let Some(name) = tag.as_str() {
                add(&mut map, app_id, name);
            }
        }
    }
    Some(map)
}

/// The `userdata/<accountid>` folders to read: the most recent login's when it exists, else all.
fn user_dirs(root: &Path) -> Vec<PathBuf> {
    let userdata = root.join("userdata");
    if let Some(account) = local::most_recent_steam_id(root).and_then(|id| local::account_id(&id)) {
        let dir = userdata.join(account.to_string());
        if dir.is_dir() {
            return vec![dir];
        }
    }
    let Ok(read) = std::fs::read_dir(&userdata) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    dirs.sort();
    dirs
}

fn read_parsed(path: &Path, parse: fn(&str) -> Option<Categories>) -> Option<Categories> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("could not read {}: {e}", path.display());
            return None;
        }
    };
    let parsed = parse(&String::from_utf8_lossy(&bytes));
    if parsed.is_none() {
        tracing::warn!("could not parse {}", path.display());
    }
    parsed
}

/// Categories of one Steam root, merged over the selected users. `None` when no readable
/// collection or tag file exists.
pub fn read_root(root: &Path) -> Option<Categories> {
    let mut merged: Option<Categories> = None;
    for dir in user_dirs(root) {
        let json = dir
            .join("config")
            .join("cloudstorage")
            .join("cloud-storage-namespace-1.json");
        let shared = dir.join("7").join("remote").join("sharedconfig.vdf");
        let parsed = if json.is_file() {
            read_parsed(&json, parse_cloud_storage)
        } else if shared.is_file() {
            read_parsed(&shared, parse_shared_config)
        } else {
            None
        };
        let Some(parsed) = parsed else { continue };
        let target = merged.get_or_insert_with(Categories::new);
        for (app, names) in parsed {
            for name in names {
                add(target, &app, &name);
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection(id: &str, body: &str) -> String {
        let value = body.replace('"', "\\\"");
        format!(
            r#"["user-collections.{id}", {{"key":"user-collections.{id}","timestamp":1,"value":"{value}","version":"1"}}]"#
        )
    }

    fn json_doc() -> String {
        let deleted = r#"["user-collections.gone", {"key":"user-collections.gone","timestamp":2,"is_deleted":true}]"#;
        let items = [
            collection(
                "uc-1",
                r#"{"id":"uc-1","name":" RPG ","added":[10,20,60],"removed":[60]}"#,
            ),
            collection(
                "from-tag-2",
                r#"{"id":"from-tag-2","name":"Done","added":[10],"removed":[]}"#,
            ),
            collection(
                "uc-dup",
                r#"{"id":"uc-dup","name":"RPG","added":[10],"removed":[]}"#,
            ),
            collection(
                "uc-dyn",
                r#"{"id":"uc-dyn","name":"Dynamic","added":[50],"removed":[],"filterSpec":{"nFormatVersion":2}}"#,
            ),
            collection(
                "favorite",
                r#"{"id":"favorite","name":"Favorites","added":[10],"removed":[]}"#,
            ),
            collection(
                "hidden",
                r#"{"id":"hidden","name":"Hidden","added":[30],"removed":[]}"#,
            ),
            collection(
                "uc-empty",
                r#"{"id":"uc-empty","name":"  ","added":[40],"removed":[]}"#,
            ),
            r#"["other.key", {"key":"other.key","value":"x"}]"#.to_string(),
            deleted.to_string(),
        ];
        format!("[{}]", items.join(","))
    }

    #[test]
    fn collections_from_cloud_storage() {
        let map = parse_cloud_storage(&json_doc()).unwrap();
        assert_eq!(map["10"], vec!["RPG", "Done"]);
        assert_eq!(map["20"], vec!["RPG"]);
        assert_eq!(map.len(), 2, "favorite, hidden, dynamic, deleted skipped");
        assert!(parse_cloud_storage("not json").is_none());
    }

    const SHARED: &str = r#""UserRoamingConfigStore" { "Software" { "Valve" { "Steam" { "apps" {
        "10" { "tags" { "0" "Co-op" "1" "Old Favs" } }
        "20" { "LastPlayed" "5" }
        "30" { "tags" { "0" "Co-op" "1" "Co-op" } }
    } } } } }"#;

    #[test]
    fn tags_from_shared_config() {
        let map = parse_shared_config(SHARED).unwrap();
        assert_eq!(map["10"], vec!["Co-op", "Old Favs"]);
        assert_eq!(map["30"], vec!["Co-op"]);
        assert!(!map.contains_key("20"));
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn json_wins_over_shared_config_and_users_are_merged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert!(read_root(root).is_none(), "no userdata");
        write(
            &root.join("userdata/111/config/cloudstorage/cloud-storage-namespace-1.json"),
            &json_doc(),
        );
        write(&root.join("userdata/111/7/remote/sharedconfig.vdf"), SHARED);
        write(&root.join("userdata/222/7/remote/sharedconfig.vdf"), SHARED);
        write(&root.join("userdata/notauser/x"), "");
        let map = read_root(root).unwrap();
        assert_eq!(map["10"], vec!["RPG", "Done", "Co-op", "Old Favs"]);
        assert_eq!(map["30"], vec!["Co-op"], "only from user 222's tags");
    }

    #[test]
    fn most_recent_user_is_preferred() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // 76561197960265728 + 111
        write(
            &root.join("config/loginusers.vdf"),
            r#""users" { "76561197960265839" { "MostRecent" "1" "Timestamp" "5" } }"#,
        );
        write(&root.join("userdata/111/7/remote/sharedconfig.vdf"), SHARED);
        write(
            &root.join("userdata/222/config/cloudstorage/cloud-storage-namespace-1.json"),
            &json_doc(),
        );
        let map = read_root(root).unwrap();
        assert_eq!(map["10"], vec!["Co-op", "Old Favs"]);
    }

    #[test]
    fn broken_files_give_none() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path()
                .join("userdata/1/config/cloudstorage/cloud-storage-namespace-1.json"),
            "{{{",
        );
        assert!(read_root(tmp.path()).is_none());
    }
}
