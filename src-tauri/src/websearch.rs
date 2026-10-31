//! Web image search for the "Pick image..." dialog: DuckDuckGo image search in a hidden window.
//!
//! A hidden webview with its own profile (never a store's, so it cannot see store sign-ins) opens
//! DuckDuckGo's image results. A script on the page reads DuckDuckGo's own `i.js` JSON answer and
//! reports it by navigating to a capture address, which `on_navigation` cancels and hands over.
//! The window never shows or takes focus: the user may be playing a fullscreen game.
// Not wired to a command yet.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

/// How long the hidden window may take to deliver results.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The page script reports to this address (a reserved TLD that never resolves: the navigation is
/// cancelled).
const CAPTURE_PREFIX: &str = "https://questkeep.invalid/websearch#";

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebImage {
    pub url: String,
    pub thumbnail: String,
    pub width: u32,
    pub height: u32,
    pub title: String,
    /// Domain of the page the image was found on, e.g. `steamgriddb.com`.
    pub site: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebImageSize {
    Any,
    Large,
}

static BUSY: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Searches DuckDuckGo for images. Blocking (up to ~20 s): call it from a worker thread.
pub fn search_images(
    app: &AppHandle,
    profile_dir: &Path,
    query: &str,
    size: WebImageSize,
    transparent: bool,
    limit: usize,
) -> Result<Vec<WebImage>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let _busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    let url = start_url(query, size, transparent)?;
    let (tx, rx) = mpsc::channel::<String>();
    let label = format!("websearch-{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title("Questkeep image search")
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        // Hidden pages are throttled otherwise, which would stall the page script.
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .data_directory(profile_dir.to_path_buf())
        .initialization_script(init_script(query, size, transparent))
        .on_navigation(move |url| {
            if let Some(payload) = url.as_str().strip_prefix(CAPTURE_PREFIX) {
                let _ = tx.send(percent_decode(payload));
                return false;
            }
            // Only DuckDuckGo may load; redirects elsewhere are refused.
            url.scheme() == "https"
                && url
                    .host_str()
                    .is_some_and(|h| h == "duckduckgo.com" || h.ends_with(".duckduckgo.com"))
        })
        .build()
        .map_err(|e| {
            tracing::warn!(error = %e, "could not open the image search window");
            format!("Could not start the image search: {e}")
        })?;
    let result = rx.recv_timeout(TIMEOUT);
    let _ = window.destroy();
    match result {
        Ok(text) => match text.strip_prefix("error:") {
            Some(message) => Err(format!("Image search failed: {message}")),
            None => parse_results(&text, limit),
        },
        Err(_) => Err("The image search did not answer in time.".into()),
    }
}

/// The DuckDuckGo images page for a query. `iaf` combines filters like DuckDuckGo does.
fn start_url(query: &str, size: WebImageSize, transparent: bool) -> Result<Url, String> {
    let mut url: Url = "https://duckduckgo.com/"
        .parse()
        .map_err(|_| "The search address is invalid.".to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", query);
        pairs.append_pair("iax", "images");
        pairs.append_pair("ia", "images");
        let mut filters = Vec::new();
        if size == WebImageSize::Large {
            filters.push("size:Large");
        }
        if transparent {
            filters.push("type:transparent");
        }
        if !filters.is_empty() {
            pairs.append_pair("iaf", &filters.join(","));
        }
    }
    Ok(url)
}

/// The `f` parameter of `i.js`: `,size,,,type,` (empty slots for unused filters).
fn i_js_filter(size: WebImageSize, transparent: bool) -> String {
    let size = if size == WebImageSize::Large {
        "size:Large"
    } else {
        ""
    };
    let kind = if transparent { "type:transparent" } else { "" };
    format!(",{size},,,{kind},")
}

fn init_script(query: &str, size: WebImageSize, transparent: bool) -> String {
    let q = serde_json::to_string(query).unwrap_or_else(|_| "\"\"".into());
    let f =
        serde_json::to_string(&i_js_filter(size, transparent)).unwrap_or_else(|_| "\"\"".into());
    SCRIPT.replace("__QUERY__", &q).replace("__FILTER__", &f)
}

/// Runs on every page of the window; only the top-level duckduckgo.com page acts, once.
///
/// Two ways to the data: the page asks `i.js` itself (with its own vqd token and bot-check
/// answers) and we watch those answers by wrapping `fetch` and `XMLHttpRequest`; if it has not
/// produced a usable answer after a few seconds, the script finds the `vqd` token and calls
/// `i.js` itself.
const SCRIPT: &str = r#"
(function () {
  if (window.top !== window || location.hostname !== 'duckduckgo.com') return;
  if (window.__qkSearch) return;
  window.__qkSearch = true;
  var QUERY = __QUERY__, FILTER = __FILTER__;
  var done = false;
  function finish(text) {
    if (done) return;
    done = true;
    location.href = 'https://questkeep.invalid/websearch#' + encodeURIComponent(text);
  }
  function fail(message) { finish('error:' + message); }
  function slim(data) {
    var out = [];
    (data.results || []).slice(0, 100).forEach(function (r) {
      out.push({ image: r.image, thumbnail: r.thumbnail, width: r.width, height: r.height, title: r.title, url: r.url });
    });
    return JSON.stringify({ results: out });
  }
  function offer(text) {
    try {
      var data = JSON.parse(text);
      if (data && Array.isArray(data.results)) { finish(slim(data)); return true; }
    } catch (e) {}
    return false;
  }
  function isImages(url) { return String(url || '').indexOf('/i.js') !== -1; }
  var origFetch = window.fetch;
  if (origFetch) {
    window.fetch = function (input) {
      var url = typeof input === 'string' ? input : input && input.url;
      var p = origFetch.apply(this, arguments);
      if (isImages(url)) {
        p.then(function (res) { return res.clone().text(); }).then(offer).catch(function () {});
      }
      return p;
    };
  }
  var origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url) {
    if (isImages(url)) {
      this.addEventListener('load', function () { try { offer(this.responseText); } catch (e) {} });
    }
    return origOpen.apply(this, arguments);
  };

  function findVqd() {
    try { if (window.DDG && DDG.deep && DDG.deep.vqd) return DDG.deep.vqd; } catch (e) {}
    var input = document.querySelector('input[name="vqd"]');
    if (input && input.value) return input.value;
    var html = document.documentElement ? document.documentElement.innerHTML : '';
    var m = /vqd=["']?(\d-[\d-]+)/.exec(html) || /vqd["']?\s*[:=]\s*["'](\d-[\d-]+)/.exec(html);
    return m ? m[1] : null;
  }
  function own(attempt) {
    if (done) return;
    var vqd = findVqd();
    if (!vqd) {
      if (attempt < 10) return setTimeout(function () { own(attempt + 1); }, 500);
      return fail('DuckDuckGo gave no search token');
    }
    var url = '/i.js?l=wt-wt&o=json&q=' + encodeURIComponent(QUERY) + '&vqd=' + encodeURIComponent(vqd) +
      '&f=' + encodeURIComponent(FILTER).replace(/%2C/g, ',') + '&p=1';
    (origFetch || fetch).call(window, url, { credentials: 'include', headers: { 'Accept': 'application/json' } })
      .then(function (res) { return res.text(); })
      .then(function (text) {
        if (done) return;
        if (!offer(text)) {
          var challenge = /botnet_challenge|"challenge"/.test(text);
          fail(challenge ? 'DuckDuckGo asked for a bot check' : 'unexpected answer from DuckDuckGo');
        }
      })
      .catch(function (e) { fail(String(e && e.message || e)); });
  }
  // Give the page 6 s to fetch its own results before asking ourselves.
  setTimeout(function () { own(0); }, 6000);
})();
"#;

