//! The plugin update check (see "Update check" in `docs/PLUGIN_REGISTRY.md`): a little after start
//! and then every 12 hours the registry index is refreshed, but only while a plugin installed from
//! the registry exists. The notification bell holds one entry for the updates that are pending;
//! see [`sync_notice`].

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use questkeep_core::events::CoreEvent;
use questkeep_core::notifications::{
    NewNotification, NotificationAction, NotificationSource, Notifications,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;
use crate::manager::PluginManager;
use crate::registry::{PluginUpdate, RegistryConfig, refresh_cache, write_atomic};

/// Wait after start, so the check does not compete with the startup work.
pub const STARTUP_DELAY: Duration = Duration::from_secs(20);
/// Time between two automatic checks while the app runs.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(12 * 60 * 60);
/// How often the scheduler looks whether a check is due (so a check that was switched on later,
/// or a PC that slept, is caught up within minutes).
const TICK: Duration = Duration::from_secs(5 * 60);
/// File in the plugins folder: what the bell entry lists and which pairs the user dismissed.
const NOTICES_FILE: &str = "plugin-update-notices.json";

/// Who asks for the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    /// The scheduler: honours the setting and stays silent when the registry can't be reached.
    Automatic,
    /// The "Check now" button: always asks, and reports why it could not.
    Manual,
}

/// The outcome of a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateCheck {
    /// Installed registry plugins with a newer version.
    pub updates: Vec<PluginUpdate>,
    /// The registry was asked. `false`: there is no registry plugin, or the automatic check is
    /// switched off.
    pub requested: bool,
    /// The registry was asked but could not be reached (automatic checks only; a manual check
    /// returns the error): `updates` come from the last copy of the index.
    pub failed: bool,
}

impl PluginManager {
    /// Refreshes the registry index and works out the updates. Nothing is requested without a
    /// registry plugin, nor for an automatic check while `automatic_enabled` is off. Publishes
    /// [`CoreEvent::PluginUpdatesChanged`] when the set of updates changed.
    pub fn check_for_updates(
        &self,
        config: &RegistryConfig,
        kind: CheckKind,
        automatic_enabled: bool,
    ) -> Result<UpdateCheck> {
        let before = self.plugin_updates(config);
        let skipped = |updates: Vec<PluginUpdate>| UpdateCheck {
            updates,
            requested: false,
            failed: false,
        };
        if kind == CheckKind::Automatic && !automatic_enabled {
            tracing::debug!("plugin update check is off");
            return Ok(skipped(before));
        }
        if !self.has_registry_plugins() {
            tracing::debug!("no plugin from the registry: nothing to check");
            return Ok(skipped(before));
        }
        if let Err(err) = refresh_cache(config, &self.paths().plugins_dir()) {
            if kind == CheckKind::Manual {
                return Err(err);
            }
            tracing::debug!(%err, "plugin update check failed");
            return Ok(UpdateCheck {
                updates: before,
                requested: true,
                failed: true,
            });
        }
        let updates = self.plugin_updates(config);
        if updates != before {
            self.events().publish(CoreEvent::PluginUpdatesChanged);
        }
        tracing::info!(count = updates.len(), "plugin update check done");
        Ok(UpdateCheck {
            updates,
            requested: true,
            failed: false,
        })
    }

    /// The "Check now" button: always asks (without a registry plugin there is nothing to ask
    /// for) and reports a registry that can't be reached as an error. Like the automatic check
    /// it brings the bell entry up to date.
    pub fn run_manual_check(
        &self,
        config: &RegistryConfig,
        notifications: &Notifications,
    ) -> Result<UpdateCheck> {
        let check = self.check_for_updates(config, CheckKind::Manual, true)?;
        self.sync_update_notice(config, notifications, true);
        Ok(check)
    }

