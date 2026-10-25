//! Reader for the launcher's product configuration cache,
//! `%LOCALAPPDATA%\Ubisoft Game Launcher\cache\configuration\configurations`.
//!
//! The file is a protobuf message: a repeated field 1 of `{ 1: uplay id (varint), 2: install id
//! (varint), 3: YAML text }`. It describes every product the launcher knows about (836 on the
//! machine it was inspected on, including DLC and games the user does not own), so it is used
//! for names and artwork only, never as a list of owned games. The YAML is read with a small
//! line scanner for the handful of scalar keys needed instead of a YAML dependency.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

use std::collections::HashMap;

/// Where the launcher's artwork lives; the configs only carry file names.
pub const ASSET_URL_BASE: &str =
    "https://ubistatic3-a.akamaihd.net/orbit/uplay_launcher_3_0/assets/";

/// What the configuration cache knows about one product.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Product {
    pub id: u32,
    pub name: Option<String>,
    /// Image URLs (asset base plus file name, localisation resolved).
    pub thumb_image: Option<String>,
    pub background_image: Option<String>,
    pub icon_image: Option<String>,
    /// `root.start_game` exists: the launcher can start it (DLC and bundles have none).
    pub startable: bool,
    /// `root.is_ulc`: unlockable content of another game.
    pub is_ulc: bool,
    /// `root.third_party_platform` exists: sold and started through another store (Steam).
    pub third_party: bool,
    /// Ids of `root.addons` (this product's DLC).
    pub addon_ids: Vec<u32>,
}

/// All products of a configurations file by Ubisoft product id. When an id appears more than once
/// the first entry that has a name wins.
pub fn parse_configurations(data: &[u8]) -> Result<HashMap<u32, Product>, String> {
    let mut products: HashMap<u32, Product> = HashMap::new();
    for entry in entries(data)? {
        let Some(yaml) = entry.yaml.as_deref() else {
            continue;
        };
        let product = parse_product(entry.uplay_id, yaml);
        match products.get(&entry.uplay_id) {
            Some(existing) if existing.name.is_some() || product.name.is_none() => {}
            _ => {
                products.insert(entry.uplay_id, product);
            }
        }
    }
    Ok(products)
}

struct Entry {
    uplay_id: u32,
    yaml: Option<String>,
}

fn entries(data: &[u8]) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    let mut top = Reader::new(data);
    while !top.done() {
        let (field, wire) = top.tag()?;
        if (field, wire) != (1, 2) {
            top.skip(wire)?;
            continue;
        }
        let mut inner = Reader::new(top.bytes()?);
        let mut entry = Entry {
            uplay_id: 0,
            yaml: None,
        };
        while !inner.done() {
            let (field, wire) = inner.tag()?;
            match (field, wire) {
                (1, 0) => entry.uplay_id = u32::try_from(inner.varint()?).unwrap_or(0),
                (3, 2) => {
                    let text = String::from_utf8_lossy(inner.bytes()?).into_owned();
                    entry.yaml = (!text.trim().is_empty()).then_some(text);
                }
                _ => inner.skip(wire)?,
            }
        }
        out.push(entry);
    }
    Ok(out)
}

/// Minimal protobuf wire reader (varint and length-delimited fields).
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.data.get(self.pos).ok_or("truncated protobuf data")?;
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("invalid protobuf varint".into())
    }

    fn tag(&mut self) -> Result<(u64, u8), String> {
        let tag = self.varint()?;
        Ok((tag >> 3, (tag & 7) as u8))
    }

    fn bytes(&mut self) -> Result<&'a [u8], String> {
        let len = usize::try_from(self.varint()?).map_err(|_| "invalid protobuf length")?;
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.data.len())
            .ok_or("truncated protobuf data")?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn skip(&mut self, wire: u8) -> Result<(), String> {
        match wire {
            0 => self.varint().map(drop),
            1 => self.advance(8),
            2 => self.bytes().map(drop),
            5 => self.advance(4),
            _ => Err(format!("unsupported protobuf wire type {wire}")),
        }
    }

    fn advance(&mut self, n: usize) -> Result<(), String> {
        if self.pos + n > self.data.len() {
            return Err("truncated protobuf data".into());
        }
        self.pos += n;
        Ok(())
    }
}

/// Reads the product fields out of one config's YAML. Names and image files may be localisation
/// keys (`l1`) that `localizations.default` resolves.
pub fn parse_product(id: u32, yaml: &str) -> Product {
    let loc = default_localizations(yaml);
    let resolve = |value: String| loc.get(&value).cloned().unwrap_or(value);
    let RootInfo {
        scalars: root,
        keys,
        addon_ids,
    } = root_info(yaml);
    let yes = |key: &str| {
        root.get(key)
            .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "yes" | "true"))
    };
    let image = |key: &str| {
        root.get(key).map(|v| resolve(v.clone())).map(|file| {
            if file.starts_with("http") {
                file
            } else {
                format!("{ASSET_URL_BASE}{file}")
            }
        })
    };
    Product {
        id,
        name: root
            .get("name")
            .map(|v| resolve(v.clone()))
            .filter(|n| !n.is_empty()),
        thumb_image: image("thumb_image"),
        background_image: image("background_image"),
        icon_image: image("icon_image"),
        startable: keys.iter().any(|k| k == "start_game"),
        is_ulc: yes("is_ulc"),
        third_party: keys.iter().any(|k| k == "third_party_platform"),
        addon_ids,
    }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_blank(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#')
}

