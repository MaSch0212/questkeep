//! What the in-app login window of a store account looks like and which navigation ends it. The
//! app owns the window (a separate webview without IPC access); the store only describes it, so
//! the app holds no store-specific code.
//!
//! The window's navigations are watched: when the store redirects to the page that carries the
//! one-time code, the navigation is cancelled and the code goes to
//! [`AccountAuth::sign_in_with_code`](super::AccountAuth::sign_in_with_code). Stores without a code
//! for other applications (Steam, Battle.net, EA) sign in through their website session instead
//! ([`WebSessionPage`]): a script on the store's pages reports what Questkeep needs by navigating
//! to a capture address below [`CAPTURE_PREFIX`].

/// Addresses page scripts navigate to in order to hand something over. Never loaded.
pub const CAPTURE_PREFIX: &str = "https://questkeep.invalid/";

/// The placeholder in [`WebSessionPage::script`] replaced by `true` in the hidden window.
const REPORT_SIGNED_OUT: &str = "REPORT_SIGNED_OUT";

#[derive(Debug, Clone, Copy)]
pub struct LoginWindow {
    /// Window title, e.g. `Sign in to GOG`.
    pub title: &'static str,
    /// The window's user agent; `None` keeps the webview's own.
    pub user_agent: Option<&'static str>,
    /// Runs on every page of the login window.
    pub script: Option<&'static str>,
    /// Set for stores signed in through their website session, which is kept in a persistent
    /// browser profile. Otherwise the window uses a private profile that ends with the sign-in.
    pub web_session: Option<WebSessionPage>,
    /// Recognizes the navigation that carries the result. `Some(value)` cancels the navigation
    /// and hands `value` over; an empty value only cancels it (the store sent no code).
    pub capture: fn(&str) -> Option<String>,
}

/// A store website session: the page that renews it and the script that reports it.
#[derive(Debug, Clone, Copy)]
pub struct WebSessionPage {
    /// The store in messages, e.g. `The Steam store` (`… did not answer in time`).
    pub store_name: &'static str,
    /// The page a hidden window loads to read the session.
    pub url: &'static str,
    /// Runs on every page of the store's windows. `var reportSignedOut = REPORT_SIGNED_OUT;`
    /// becomes `true` in the hidden window, which must also learn about a signed-out session.
    pub script: &'static str,
}

impl WebSessionPage {
    pub fn script(&self, report_signed_out: bool) -> String {
        self.script.replace(
            REPORT_SIGNED_OUT,
            if report_signed_out { "true" } else { "false" },
        )
    }
}

/// What to do with a navigation of a store window.
#[derive(Debug, PartialEq, Eq)]
pub enum Navigation {
    Allow,
    Block,
    /// Cancel it and hand the value over.
    Capture(String),
}

impl LoginWindow {
    /// Only https pages load; capture addresses never do.
    pub fn navigation(&self, url: &str) -> Navigation {
        if let Some(value) = (self.capture)(url) {
            return if value.is_empty() {
                Navigation::Block
            } else {
                Navigation::Capture(value)
            };
        }
        if url.starts_with(CAPTURE_PREFIX) {
            Navigation::Block
        } else if url.starts_with("https://") || url == "about:blank" {
            Navigation::Allow
        } else {
            Navigation::Block
        }
    }
}

/// A capture address itself (not a look-alike that merely starts with it).
pub fn is_capture(url: &str, prefix: &str) -> bool {
    url.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('?'))
}

/// For [`LoginWindow::capture`]: the whole address when it is the capture address `prefix`.
pub fn capture_whole(url: &str, prefix: &str) -> Option<String> {
    is_capture(url, prefix).then(|| url.to_string())
}

/// For [`LoginWindow::capture`]: the query parameter `name` when `matches` (empty when missing).
pub fn capture_param(url: &str, matches: bool, name: &str) -> Option<String> {
    matches.then(|| super::query_param(url, name).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(capture: fn(&str) -> Option<String>) -> LoginWindow {
        LoginWindow {
            title: "Sign in",
            user_agent: None,
            script: None,
            web_session: None,
            capture,
        }
    }

    #[test]
    fn only_https_pages_load_and_capture_addresses_never_do() {
        let w = window(|_| None);
        for url in [
            "https://auth.gog.com/auth?client_id=1",
            "https://www.epicgames.com/id/api/redirect?clientId=1",
            "about:blank",
        ] {
            assert_eq!(w.navigation(url), Navigation::Allow, "{url}");
        }
        for url in [
            "http://auth.gog.com/auth",
            "file:///C:/secret.txt",
            "javascript:alert(1)",
            "data:text/html,hi",
            "questkeep://x",
            "ftp://x/y",
            "https://questkeep.invalid/steam-session?token=1",
        ] {
            assert_eq!(w.navigation(url), Navigation::Block, "{url}");
        }
    }

    #[test]
    fn captures_are_handed_over_and_empty_ones_only_cancel() {
        let w = window(|url| capture_param(url, url.starts_with("https://a.test/done"), "code"));
        assert_eq!(
            w.navigation("https://a.test/done?code=x1"),
            Navigation::Capture("x1".into())
        );
        assert_eq!(w.navigation("https://a.test/done"), Navigation::Block);
        let w = window(|url| capture_whole(url, "https://questkeep.invalid/s"));
        let url = "https://questkeep.invalid/s?a=1";
        assert_eq!(w.navigation(url), Navigation::Capture(url.into()));
        assert_eq!(
            w.navigation("https://questkeep.invalid/sx?a=1"),
            Navigation::Block
        );
    }

    #[test]
    fn session_scripts_report_signed_out_only_when_hidden() {
        let page = WebSessionPage {
            store_name: "Store",
            url: "https://a.test/",
            script: "var reportSignedOut = REPORT_SIGNED_OUT;",
        };
        assert_eq!(page.script(true), "var reportSignedOut = true;");
        assert_eq!(page.script(false), "var reportSignedOut = false;");
    }
}