    /// One automatic check, then the bell entry is brought up to date from what is cached (also
    /// after a check that failed: the entry always reflects the last known index).
    pub fn run_scheduled_check(
        &self,
        config: &RegistryConfig,
        notifications: &Notifications,
        enabled: bool,
    ) -> UpdateCheck {
        let check = match self.check_for_updates(config, CheckKind::Automatic, enabled) {
            Ok(check) => check,
            Err(err) => {
                tracing::warn!(%err, "plugin update check failed");
                UpdateCheck {
                    updates: Vec::new(),
                    requested: true,
                    failed: true,
                }
            }
        };
        self.sync_update_notice(config, notifications, enabled);
        check
    }

    /// Brings the bell entry for plugin updates in line with the updates that are pending now
    /// (from the cached index, no network); see [`sync_notice`]. With `notify` off (the
    /// automatic check is switched off) an entry may shrink or go, but none is created and
    /// none becomes unread.
    pub fn sync_update_notice(
        &self,
        config: &RegistryConfig,
        notifications: &Notifications,
        notify: bool,
    ) {
        let pending = self.plugin_updates(config);
        sync_notice(&self.paths().plugins_dir(), notifications, &pending, notify);
    }
}

/// What the plugins folder remembers about the bell entry (`plugin-update-notices.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Notices {
    /// (plugin, version) pairs of an entry the user removed: they do not bring it back.
    #[serde(default)]
    dismissed: BTreeSet<String>,
    /// The entry that is in the bell, and the pairs its text lists.
    #[serde(default)]
    shown: Option<Shown>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Shown {
    /// The id of the entry (a UUID).
    entry: String,
    pairs: BTreeSet<String>,
}

/// The identity of an update: plugin and offered version.
fn notice_key(update: &PluginUpdate) -> String {
    format!("{}@{}", update.id, update.latest_version)
}

