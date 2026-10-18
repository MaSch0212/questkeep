//! The login window for store accounts (GOG, Epic).
//!
//! It is a separate window that loads the store's own login page. It has no IPC access: the
//! capability file only names the `main` window, and `lib.rs` rejects every command that does not
//! come from `main`. The window's navigations are watched instead: when the store redirects to
//! the page that carries the one-time code, the navigation is cancelled and the code is handed to
//! the core. The window runs in a private (non-persistent) browser profile, so no store session
//! outlives the sign-in.

use questkeep_core::auth::query_param;
use questkeep_core::providers::gog::LOGIN_SUCCESS_PREFIX;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Where Epic sends the code when it treats the window as its own launcher. We don't send the
/// launcher's user agent (with it, Epic rejects our redirect URL as "not known to the client"),
/// but if Epic ever redirects here, the code is in the URL and is caught the same way.
const EPIC_LAUNCHER_REDIRECT: &str = "https://localhost/launcher/authorized";
/// Epic's code page cannot be intercepted by URL (it answers with JSON), so a script reads the
/// JSON and navigates here; the navigation is then caught and cancelled.
const CAPTURE_PREFIX: &str = "https://questkeep.invalid/";
const EPIC_CAPTURE_URL: &str = "https://questkeep.invalid/epic-auth";
#[cfg(test)]
const EPIC_REDIRECT_PREFIX: &str = "https://www.epicgames.com/id/api/redirect";

/// Runs on every page of the Epic login window but only acts on the redirect page.
const EPIC_SCRIPT: &str = r#"
(function () {
  if (!location.href.startsWith("https://www.epicgames.com/id/api/redirect")) return;
  function capture() {
    try {
      var data = JSON.parse(document.body.innerText);
      if (data && data.authorizationCode) {
        location.href = "https://questkeep.invalid/epic-auth?code=" + encodeURIComponent(data.authorizationCode);
      }
    } catch (e) {}
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", capture);
  } else {
    capture();
  }
})();
"#;

/// How a login window ended.
#[derive(Debug)]
pub enum Outcome {
    /// The store handed out a one-time code.
    Code(String),
    /// The user closed the window.
    Closed,
}

/// What to do with a navigation of the login window.
#[derive(Debug, PartialEq, Eq)]
enum Navigation {
    Allow,
    Block,
    Capture(String),
}

fn classify(provider: &str, url: &str) -> Navigation {
    let captured = match provider {
        "gog" => url
            .starts_with(LOGIN_SUCCESS_PREFIX)
            .then(|| query_param(url, "code")),
        "epic" => (url.starts_with(EPIC_CAPTURE_URL) || url.starts_with(EPIC_LAUNCHER_REDIRECT))
            .then(|| query_param(url, "code")),
        _ => None,
    };
    if let Some(code) = captured {
        return match code.filter(|c| !c.is_empty()) {
            Some(code) => Navigation::Capture(code),
            None => Navigation::Block,
        };
    }
    if url.starts_with(CAPTURE_PREFIX) {
        return Navigation::Block;
    }
    if url.starts_with("https://") || url == "about:blank" {
        Navigation::Allow
    } else {
        Navigation::Block
    }
}

fn title(provider: &str) -> &'static str {
    match provider {
        "gog" => "Sign in to GOG",
        "epic" => "Sign in to Epic Games",
        _ => "Sign in",
    }
}

/// Opens the login window and waits until it yields a code or is closed. Only one window per
/// provider: asking again focuses the open one and reports an error.
pub async fn run(app: &AppHandle, provider: &str, login_url: &str) -> Result<Outcome, String> {
    let label = format!("signin-{provider}");
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.set_focus();
        return Err("A sign-in window for this store is already open.".into());
    }
    let url: Url = login_url
        .parse()
        .map_err(|_| "The sign-in address is invalid.".to_string())?;
    if url.scheme() != "https" {
        return Err("The sign-in address is not secure.".into());
    }

    let (tx, mut rx) = tauri::async_runtime::channel::<Outcome>(4);
    let nav_tx = tx.clone();
    let nav_provider = provider.to_string();
    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(title(provider))
        .inner_size(500.0, 700.0)
        .center()
        .incognito(true)
        .on_navigation(move |url| match classify(&nav_provider, url.as_str()) {
            Navigation::Allow => true,
            Navigation::Block => false,
            Navigation::Capture(code) => {
                let _ = nav_tx.try_send(Outcome::Code(code));
                false
            }
        });
    if provider == "epic" {
        builder = builder.initialization_script(EPIC_SCRIPT);
    }
    let window = builder.build().map_err(|e| {
        tracing::warn!(error = %e, "could not open the sign-in window");
        format!("Could not open the sign-in window: {e}")
    })?;
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let _ = tx.try_send(Outcome::Closed);
        }
    });

    let outcome = rx.recv().await.unwrap_or(Outcome::Closed);
    if matches!(outcome, Outcome::Code(_)) {
        let _ = window.destroy();
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gog_success_redirect_is_captured() {
        assert_eq!(
            classify(
                "gog",
                "https://embed.gog.com/on_login_success?origin=client&code=AbC123_xyz-9"
            ),
            Navigation::Capture("AbC123_xyz-9".into())
        );
        // No code in the redirect: cancel, never let the page load.
        assert_eq!(
            classify(
                "gog",
                "https://embed.gog.com/on_login_success?origin=client"
            ),
            Navigation::Block
        );
    }

    #[test]
    fn epic_capture_url_is_captured_and_only_for_epic() {
        assert_eq!(
            classify(
                "epic",
                "https://localhost/launcher/authorized?code=fedcba9876543210"
            ),
            Navigation::Capture("fedcba9876543210".into())
        );
        let url = "https://questkeep.invalid/epic-auth?code=0123456789abcdef";
        assert_eq!(
            classify("epic", url),
            Navigation::Capture("0123456789abcdef".into())
        );
        assert_eq!(classify("gog", url), Navigation::Block);
        assert_eq!(
            classify("epic", "https://questkeep.invalid/other?code=1"),
            Navigation::Block
        );
        assert_eq!(
            classify("epic", "https://questkeep.invalid/epic-auth"),
            Navigation::Block
        );
    }

    #[test]
    fn only_https_pages_load() {
        for url in [
            "https://www.epicgames.com/id/login?redirectUrl=x",
            "https://auth.gog.com/auth?client_id=1",
            "https://www.epicgames.com/id/api/redirect?clientId=1",
            "about:blank",
        ] {
            assert_eq!(classify("epic", url), Navigation::Allow, "{url}");
        }
        for url in [
            "http://auth.gog.com/auth",
            "file:///C:/secret.txt",
            "javascript:alert(1)",
            "data:text/html,hi",
            "questkeep://x",
            "ftp://x/y",
        ] {
            assert_eq!(classify("gog", url), Navigation::Block, "{url}");
        }
    }

    #[test]
    fn epic_script_only_acts_on_the_redirect_page() {
        assert!(EPIC_SCRIPT.contains(EPIC_REDIRECT_PREFIX));
        assert!(EPIC_SCRIPT.contains(EPIC_CAPTURE_URL));
    }
}
