//! Hands the files of installed plugins to the main window: extension scripts, styles and
//! images load from `qkplugin://localhost/<plugin id>/<path>` (`http://qkplugin.localhost/...`
//! on Windows). Only files inside an enabled plugin's installed copy are served.

use std::sync::Arc;

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, UriSchemeContext};

use crate::AppState;

/// The URI scheme; the CSP in tauri.conf.json allows it.
pub const SCHEME: &str = "qkplugin";

pub fn serve(
    ctx: UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    // Store sign-in windows show foreign pages: they get nothing.
    if ctx.webview_label() != crate::window::MAIN_WINDOW {
        return status(StatusCode::FORBIDDEN);
    }
    let Some(state) = ctx.app_handle().try_state::<Arc<AppState>>() else {
        return status(StatusCode::SERVICE_UNAVAILABLE);
    };
    let Some((id, relative)) = split_path(request.uri().path()) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Some(file) = state.plugins.file(&id, &relative) else {
        return status(StatusCode::NOT_FOUND);
    };
    match std::fs::read(&file) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type(&relative))
            // The page (another origin) imports these as modules.
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            // A reloaded plugin must never be answered from a cache.
            .header(header::CACHE_CONTROL, "no-store")
            .header("X-Content-Type-Options", "nosniff")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(err) => {
            tracing::warn!(%err, file = %file.display(), "could not read a plugin file");
            status(StatusCode::NOT_FOUND)
        }
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = code;
    response
}

/// `/<plugin id>/<path>` as (id, path), percent-decoded. `None` when either part is missing.
fn split_path(path: &str) -> Option<(String, String)> {
    let decoded = percent_decode(path)?;
    let (id, relative) = decoded.trim_start_matches('/').split_once('/')?;
    (!id.is_empty() && !relative.is_empty()).then(|| (id.to_string(), relative.to_string()))
}

/// Decodes `%XX`; `None` for broken escapes or text that is not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The media type by file extension. HTML is handed out as plain text: plugin pages are built by
/// their script, never navigated to.
fn content_type(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "txt" | "md" | "html" | "htm" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_decodes_the_request_path() {
        assert_eq!(
            split_path("/my-plugin/ui/main.js"),
            Some(("my-plugin".into(), "ui/main.js".into()))
        );
        assert_eq!(
            split_path("/p/img/box%20art.png"),
            Some(("p".into(), "img/box art.png".into()))
        );
        // Decoded dots reach the plugin manager, which refuses to leave the plugin folder.
        assert_eq!(
            split_path("/p/%2e%2e/secret"),
            Some(("p".into(), "../secret".into()))
        );
        for bad in ["", "/", "/p", "/p/", "//x", "/p/%zz", "/p/%f", "/p/%ff%fe"] {
            assert_eq!(split_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn media_types() {
        assert!(content_type("main.JS").starts_with("text/javascript"));
        assert!(content_type("a/b/style.css").starts_with("text/css"));
        assert_eq!(content_type("icon.svg"), "image/svg+xml");
        assert!(content_type("page.html").starts_with("text/plain"));
        assert_eq!(content_type("noextension"), "application/octet-stream");
    }
}