/// Reads the memory. The first version of the file was a plain list of announced pairs; it says
/// nothing about what the user removed, so it counts as an empty memory (an entry that is in the
/// bell is picked up as it is).
fn read_notices(plugins_dir: &Path) -> Notices {
    std::fs::read_to_string(plugins_dir.join(NOTICES_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(serde_json::Value::is_object)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn write_notices(plugins_dir: &Path, notices: &Notices) {
    match serde_json::to_vec(notices) {
        Ok(bytes) => {
            if let Err(err) = write_atomic(&plugins_dir.join(NOTICES_FILE), &bytes) {
                tracing::warn!(%err, "could not remember the plugin update notice");
            }
        }
        Err(err) => tracing::warn!(%err, "could not remember the plugin update notice"),
    }
}

/// The headline and text of the bell entry for `updates`.
pub fn notice_text(updates: &[PluginUpdate]) -> (String, String) {
    let list = updates
        .iter()
        .map(|u| {
            format!(
                "{} {} \u{2192} {}",
                u.name, u.installed_version, u.latest_version
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    match updates.len() {
        1 => (
            "Plugin update available".into(),
            format!("1 plugin update available: {list}"),
        ),
        n => (
            "Plugin updates available".into(),
            format!("{n} plugin updates available: {list}"),
        ),
    }
}

/// Serializes [`sync_notice`]: it reads and writes the notices file and the bell.
static SYNC: Mutex<()> = Mutex::new(());

/// Makes the bell entry for plugin updates represent the pending set `updates`, like a to-do:
///
/// - It exists while a pending (plugin, version) pair is not in the dismissed memory, and its
///   text lists all pending updates. It is created unread, and updated in place when the set
///   changes; it turns unread again only when a pair appears that its text did not list.
/// - It goes by itself when nothing is pending (installed, removed, pinned, no longer offered).
/// - An entry that was listed before and is gone from the bell was removed by the user: its
///   pairs are remembered and do not bring it back, only a new pair does.
///
/// Opening the bell or the updates dialog never touches it. With `notify` off nothing is
/// created and nothing turns unread (the entry may still shrink or go).
pub fn sync_notice(
    plugins_dir: &Path,
    notifications: &Notifications,
    updates: &[PluginUpdate],
    notify: bool,
) {
    let _guard = SYNC.lock().unwrap_or_else(PoisonError::into_inner);
    let action = NotificationAction::PluginUpdates;
    let current: BTreeSet<String> = updates.iter().map(notice_key).collect();
    let before = read_notices(plugins_dir);
    let mut notices = before.clone();

    let mut entries = notifications.with_action(&action).into_iter();
    let existing = entries.next();
    for extra in entries {
        notifications.dismiss(extra.id);
    }

    // Listed before, gone now: the user removed it.
    if existing.is_none()
        && let Some(shown) = notices.shown.take()
    {
        notices.dismissed.extend(shown.pairs);
    }
    // The pairs the entry's text lists (unknown for an entry from before this memory existed).
    let listed = existing.as_ref().and_then(|e| {
        notices
            .shown
            .as_ref()
            .filter(|s| s.entry == e.id.to_string())
            .map(|s| s.pairs.clone())
    });
    let visible = current.iter().any(|key| !notices.dismissed.contains(key));
    let (title, message) = notice_text(updates);

    match existing {
        Some(entry) if !visible => {
            notifications.dismiss(entry.id);
            notices.shown = None;
        }
        Some(entry) => {
            let has_new = listed
                .as_ref()
                .is_some_and(|listed| current.iter().any(|key| !listed.contains(key)));
            if !has_new || notify {
                if entry.title != title || entry.message != message || (has_new && notify) {
                    notifications.modify(entry.id, |n| {
                        n.title = title;
                        n.message = message;
                        if has_new {
                            n.read = false;
                            n.time = chrono::Utc::now();
                        }
                    });
                }
                notices.shown = Some(Shown {
                    entry: entry.id.to_string(),
                    pairs: current,
                });
            }
        }
        None if visible && notify => {
            let posted = notifications.post(
                NewNotification::info(NotificationSource::Plugins, title, message).action(action),
            );
            notices.shown = Some(Shown {
                entry: posted.id.to_string(),
                pairs: current,
            });
        }
        None => {}
    }

    if notices != before {
        write_notices(plugins_dir, &notices);
    }
}

/// Starts the threads of the update check: one brings the bell entry up to date whenever plugins
/// or their updates change (an update was installed, a plugin removed), the other checks
/// [`STARTUP_DELAY`] after start and then every [`CHECK_INTERVAL`]. `enabled` is asked before
/// each check (the user's setting); `config` says which registry to ask. The entry itself is
/// brought up to date from the cached index by the caller once the plugins are loaded.
pub fn spawn_scheduler(
    manager: PluginManager,
    notifications: Notifications,
    config: impl Fn() -> RegistryConfig + Send + Sync + 'static,
    enabled: impl Fn() -> bool + Send + Sync + 'static,
) {
    let config = Arc::new(config);
    let enabled = Arc::new(enabled);

    let (tx, rx) = mpsc::channel::<()>();
    let tx = Mutex::new(tx);
    manager.events().subscribe(move |event| {
        if matches!(
            event,
            CoreEvent::PluginsChanged | CoreEvent::PluginUpdatesChanged
        ) {
            // Only a signal: the work happens on the thread below, never on the publisher's.
            let _ = tx.lock().unwrap_or_else(PoisonError::into_inner).send(());
        }
    });
    let sync = {
        let (manager, notifications) = (manager.clone(), notifications.clone());
        let (config, enabled) = (config.clone(), enabled.clone());
        std::thread::Builder::new()
            .name("plugin-update-notice".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    while rx.try_recv().is_ok() {}
                    manager.sync_update_notice(&config(), &notifications, enabled());
                }
            })
    };
    if let Err(err) = sync {
        tracing::warn!(%err, "could not start the plugin update notice");
    }

    let spawned = std::thread::Builder::new()
        .name("plugin-update-check".into())
        .spawn(move || {
            std::thread::sleep(STARTUP_DELAY);
            let mut last: Option<Instant> = None;
            loop {
                if last.is_none_or(|at| at.elapsed() >= CHECK_INTERVAL) && enabled() {
                    manager.run_scheduled_check(&config(), &notifications, true);
                    last = Some(Instant::now());
                }
                std::thread::sleep(TICK);
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the plugin update check");
    }
}
