//! Public, unauthenticated GOG catalogue API: one request to `https://api.gog.com/v2/games/<id>`
//! yields the description, release date, developers, publishers, genre tags and image URLs. The
//! parsing is separate from the fetching so it can be tested with fixture JSON.

use std::time::Duration;

use serde::Deserialize;

use super::super::ProviderError;
use crate::model::GameMetadata;

const API_BASE: &str = "https://api.gog.com/v2/games";

/// The fields of the v2 games response that Questkeep uses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct StoreGame {
    /// HTML description.
    #[serde(default)]
    overview: Option<String>,
    #[serde(default, rename = "_links")]
    links: Links,
    #[serde(default, rename = "_embedded")]
    embedded: Embedded,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Links {
    #[serde(default)]
    box_art_image: Option<Href>,
    #[serde(default)]
    background_image: Option<Href>,
    #[serde(default)]
    galaxy_background_image: Option<Href>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct Href {
    #[serde(default)]
    href: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct Embedded {
    #[serde(default)]
    product: Option<Product>,
    #[serde(default)]
    developers: Vec<Named>,
    #[serde(default)]
    publishers: Vec<Named>,
    #[serde(default)]
    tags: Vec<Named>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    #[serde(default)]
    global_release_date: Option<String>,
    #[serde(default)]
    gog_release_date: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
struct Named {
    #[serde(default)]
    name: String,
}

impl StoreGame {
    pub fn parse(json: &str) -> Result<Self, ProviderError> {
        serde_json::from_str(json)
            .map_err(|e| ProviderError::Parse(format!("unexpected GOG API response: {e}")))
    }

    pub fn metadata(&self) -> GameMetadata {
        let names = |list: &[Named]| -> Vec<String> {
            list.iter()
                .map(|n| n.name.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect()
        };
        let product = self.embedded.product.as_ref();
        let release_date = product
            .and_then(|p| p.global_release_date.as_deref())
            .and_then(normalize_date)
            .or_else(|| {
                product
                    .and_then(|p| p.gog_release_date.as_deref())
                    .and_then(normalize_date)
            });
        GameMetadata {
            description: self
                .overview
                .as_deref()
                .map(html_to_text)
                .filter(|d| !d.is_empty()),
            release_date,
            developers: names(&self.embedded.developers),
            publishers: names(&self.embedded.publishers),
            genres: names(&self.embedded.tags),
        }
    }

    /// Portrait box art.
    pub fn cover_urls(&self) -> Vec<String> {
        image_urls([self.links.box_art_image.as_ref()])
    }

    /// Wide background images, best first.
    pub fn background_urls(&self) -> Vec<String> {
        image_urls([
            self.links.background_image.as_ref(),
            self.links.galaxy_background_image.as_ref(),
        ])
    }
}

fn image_urls<const N: usize>(hrefs: [Option<&Href>; N]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for href in hrefs.into_iter().flatten() {
        if let Some(url) = normalize_url(&href.href)
            && !out.contains(&url)
        {
            out.push(url);
        }
    }
    out
}

/// GOG sometimes returns scheme-less `//host/path` URLs.
pub fn normalize_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        None
    } else if let Some(rest) = url.strip_prefix("//") {
        Some(format!("https://{rest}"))
    } else if url.starts_with("https://") || url.starts_with("http://") {
        Some(url.to_string())
    } else {
        None
    }
}

/// `2007-10-26T00:00:00+02:00` -> `2007-10-26`; anything that does not start with a date is
/// dropped.
fn normalize_date(value: &str) -> Option<String> {
    let date = value.trim().get(..10)?;
    let ok = date.char_indices().all(|(i, c)| match i {
        4 | 7 => c == '-',
        _ => c.is_ascii_digit(),
    });
    ok.then(|| date.to_string())
}

/// Only numeric product ids are valid; this also keeps arbitrary text out of request URLs.
pub fn is_product_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.chars().all(|c| c.is_ascii_digit())
}

pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(concat!("Questkeep/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Fetches a game from the public API. `Ok(None)` when GOG does not know the id (404).
pub fn fetch_game(http: &ureq::Agent, id: &str) -> Result<Option<StoreGame>, ProviderError> {
    if !is_product_id(id) {
        return Ok(None);
    }
    let url = format!("{API_BASE}/{id}?locale=en-US");
    let mut response = match http.get(&url).call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(err) => return Err(ProviderError::Network(err.to_string())),
    };
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    StoreGame::parse(&body).map(Some)
}

/// Converts the HTML of a store description to plain text: tags are removed, block elements
/// become line breaks, list items get a `- ` bullet, scripts/styles/videos are dropped, common
/// entities are decoded and whitespace is normalised.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        push_text(&mut out, &rest[..lt]);
        rest = &rest[lt..];
        let Some(gt) = rest.find('>') else {
            // A lone `<` with no closing `>`: treat the remainder as text.
            push_text(&mut out, rest);
            rest = "";
            break;
        };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match name.as_str() {
            "script" | "style" | "video" | "iframe" if !closing && !tag.ends_with('/') => {
                // Drop everything up to the matching closing tag (or the end of the input).
                let needle = format!("</{name}");
                rest = match rest.to_ascii_lowercase().find(&needle) {
                    Some(pos) => rest[pos..].find('>').map_or("", |g| &rest[pos + g + 1..]),
                    None => "",
                };
            }
            "li" if !closing => out.push_str("\n- "),
            "li" => {}
            "br" | "p" | "div" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "tr"
            | "table" | "blockquote" => out.push('\n'),
            _ => {}
        }
    }
    push_text(&mut out, rest);

    let decoded = decode_entities(&out);
    let mut lines: Vec<String> = Vec::new();
    for line in decoded.split('\n') {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() && lines.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Appends text with line breaks in the source treated as plain spaces (as HTML does).
fn push_text(out: &mut String, text: &str) {
    out.push_str(&text.replace(['\n', '\r', '\t'], " "));
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest[1..]
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| entity(&rest[1..1 + end]).map(|c| (c, end + 2)));
        match decoded {
            Some((c, consumed)) => {
                out.push(c);
                rest = &rest[consumed..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "ndash" => '\u{2013}',
        "mdash" => '\u{2014}',
        "hellip" => '\u{2026}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "trade" => '\u{2122}',
        "reg" => '\u{00ae}',
        "copy" => '\u{00a9}',
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)?
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/gog/v2_game.json");

    #[test]
    fn strips_html_to_readable_text() {
        let text = html_to_text(
            "<p>Hello&nbsp;<b>world</b> &amp; &lt;you&gt;&#33; &#x41;<br>\n<br>\nNext <em>line</em></p>\
             <script>var x = 1 < 2;</script><ul><li>One</li><li>Two</li></ul>",
        );
        assert_eq!(text, "Hello world & <you>! A\n\nNext line\n\n- One\n- Two");
    }

    #[test]
    fn html_edge_cases() {
        assert_eq!(html_to_text(""), "");
        assert_eq!(html_to_text("plain text"), "plain text");
        assert_eq!(
            html_to_text("a < b and 5 &unknown; c"),
            "a < b and 5 &unknown; c"
        );
        assert_eq!(html_to_text("<p>unclosed <b"), "unclosed <b");
        assert_eq!(html_to_text("<video><source src=x></video>kept"), "kept");
        assert_eq!(html_to_text("<p> \n </p>"), "");
    }

    #[test]
    fn maps_v2_fixture_to_metadata() {
        let game = StoreGame::parse(FIXTURE).unwrap();
        let meta = game.metadata();
        assert_eq!(
            meta.description.as_deref(),
            Some(
                "The Witcher is a role-playing game set in a dark fantasy world.\n\n\
                 Become Geralt & make choices.\n\nKEY FEATURES\n\n\
                 - Choose from over 250 abilities.\n- Three endings \u{2014} each different."
            )
        );
        assert_eq!(meta.release_date.as_deref(), Some("2007-10-26"));
        assert_eq!(meta.developers, ["CD PROJEKT RED"]);
        assert_eq!(meta.publishers, ["CD PROJEKT RED"]);
        assert_eq!(meta.genres, ["Role-playing", "Action", "Fantasy"]);
    }

    #[test]
    fn maps_v2_fixture_to_images() {
        let game = StoreGame::parse(FIXTURE).unwrap();
        let covers = game.cover_urls();
        assert_eq!(covers.len(), 1);
        assert!(covers[0].starts_with("https://images.gog-statics.com/32385c"));
        let backgrounds = game.background_urls();
        assert_eq!(backgrounds.len(), 2);
        assert!(backgrounds[0].contains("c73009d1"));
    }

    #[test]
    fn sparse_or_odd_responses_do_not_fail() {
        let game = StoreGame::parse("{}").unwrap();
        assert_eq!(game.metadata(), GameMetadata::default());
        assert!(game.cover_urls().is_empty());
        let game = StoreGame::parse(
            r#"{"overview":null,"_embedded":{"product":{"globalReleaseDate":"soon","gogReleaseDate":"2011-05-09T23:00:00+02:00"}}}"#,
        )
        .unwrap();
        assert_eq!(game.metadata().release_date.as_deref(), Some("2011-05-09"));
        assert!(matches!(
            StoreGame::parse("<html>"),
            Err(ProviderError::Parse(_))
        ));
    }

    #[test]
    fn url_and_id_helpers() {
        assert_eq!(
            normalize_url("//images-2.gog-statics.com/a.jpg").as_deref(),
            Some("https://images-2.gog-statics.com/a.jpg")
        );
        assert_eq!(normalize_url("https://x/y").as_deref(), Some("https://x/y"));
        assert_eq!(normalize_url("javascript:alert(1)"), None);
        assert_eq!(normalize_url(" "), None);
        assert!(is_product_id("1207658924"));
        assert!(!is_product_id("12/../3"));
        assert!(!is_product_id(""));
        assert_eq!(normalize_date("2007"), None);
    }

    #[test]
    fn invalid_ids_never_reach_the_network() {
        let http = http_agent();
        assert!(matches!(fetch_game(&http, "../x"), Ok(None)));
    }
}
