//! The login window for store accounts (GOG, Epic, Xbox, Amazon, Steam, Battle.net, EA) and the
//! hidden window that reads the website sessions of Steam, Battle.net and EA.
//!
//! It is a separate window that loads the store's own login page. It has no IPC access: the
//! capability file only names the `main` window, and `lib.rs` rejects every command that does not
//! come from `main`. The window's navigations are watched instead: when the store redirects to
//! the page that carries the one-time code, the navigation is cancelled and the code is handed to
//! the core. The window runs in a private (non-persistent) browser profile, so no store session
//! outlives the sign-in.
//!
//! Steam, Battle.net and EA are the exception: they have no code for other applications, only
//! their website session. Their windows use a persistent profile per store
//! (`<data>/webview/<store>`), and a script on the store's pages reports what Questkeep needs
//! through a capture navigation: Steam's Web API token, Battle.net's owned games, an EA access
//! token (see `questkeep_core::providers::{steam, battlenet, ea}`). Before a scan a hidden window
//! loads such a page with that profile, which lets the store renew the session itself. Signing
//! out deletes the profile's data.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use questkeep_core::auth::{WebSession, query_param};
use questkeep_core::providers::amazon::{
    CODE_PARAM as AMAZON_CODE_PARAM, LOGIN_RETURN_PREFIX as AMAZON_RETURN_PREFIX,
};
use questkeep_core::providers::battlenet::{
    CAPTURE_PREFIX as BATTLENET_CAPTURE_PREFIX, SESSION_URL as BATTLENET_SESSION_URL,
};
use questkeep_core::providers::ea::{
    CAPTURE_PREFIX as EA_CAPTURE_PREFIX, LOGIN_DONE_PREFIX as EA_LOGIN_DONE_PREFIX,
    SESSION_URL as EA_SESSION_URL,
};
use questkeep_core::providers::gog::LOGIN_SUCCESS_PREFIX;
use questkeep_core::providers::steam::{CAPTURE_PREFIX as STEAM_CAPTURE_PREFIX, SESSION_URL};
use questkeep_core::providers::xbox::LOGIN_REDIRECT_PREFIX as XBOX_REDIRECT_PREFIX;
use tauri::utils::config::BackgroundThrottlingPolicy;
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

/// Amazon's login only accepts the launcher's client id from the launcher's own browser.
const AMAZON_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) @amzn/aga-electron-platform/1.0.0 Chrome/78.0.3904.130 Electron/7.1.9 Safari/537.36";

/// Runs on every page of the Steam windows but only acts on store pages: reports the signed-in
/// user's Web API token from the `application_config` element. `REPORT_SIGNED_OUT` is replaced
/// by `true` in the hidden window, which must also learn about a signed-out session; it waits a
/// moment first, because Steam may still be renewing the session through a redirect.
const STEAM_SCRIPT: &str = r#"
(function () {
  if (location.hostname !== "store.steampowered.com") return;
  var reportSignedOut = REPORT_SIGNED_OUT;
  var target = "https://questkeep.invalid/steam-session";
  function read(el, name) {
    try { return JSON.parse(el.getAttribute(name) || "null"); } catch (e) { return null; }
  }
  function capture() {
    var el = document.getElementById("application_config");
    var user = el && read(el, "data-userinfo");
    var config = el && read(el, "data-store_user_config");
    if (user && user.logged_in && user.steamid && config && config.webapi_token) {
      var menu = document.getElementById("account_pulldown");
      var name = menu ? menu.textContent.trim() : "";
      location.href = target + "?steamid=" + encodeURIComponent(user.steamid) +
        "&token=" + encodeURIComponent(config.webapi_token) + "&name=" + encodeURIComponent(name);
    } else if (reportSignedOut) {
      setTimeout(function () { location.href = target + "?signedout=1"; }, 5000);
    }
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", capture);
  } else {
    capture();
  }
})();
"#;