/// Parses DuckDuckGo's `i.js` JSON. Keeps the order; drops results without a usable image or
/// thumbnail address.
pub fn parse_results(json: &str, limit: usize) -> Result<Vec<WebImage>, String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| "DuckDuckGo sent an unreadable answer.".to_string())?;
    let items = value
        .get("results")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "DuckDuckGo sent no results.".to_string())?;
    let mut found = Vec::new();
    for item in items {
        if found.len() >= limit {
            break;
        }
        let text = |key: &str| item.get(key).and_then(|v| v.as_str()).unwrap_or("").trim();
        let (image, thumbnail) = (text("image"), text("thumbnail"));
        let Some(image_url) = safe_url(image) else {
            continue;
        };
        if safe_url(thumbnail).is_none() {
            continue;
        }
        let site = Url::parse(text("url"))
            .ok()
            .and_then(|u| host_without_www(&u))
            .or_else(|| host_without_www(&image_url))
            .unwrap_or_default();
        found.push(WebImage {
            url: image.to_string(),
            thumbnail: thumbnail.to_string(),
            width: number(item.get("width")),
            height: number(item.get("height")),
            title: text("title").to_string(),
            site,
        });
    }
    Ok(found)
}

/// An http(s) address with a host and no whitespace.
fn safe_url(text: &str) -> Option<Url> {
    if text.is_empty() || text.len() > 2048 || text.chars().any(char::is_whitespace) {
        return None;
    }
    let url = Url::parse(text).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some()).then_some(url)
}