/// `key: value` of a YAML line (no leading indentation); `None` for list items and the like.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.trim().split_once(':')?;
    let valid = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    // `a: b` has a space after the colon; `a:b` would be part of a plain scalar.
    (valid && (value.is_empty() || value.starts_with(' '))).then(|| (key, value.trim()))
}

/// What the top-level `root:` mapping holds directly.
struct RootInfo {
    /// Children with a scalar value.
    scalars: HashMap<String, String>,
    /// Every child key, including those holding a mapping or list (`start_game:`).
    keys: Vec<String>,
    /// `id`s of the `addons:` list items.
    addon_ids: Vec<u32>,
}

fn root_info(yaml: &str) -> RootInfo {
    let mut info = RootInfo {
        scalars: HashMap::new(),
        keys: Vec::new(),
        addon_ids: Vec::new(),
    };
    let mut lines = yaml.lines().skip_while(|l| l.trim_end() != "root:");
    lines.next();
    let mut child_indent = None;
    let mut in_addons = false;
    for line in lines {
        if is_blank(line) {
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 {
            break;
        }
        let child_indent = *child_indent.get_or_insert(indent);
        let item = line.trim_start().strip_prefix("- ");
        // List items may sit at the key's own indentation (`addons:\n  - id: 96`).
        if indent > child_indent || (indent == child_indent && item.is_some()) {
            if in_addons
                && let Some(("id", value)) = item.and_then(key_value)
                && let Ok(id) = unquote(value).parse()
            {
                info.addon_ids.push(id);
            }
            continue;
        }
        in_addons = false;
        if let Some((key, value)) = key_value(line) {
            info.keys.push(key.to_string());
            if value.is_empty() {
                in_addons = key == "addons";
            } else {
                info.scalars.insert(key.to_string(), unquote(value));
            }
        }
    }
    info
}

/// The `localizations: default:` table (`l1: Some Name`).
fn default_localizations(yaml: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut lines = yaml
        .lines()
        .skip_while(|l| l.trim_end() != "localizations:");
    lines.next();
    let mut default_indent = None;
    for line in lines {
        if is_blank(line) {
            continue;
        }
        let indent = indent_of(line);
        match default_indent {
            None => {
                if indent == 0 {
                    break;
                }
                if key_value(line) == Some(("default", "")) {
                    default_indent = Some(indent);
                }
            }
            Some(parent) => {
                if indent <= parent {
                    break;
                }
                if let Some((key, value)) = key_value(line)
                    && !value.is_empty()
                {
                    out.insert(key.to_string(), unquote(value));
                }
            }
        }
    }
    out
}

/// The products of the cache that are games of their own, picked the way Playnite's Ubisoft
/// library does: the launcher can start them, and they are not DLC (another product's addon or
/// unlockable content) or sold through another store. The cache also holds products the user
/// only looked at, so this is a best guess at the library, not a list of owned games.
pub fn catalogue_games(products: &HashMap<u32, Product>) -> Vec<&Product> {
    let dlc: std::collections::HashSet<u32> = products
        .values()
        .flat_map(|p| p.addon_ids.iter().copied())
        .chain(products.values().filter(|p| p.is_ulc).map(|p| p.id))
        .collect();
    let mut games: Vec<&Product> = products
        .values()
        .filter(|p| p.startable && !p.is_ulc && !p.third_party && !dlc.contains(&p.id))
        .filter(|p| {
            p.name
                .as_deref()
                .is_some_and(|n| !n.trim().is_empty() && !super::install::is_placeholder(n))
        })
        .collect();
    games.sort_by_key(|p| p.id);
    games
}

/// Plain, single-quoted or double-quoted YAML scalar to its text.
fn unquote(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    } else if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        inner.replace("''", "'")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub const TRACKMANIA: &str = include_str!("../../../tests/fixtures/ubisoft/product-5595.yaml");
    pub const BROTHERHOOD: &str = include_str!("../../../tests/fixtures/ubisoft/product-26.yaml");
    pub const DIVISION: &str = include_str!("../../../tests/fixtures/ubisoft/product-568.yaml");

    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    /// Encodes a configurations file the way the launcher does.
    pub fn encode(entries: &[(u32, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (id, yaml) in entries {
            let mut inner = vec![0x08];
            varint(u64::from(*id), &mut inner);
            inner.push(0x10);
            varint(u64::from(*id), &mut inner);
            inner.push(0x1a);
            varint(yaml.len() as u64, &mut inner);
            inner.extend_from_slice(yaml.as_bytes());
            out.push(0x0a);
            varint(inner.len() as u64, &mut out);
            out.extend(inner);
        }
        out
    }

    #[test]
    fn plain_product_has_name_and_image_urls() {
        let p = parse_product(5595, TRACKMANIA);
        assert_eq!(p.name.as_deref(), Some("Trackmania"));
        assert_eq!(
            p.thumb_image.as_deref(),
            Some(&*format!(
                "{ASSET_URL_BASE}29af1426c8ea0ed58abcc96195c45ef7.jpg"
            ))
        );
        assert!(
            p.background_image
                .unwrap()
                .ends_with("6b7da0564c173293938428c11930afa0.jpg")
        );
        assert!(
            p.icon_image
                .unwrap()
                .ends_with("8231e71808581ef4d3499fe96ecdf921.ico")
        );
    }

    #[test]
    fn localised_name_with_comment_header_and_other_languages() {
        let p = parse_product(26, BROTHERHOOD);
        assert_eq!(p.name.as_deref(), Some("Assassin's Creed® Brotherhood"));
        assert!(
            p.thumb_image
                .unwrap()
                .ends_with("17319894db2463c9d82d789b3e46461f.jpg")
        );
    }

    #[test]
    fn reads_the_catalogue_flags() {
        let p = parse_product(5595, TRACKMANIA);
        assert!(p.startable && !p.is_ulc && !p.third_party && p.addon_ids.is_empty());
        // Both list layouts of the real cache: items at the key's indentation and deeper.
        let yaml = "root:\n  name: Base\n  addons:\n  - id: 96\n    is_visible: no\n    name: \"DLC: One\"\n  - id: '97'\n  start_game:\n    online:\n      executables:\n      - path: game.exe\n  third_party_platform:\n    name: Steam\n  is_ulc: yes\nlocalizations: {}\n";
        let p = parse_product(1, yaml);
        assert_eq!(p.addon_ids, [96, 97]);
        assert!(p.startable && p.third_party && p.is_ulc);
        let yaml = "root:\n  name: Base\n  addons:\n    - id: 912\n      thumb_image: x.jpg\n  icon_image: i.ico\n";
        let p = parse_product(2, yaml);
        assert_eq!(p.addon_ids, [912]);
        assert!(!p.startable);
        assert!(p.icon_image.unwrap().ends_with("i.ico"));
    }

    #[test]
    fn catalogue_keeps_startable_games_of_their_own() {
        let product = |id: u32, name: &str| Product {
            id,
            name: Some(name.into()),
            startable: true,
            ..Product::default()
        };
        let products: HashMap<u32, Product> = [
            Product {
                addon_ids: vec![3],
                ..product(1, "Base Game")
            },
            product(3, "Base Game - Season Pass"),
            Product {
                is_ulc: true,
                ..product(4, "Bonus")
            },
            Product {
                third_party: true,
                ..product(5, "Sold on Steam")
            },
            Product {
                startable: false,
                ..product(6, "Bundle")
            },
            product(7, "GAMENAME"),
            product(8, "Another Game"),
        ]
        .into_iter()
        .map(|p| (p.id, p))
        .collect();
        let ids: Vec<u32> = catalogue_games(&products).iter().map(|p| p.id).collect();
        assert_eq!(ids, [1, 8]);
    }

    #[test]
    fn localised_image_file_is_resolved() {
        let p = parse_product(568, DIVISION);
        assert_eq!(p.name.as_deref(), Some("Tom Clancy's The Division™"));
        assert_eq!(
            p.thumb_image.as_deref(),
            Some(&*format!(
                "{ASSET_URL_BASE}20245d0356a609a8f2968aec287443a3.jpg"
            ))
        );
    }

    #[test]
    fn nested_keys_do_not_leak_into_the_root_fields() {
        let yaml = "root:\n  name: Outer\n  start_game:\n    name: Inner\nlocalizations: {}\n";
        assert_eq!(parse_product(1, yaml).name.as_deref(), Some("Outer"));
    }

    #[test]
    fn quoted_names_are_unquoted() {
        let yaml = "root:\n  name: \"Far Cry® 3\"\n";
        assert_eq!(parse_product(1, yaml).name.as_deref(), Some("Far Cry® 3"));
        let yaml = "root:\n  name: 'It''s: here'\n";
        assert_eq!(parse_product(1, yaml).name.as_deref(), Some("It's: here"));
    }

    #[test]
    fn parses_the_protobuf_container() {
        let data = encode(&[(5595, TRACKMANIA), (26, BROTHERHOOD), (7, "")]);
        let products = parse_configurations(&data).unwrap();
        assert_eq!(products.len(), 2, "entries without YAML are skipped");
        assert_eq!(products[&5595].name.as_deref(), Some("Trackmania"));
        assert_eq!(
            products[&26].name.as_deref(),
            Some("Assassin's Creed® Brotherhood")
        );
    }

    #[test]
    fn truncated_data_is_an_error() {
        let data = encode(&[(5595, TRACKMANIA)]);
        assert!(parse_configurations(&data[..data.len() - 10]).is_err());
        assert!(parse_configurations(&[]).unwrap().is_empty());
    }
}