/// Runs on Battle.net pages. On the account site (after the login, or when the session is still
/// valid) it reads the owned games and the BattleTag with the page's own session and reports
/// them. On a login page the hidden window reports a signed-out session.
const BATTLENET_SCRIPT: &str = r##"
(function () {
  var reportSignedOut = REPORT_SIGNED_OUT;
  var target = "https://questkeep.invalid/battlenet-session";
  var host = location.hostname;
  if (host !== "battle.net" && !/\.battle\.net$/.test(host)) return;
  if (/^\/login/.test(location.pathname)) {
    if (reportSignedOut) {
      setTimeout(function () { location.href = target + "?signedout=1"; }, 5000);
    }
    return;
  }
  if (host !== "account.battle.net" || /^\/(callback|oauth2)\//.test(location.pathname)) return;
  function get(path, anyStatus) {
    return fetch(path, { credentials: "include", headers: { Accept: "application/json" } })
      .then(function (r) {
        if (!r.ok && !anyStatus) throw new Error("http " + r.status);
        return r.json();
      });
  }
  function tag(details) {
    var t = details && details.battleTag;
    if (t && typeof t === "object") return t.name ? t.name + (t.code ? "#" + t.code : "") : "";
    return typeof t === "string" ? t : "";
  }
  get("/api/", true).then(function (status) {
    if (!status || !status.authenticated) throw new Error("signedout");
    return Promise.all([
      get("/api/games-and-subs"),
      get("/api/details").catch(function () { return null; })
    ]);
  }).then(function (r) {
    var games = ((r[0] && r[0].gameAccounts) || []).map(function (g) {
      return { titleId: g.titleId, localizedGameName: g.localizedGameName };
    });
    location.href = target + "?name=" + encodeURIComponent(tag(r[1])) +
      "&games=" + encodeURIComponent(JSON.stringify(games));
  }).catch(function (e) {
    if (reportSignedOut && e && e.message === "signedout") location.href = target + "?signedout=1";
  });
})();
"##;

/// Runs on EA pages. On the login page it ticks "Remember me" once, so the session outlives the
/// window. On the token page (`connect/auth` for `nucleus:rest`, which answers with JSON) it
/// reports the access token, or in the hidden window a signed-out session.
const EA_SCRIPT: &str = r##"
(function () {
  var reportSignedOut = REPORT_SIGNED_OUT;
  var target = "https://questkeep.invalid/ea-session";
  if (location.hostname === "signin.ea.com") {
    var tries = 0;
    var tick = setInterval(function () {
      var box = document.getElementById("rememberMe");
      if (box || ++tries > 40) clearInterval(tick);
      if (box && !box.checked) box.click();
    }, 250);
    return;
  }
  if (location.hostname !== "accounts.ea.com" || location.pathname !== "/connect/auth" ||
      location.search.indexOf("nucleus") < 0) return;
  function capture() {
    var data = null;
    try { data = JSON.parse(document.body.innerText); } catch (e) {}
    if (data && data.access_token) {
      location.href = target + "?token=" + encodeURIComponent(data.access_token) +
        "&expires=" + encodeURIComponent(data.expires_in || "");
    } else if (reportSignedOut) {
      location.href = target + "?signedout=1";
    }
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", capture);
  } else {
    capture();
  }
})();
"##;

/// Whether the store's sign-in is its website session, kept in a persistent profile.
fn uses_web_profile(provider: &str) -> bool {
    matches!(provider, "steam" | "battlenet" | "ea")
}

/// The page script of a website-session store; `report_signed_out` in the hidden window.
fn page_script(provider: &str, report_signed_out: bool) -> Option<String> {
    let script = match provider {
        "steam" => STEAM_SCRIPT,
        "battlenet" => BATTLENET_SCRIPT,
        "ea" => EA_SCRIPT,
        _ => return None,
    };
    Some(script.replace(
        "REPORT_SIGNED_OUT",
        if report_signed_out { "true" } else { "false" },
    ))
}

/// The page the hidden window loads to read a store's session.
fn session_url(provider: &str) -> Option<&'static str> {
    match provider {
        "steam" => Some(SESSION_URL),
        "battlenet" => Some(BATTLENET_SESSION_URL),
        "ea" => Some(EA_SESSION_URL),
        _ => None,
    }
}

