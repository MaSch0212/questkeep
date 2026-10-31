//! The login window for store accounts and the hidden window that reads store website sessions.
//!
//! It is a separate window that loads the store's own login page. It has no IPC access: the
//! capability file only names the `main` window, and `lib.rs` rejects every command that does not
//! come from `main`. The store describes the window (`LoginWindow`, see
//! `questkeep_store_api::auth`): its title and scripts, and which navigation carries the result,
//! which is then cancelled and handed to the core. The window runs in a private (non-persistent)
//! browser profile, so no store session outlives the sign-in.
//!
//! Stores signed in through their website session (Steam, Battle.net, EA) are the exception: their
//! windows use a persistent profile per store (`<data>/webview/<store>`), and a script on the
//! store's pages reports what Questkeep needs through a capture navigation. Before a scan a hidden
//! window loads such a page with that profile, which lets the store renew the session itself.
//! Signing out deletes the profile's data.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use questkeep_core::auth::{LoginWindow, Navigation, WebSession};
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent};

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

/// Opens the login window and waits until it yields a code or is closed. Only one window per
/// provider: asking again focuses the open one and reports an error. `profile` is the
/// persistent browser profile of stores signed in through their website (Steam, Battle.net, EA).
pub async fn run(
    app: &AppHandle,
    provider: &str,
    login_url: &str,
    window: LoginWindow,
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
    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(window.title)
        .inner_size(500.0, 700.0)
        .center()
        .incognito(window.web_session.is_none())
        .on_navigation(move |url| match window.navigation(url.as_str()) {
            Navigation::Allow => true,
            Navigation::Block => false,
            Navigation::Capture(code) => {
                let _ = nav_tx.try_send(Outcome::Code(code));
                false
            }
        });
    if let Some(script) = window.script {
        builder = builder.initialization_script(script);
    }
    if let Some(user_agent) = window.user_agent {
        builder = builder.user_agent(user_agent);
    }
    if let Some(page) = window.web_session {
        builder = builder
            .data_directory(profile)
            .initialization_script(page.script(false));
    }
    let webview = builder.build().map_err(|e| {
        tracing::warn!(error = %e, "could not open the sign-in window");
        format!("Could not open the sign-in window: {e}")
    })?;
    webview.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let _ = tx.try_send(Outcome::Closed);
        }
    });

    let outcome = rx.recv().await.unwrap_or(Outcome::Closed);
    if matches!(outcome, Outcome::Code(_)) {
        let _ = webview.destroy();
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
        window: Option<&LoginWindow>,
        url: Url,
        captured: Option<mpsc::Sender<String>>,
    ) -> Result<tauri::WebviewWindow, String> {
        let label = format!("session-{provider}");
        if let Some(stale) = self.app.get_webview_window(&label) {
            let _ = stale.destroy();
        }
        let title = window.map_or("Questkeep", |w| w.title);
        let mut builder = WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(url))
            .title(title)
            .visible(false)
            .focused(false)
            .skip_taskbar(true)
            // Hidden pages are throttled otherwise, which stalls the stores' session renewal.
            .background_throttling(BackgroundThrottlingPolicy::Disabled)
            .data_directory(self.profile(provider));
        if let Some(page) = window.and_then(|w| w.web_session) {
            builder = builder.initialization_script(page.script(true));
        }
        // Clearing the session loads nothing but about:blank.
        let window = window.copied();
        builder
            .on_navigation(
                move |url| match window.map(|w| w.navigation(url.as_str())) {
                    None => url.as_str() == "about:blank",
                    Some(Navigation::Allow) => true,
                    Some(Navigation::Block) => false,
                    Some(Navigation::Capture(found)) => {
                        if let Some(tx) = &captured {
                            let _ = tx.send(found);
                        }
                        false
                    }
                },
            )
            .build()
            .map_err(|e| {
                tracing::warn!(error = %e, "could not open the hidden store window");
                format!("Could not open the {title} session window: {e}")
            })
    }
}

impl WebSession for ShellWebSession {
    fn refresh(&self, provider: &str, window: &LoginWindow) -> Result<Option<String>, String> {
        let Some(page) = window.web_session else {
            return Err("This store has no web session.".into());
        };
        let _busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let url: Url = page
            .url
            .parse()
            .map_err(|_| "The store address is invalid.".to_string())?;
        let (tx, rx) = mpsc::channel();
        let hidden = self.hidden_window(provider, Some(window), url, Some(tx))?;
        let result = rx.recv_timeout(SESSION_TIMEOUT);
        let _ = hidden.destroy();
        match result {
            Ok(captured) => Ok(Some(captured)),
            Err(_) => Err(format!("{} did not answer in time.", page.store_name)),
        }
    }

    fn clear(&self, provider: &str) -> Result<(), String> {
        let _busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let url: Url = "about:blank"
            .parse()
            .map_err(|_| "invalid address".to_string())?;
        let window = self.hidden_window(provider, None, url, None)?;
        let result = window
            .clear_all_browsing_data()
            .map_err(|e| format!("Could not delete the saved sign-in: {e}"));
        let _ = window.destroy();
        result
    }
}