fn host_without_www(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    Some(host.strip_prefix("www.").unwrap_or(host).to_string())
}

/// Numbers arrive as numbers or digit strings; anything else is 0.
fn number(value: Option<&serde_json::Value>) -> u32 {
    match value {
        Some(serde_json::Value::Number(n)) => {
            n.as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0)
        }
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// Decodes `%XX` escapes (the page used `encodeURIComponent`).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[i + 1..i + 3]).ok())
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/ddg_images.json");

    #[test]
    fn parses_realistic_results() {
        let found = parse_results(FIXTURE, 10).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].site, "steamgriddb.com");
        assert_eq!(found[0].width, 600);
        assert_eq!(found[0].title, "Hades - SteamGridDB");
        assert_eq!(found[1].site, "store.steampowered.com");
        // No page url: falls back to the image host; string width parses, null height is 0.
        assert_eq!(found[2].site, "pics.example.net");
        assert_eq!((found[2].width, found[2].height), (800, 0));
    }

    #[test]
    fn respects_limit_and_rejects_bad_json() {
        assert_eq!(parse_results(FIXTURE, 1).unwrap().len(), 1);
        assert!(parse_results("not json", 5).is_err());
        assert!(parse_results("{\"challenge\":{}}", 5).is_err());
    }

    #[test]
    fn start_url_encodes_query_and_filters() {
        let url = start_url("hades & co/ü", WebImageSize::Any, false).unwrap();
        assert_eq!(
            url.as_str(),
            "https://duckduckgo.com/?q=hades+%26+co%2F%C3%BC&iax=images&ia=images"
        );
        let url = start_url("x", WebImageSize::Large, true).unwrap();
        assert!(
            url.as_str()
                .ends_with("&iaf=size%3ALarge%2Ctype%3Atransparent")
        );
        let url = start_url("x", WebImageSize::Any, true).unwrap();
        assert!(url.as_str().ends_with("&iaf=type%3Atransparent"));
    }

    #[test]
    fn i_js_filter_slots() {
        assert_eq!(i_js_filter(WebImageSize::Any, false), ",,,,,");
        assert_eq!(
            i_js_filter(WebImageSize::Large, true),
            ",size:Large,,,type:transparent,"
        );
    }

    #[test]
    fn decodes_capture_payload() {
        assert_eq!(
            percent_decode("%7B%22a%22%3A%22%C3%BC%22%7D"),
            "{\"a\":\"ü\"}"
        );
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn script_embeds_query_safely() {
        let s = init_script("a\"b", WebImageSize::Any, false);
        assert!(s.contains("a\\\"b"));
        assert!(!s.contains("__QUERY__") && !s.contains("__FILTER__"));
    }
}