/// How long the hidden window may take to report a store session.
const SESSION_TIMEOUT: Duration = Duration::from_secs(45);

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

/// The Microsoft desktop redirect itself (not a page that merely starts with the same letters).
fn is_xbox_redirect(url: &str) -> bool {
    url.strip_prefix(XBOX_REDIRECT_PREFIX)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['?', '#']))
}

fn classify(provider: &str, url: &str) -> Navigation {
    let captured = match provider {
        "gog" => url
            .starts_with(LOGIN_SUCCESS_PREFIX)
            .then(|| query_param(url, "code")),
        "epic" => (url.starts_with(EPIC_CAPTURE_URL) || url.starts_with(EPIC_LAUNCHER_REDIRECT))
            .then(|| query_param(url, "code")),
        // Microsoft ends the login on a blank page at oauth20_desktop.srf?code=....
        "xbox" => is_xbox_redirect(url).then(|| query_param(url, "code")),
        // Amazon returns to its front page with the code in the query. Other amazon.com pages
        // (the login itself) load normally.
        "amazon" => (url.starts_with(AMAZON_RETURN_PREFIX)
            && query_param(url, AMAZON_CODE_PARAM).is_some())
        .then(|| query_param(url, AMAZON_CODE_PARAM)),
        // The core reads the whole capture address.
        "steam" => is_capture(url, STEAM_CAPTURE_PREFIX).then(|| Some(url.to_string())),
        "battlenet" => is_capture(url, BATTLENET_CAPTURE_PREFIX).then(|| Some(url.to_string())),
        // EA's website login ends on login_check; the core then reads the new session.
        "ea" => (is_capture(url, EA_CAPTURE_PREFIX) || is_capture(url, EA_LOGIN_DONE_PREFIX))
            .then(|| Some(url.to_string())),
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

/// A capture address itself (not a look-alike that merely starts with it).
fn is_capture(url: &str, prefix: &str) -> bool {
    url.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('?'))
}

fn store_name(provider: &str) -> &'static str {
    match provider {
        "steam" => "The Steam store",
        "battlenet" => "Battle.net",
        "ea" => "EA",
        _ => "The store",
    }
}

fn title(provider: &str) -> &'static str {
    match provider {
        "gog" => "Sign in to GOG",
        "epic" => "Sign in to Epic Games",
        "xbox" => "Sign in to Xbox",
        "amazon" => "Sign in to Amazon Games",
        "steam" => "Sign in to Steam",
        "battlenet" => "Sign in to Battle.net",
        "ea" => "Sign in to EA",
        _ => "Sign in",
    }
}

/// Opens the login window and waits until it yields a code or is closed. Only one window per
/// provider: asking again focuses the open one and reports an error. `profile` is the
/// persistent browser profile of stores signed in through their website (Steam, Battle.net, EA).
pub async fn run(
    app: &AppHandle,
    provider: &str,
    login_url: &str,
    profile: PathBuf,
) -> Result<Outcome, String> {
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
        .incognito(!uses_web_profile(provider))
        .on_navigation(move |url| match classify(&nav_provider, url.as_str()) {
            Navigation::Allow => true,
            Navigation::Block => false,
            Navigation::Capture(code) => {
                let _ = nav_tx.try_send(Outcome::Code(code));
                false
            }
        });
    match provider {
        "epic" => builder = builder.initialization_script(EPIC_SCRIPT),
        "amazon" => builder = builder.user_agent(AMAZON_USER_AGENT),
        _ => {}
    }
    if uses_web_profile(provider) {
        builder = builder.data_directory(profile);
        if let Some(script) = page_script(provider, false) {
            builder = builder.initialization_script(script);
        }
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

/// Store website sessions in their persistent browser profiles, driven by hidden windows.
pub struct ShellWebSession {
    app: AppHandle,
    /// `<data>/webview`; each store gets a profile folder below it.
    profiles: PathBuf,
    /// One hidden window at a time (a sync and a manual scan may ask together).
    busy: Mutex<()>,
}

impl ShellWebSession {
    pub fn new(app: AppHandle, profiles: PathBuf) -> Self {
        Self {
            app,
            profiles,
            busy: Mutex::new(()),
        }
    }

    pub fn profile(&self, provider: &str) -> PathBuf {
        self.profiles.join(provider)
    }

    /// A hidden, unfocused window off the taskbar on the store's profile. It never shows: the
    /// user may be playing a fullscreen game while the library syncs.
    fn hidden_window(
        &self,
        provider: &str,
        url: Url,
        captured: Option<mpsc::Sender<String>>,
    ) -> Result<tauri::WebviewWindow, String> {
        let label = format!("session-{provider}");
        if let Some(stale) = self.app.get_webview_window(&label) {
            let _ = stale.destroy();
        }
        let watched = provider.to_string();
        let mut builder = WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(url))
            .title(title(provider))
            .visible(false)
            .focused(false)
            .skip_taskbar(true)
            // Hidden pages are throttled otherwise, which stalls the stores' session renewal.
            .background_throttling(BackgroundThrottlingPolicy::Disabled)
            .data_directory(self.profile(provider));
        if let Some(script) = page_script(provider, true) {
            builder = builder.initialization_script(script);
        }
        builder
            .on_navigation(move |url| match classify(&watched, url.as_str()) {
                Navigation::Allow => true,
                Navigation::Block => false,
                Navigation::Capture(found) => {
                    if let Some(tx) = &captured {
                        let _ = tx.send(found);
                    }
                    false
                }
            })
            .build()
            .map_err(|e| {
                tracing::warn!(error = %e, "could not open the hidden store window");
                format!("Could not open the {} session window: {e}", title(provider))
            })
    }
}

impl WebSession for ShellWebSession {
    fn refresh(&self, provider: &str) -> Result<Option<String>, String> {
        let Some(session_url) = session_url(provider) else {
            return Err("This store has no web session.".into());
        };
        let _busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let url: Url = session_url
            .parse()
            .map_err(|_| "The store address is invalid.".to_string())?;
        let (tx, rx) = mpsc::channel();
        let window = self.hidden_window(provider, url, Some(tx))?;
        let result = rx.recv_timeout(SESSION_TIMEOUT);
        let _ = window.destroy();
        match result {
            Ok(captured) => Ok(Some(captured)),
            Err(_) => Err(format!("{} did not answer in time.", store_name(provider))),
        }
    }

    fn clear(&self, provider: &str) -> Result<(), String> {
        let _busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let url: Url = "about:blank"
            .parse()
            .map_err(|_| "invalid address".to_string())?;
        let window = self.hidden_window(provider, url, None)?;
        let result = window
            .clear_all_browsing_data()
            .map_err(|e| format!("Could not delete the saved sign-in: {e}"));
        let _ = window.destroy();
        result
    }
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
    fn xbox_redirect_is_captured_and_only_for_xbox() {
        let url = "https://login.live.com/oauth20_desktop.srf?code=M.C507_BAY.2.U.0123abcd&lc=1033";
        assert_eq!(
            classify("xbox", url),
            Navigation::Capture("M.C507_BAY.2.U.0123abcd".into())
        );
        // Cancelled or failed: cancel the navigation, never load the blank page.
        for url in [
            "https://login.live.com/oauth20_desktop.srf?error=access_denied",
            "https://login.live.com/oauth20_desktop.srf",
        ] {
            assert_eq!(classify("xbox", url), Navigation::Block, "{url}");
        }
        // The login pages and look-alikes are not the redirect.
        for url in [
            "https://login.live.com/oauth20_authorize.srf?client_id=1",
            "https://login.live.com/oauth20_desktop.srf.evil.test/?code=12345678",
            "https://login.live.com/oauth20_desktop.srfx?code=12345678",
        ] {
            assert_eq!(classify("xbox", url), Navigation::Allow, "{url}");
        }
        assert_eq!(classify("gog", url), Navigation::Allow);
        assert_eq!(classify("epic", url), Navigation::Allow);
        assert_eq!(
            classify(
                "xbox",
                "http://login.live.com/oauth20_desktop.srf?code=12345678"
            ),
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
    fn amazon_return_is_captured_and_login_pages_load() {
        let url = "https://www.amazon.com/?openid.assoc_handle=amzn_sonic_games_launcher&openid.oa2.authorization_code=ANdNAVhyhqir&openid.mode=id_res";
        assert_eq!(
            classify("amazon", url),
            Navigation::Capture("ANdNAVhyhqir".into())
        );
        for url in [
            "https://www.amazon.com/ap/signin?openid.mode=checkid_setup",
            "https://www.amazon.com/ap/mfa?arb=1",
            "https://www.amazon.com/?openid.mode=cancel",
        ] {
            assert_eq!(classify("amazon", url), Navigation::Allow, "{url}");
        }
        assert_eq!(classify("gog", url), Navigation::Allow);
    }

    #[test]
    fn steam_capture_is_handed_over_whole_and_only_for_steam() {
        let url = "https://questkeep.invalid/steam-session?steamid=76561197960287930&token=a.b.c";
        assert_eq!(classify("steam", url), Navigation::Capture(url.into()));
        assert_eq!(classify("epic", url), Navigation::Block);
        assert_eq!(
            classify(
                "steam",
                "https://questkeep.invalid/steam-sessionx?steamid=1"
            ),
            Navigation::Block
        );
        assert_eq!(
            classify(
                "steam",
                "https://store.steampowered.com/login/?redir=explore%2F"
            ),
            Navigation::Allow
        );
    }

    #[test]
    fn page_scripts_report_signed_out_only_when_hidden() {
        for provider in ["steam", "battlenet", "ea"] {
            assert!(uses_web_profile(provider));
            assert!(session_url(provider).is_some_and(|u| u.starts_with("https://")));
            let hidden = page_script(provider, true).unwrap();
            assert!(hidden.contains("var reportSignedOut = true;"), "{provider}");
            assert!(!hidden.contains("REPORT_SIGNED_OUT"), "{provider}");
            assert!(
                page_script(provider, false)
                    .unwrap()
                    .contains("var reportSignedOut = false;")
            );
        }
        assert!(page_script("gog", true).is_none() && !uses_web_profile("gog"));
        assert!(session_url("gog").is_none());
        assert!(STEAM_SCRIPT.contains(STEAM_CAPTURE_PREFIX));
        assert!(BATTLENET_SCRIPT.contains(BATTLENET_CAPTURE_PREFIX));
        assert!(EA_SCRIPT.contains(EA_CAPTURE_PREFIX));
    }

    #[test]
    fn battlenet_and_ea_captures_are_handed_over_whole() {
        let url = "https://questkeep.invalid/battlenet-session?name=Ann%231&games=%5B%5D";
        assert_eq!(classify("battlenet", url), Navigation::Capture(url.into()));
        assert_eq!(classify("ea", url), Navigation::Block);
        assert_eq!(
            classify("battlenet", "https://account.battle.net/overview"),
            Navigation::Allow
        );
        let token = "https://questkeep.invalid/ea-session?token=QVQwOjIuMDoz&expires=3599";
        assert_eq!(classify("ea", token), Navigation::Capture(token.into()));
        let done = "https://www.ea.com/login_check?code=QUOxOjMuMDoz&state=x";
        assert_eq!(classify("ea", done), Navigation::Capture(done.into()));
        for url in [
            "https://www.ea.com/login_checkx?code=1",
            "https://signin.ea.com/p/juno/login?execution=e1s1",
        ] {
            assert_eq!(classify("ea", url), Navigation::Allow, "{url}");
        }
        assert_eq!(classify("steam", done), Navigation::Allow);
    }

    #[test]
    fn epic_script_only_acts_on_the_redirect_page() {
        assert!(EPIC_SCRIPT.contains(EPIC_REDIRECT_PREFIX));
        assert!(EPIC_SCRIPT.contains(EPIC_CAPTURE_URL));
    }
}
