//! Errors and warnings kept for the notification bell. Anything in the app can post one (the
//! launcher's threads, scripts, syncs, the UI); the last [`KEEP`] are saved in
//! `notifications.json` so they survive a restart. Every change publishes
//! [`CoreEvent::NotificationsChanged`]; a new entry also publishes [`CoreEvent::Notified`] so the
//! shell can show a system notification while the window is in the background.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::events::{CoreEvent, EventBus, WeakEventBus};

/// How many entries are kept.
pub const KEEP: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NotificationLevel {
    Error,
    Warning,
    /// Good to know, nothing went wrong (plugin updates).
    Info,
}

/// What the entry is about (shown next to its time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NotificationSource {
    /// Starting or tracking a game.
    Launch,
    /// Game scripts.
    Scripts,
    /// Closing store launchers after playing.
    Launchers,
    /// Store syncs.
    Sync,
    /// Plugins (updates found by the check).
    Plugins,
    /// Anything else.
    App,
}

/// A button on the entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum NotificationAction {
    /// Opens a file (a script's output log).
    OpenFile { path: String },
    /// Opens the script dialog of a script.
    EditScript {
        #[ts(type = "string")]
        id: Uuid,
    },
    /// Opens Settings › Stores and accounts at a store.
    StoreSettings { store: String },
    /// Opens the Plugin updates dialog.
    PluginUpdates,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Notification {
    #[ts(type = "string")]
    pub id: Uuid,
    #[ts(type = "string")]
    pub time: DateTime<Utc>,
    pub level: NotificationLevel,
    pub source: NotificationSource,
    /// A short headline for system notifications (e.g. "Pre script failed").
    pub title: String,
    pub message: String,
    #[ts(type = "string | null")]
    pub game_id: Option<Uuid>,
    /// More buttons than one never fit a notification; the UI adds a second where it knows one.
    pub action: Option<NotificationAction>,
    pub read: bool,
}

/// What a new entry says; see [`Notifications::post`].
#[derive(Debug, Clone)]
pub struct NewNotification {
    pub level: NotificationLevel,
    pub source: NotificationSource,
    pub title: String,
    pub message: String,
    pub game_id: Option<Uuid>,
    pub action: Option<NotificationAction>,
}

impl NewNotification {
    pub fn error(
        source: NotificationSource,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level: NotificationLevel::Error,
            source,
            title: title.into(),
            message: message.into(),
            game_id: None,
            action: None,
        }
    }

    pub fn warning(
        source: NotificationSource,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level: NotificationLevel::Warning,
            ..Self::error(source, title, message)
        }
    }

    pub fn info(
        source: NotificationSource,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level: NotificationLevel::Info,
            ..Self::error(source, title, message)
        }
    }

    pub fn game(mut self, id: Uuid) -> Self {
        self.game_id = Some(id);
        self
    }

    pub fn action(mut self, action: NotificationAction) -> Self {
        self.action = Some(action);
        self
    }
}

/// The kept entries. Cheap to clone; clones share them.
#[derive(Clone)]
pub struct Notifications {
    path: Option<PathBuf>,
    items: Arc<Mutex<Vec<Notification>>>,
    events: WeakEventBus,
}

impl std::fmt::Debug for Notifications {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notifications")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Notifications {
    /// Loads the entries kept in `path` (none when missing or unreadable).
    pub fn open(path: impl Into<PathBuf>, events: &EventBus) -> Self {
        let path = path.into();
        let items = std::fs::read(&path)
            .ok()
            .and_then(
                |bytes| match serde_json::from_slice::<Vec<Notification>>(&bytes) {
                    Ok(items) => Some(items),
                    Err(err) => {
                        tracing::warn!(%err, "could not read the kept notifications");
                        None
                    }
                },
            )
            .unwrap_or_default();
        Self {
            path: Some(path),
            items: Arc::new(Mutex::new(items)),
            events: events.downgrade(),
        }
    }

    /// Entries that are only kept in memory (tests).
    pub fn in_memory(events: &EventBus) -> Self {
        Self {
            path: None,
            items: Arc::new(Mutex::new(Vec::new())),
            events: events.downgrade(),
        }
    }

    /// Records game errors and failed syncs from the event bus as they happen.
    pub fn watch(&self, events: &EventBus) {
        let this = self.clone();
        events.subscribe(move |event| match event {
            CoreEvent::GameError { id, message } => {
                this.post(
                    NewNotification::error(NotificationSource::Launch, "Game error", message)
                        .game(*id),
                );
            }
            CoreEvent::ScanFailed { provider, message } => {
                this.post(
                    NewNotification::error(
                        NotificationSource::Sync,
                        "Sync failed",
                        message.clone(),
                    )
                    .action(NotificationAction::StoreSettings {
                        store: provider.clone(),
                    }),
                );
            }
            _ => {}
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Notification>> {
        self.items.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Newest first.
    pub fn list(&self) -> Vec<Notification> {
        self.lock().clone()
    }

    /// Adds an entry (newest first), keeps the last [`KEEP`] and tells the UI and the shell.
    pub fn post(&self, new: NewNotification) -> Notification {
        let notification = Notification {
            id: Uuid::new_v4(),
            time: Utc::now(),
            level: new.level,
            source: new.source,
            title: new.title,
            message: new.message,
            game_id: new.game_id,
            action: new.action,
            read: false,
        };
        match notification.level {
            NotificationLevel::Error => {
                tracing::warn!(message = %notification.message, "error notification")
            }
            NotificationLevel::Warning => {
                tracing::info!(message = %notification.message, "warning notification")
            }
            NotificationLevel::Info => {
                tracing::info!(message = %notification.message, "notification")
            }
        }
        self.change(|items| {
            items.insert(0, notification.clone());
            items.truncate(KEEP);
        });
        self.events.publish(CoreEvent::Notified {
            notification: notification.clone(),
        });
        notification
    }

    pub fn mark_all_read(&self) {
        self.change(|items| items.iter_mut().for_each(|n| n.read = true));
    }

    pub fn dismiss(&self, id: Uuid) {
        self.change(|items| items.retain(|n| n.id != id));
    }

    /// Removes the entries that carry `action` (a newer entry replaces them).
    pub fn dismiss_with_action(&self, action: &NotificationAction) {
        self.change(|items| items.retain(|n| n.action.as_ref() != Some(action)));
    }

    pub fn clear(&self) {
        self.change(Vec::clear);
    }

    /// The entries that carry `action`, newest first.
    pub fn with_action(&self, action: &NotificationAction) -> Vec<Notification> {
        self.lock()
            .iter()
            .filter(|n| n.action.as_ref() == Some(action))
            .cloned()
            .collect()
    }

    /// Changes the entry `id` in place (its place in the list and its id stay). Returns whether
    /// there is such an entry.
    pub fn modify(&self, id: Uuid, edit: impl FnOnce(&mut Notification)) -> bool {
        let mut found = false;
        self.change(|items| {
            if let Some(entry) = items.iter_mut().find(|n| n.id == id) {
                edit(entry);
                entry.id = id;
                found = true;
            }
        });
        found
    }

    fn change(&self, edit: impl FnOnce(&mut Vec<Notification>)) {
        {
            let mut items = self.lock();
            let before = items.clone();
            edit(&mut items);
            if *items == before {
                return;
            }
            // Written while the lock is held: two changes at once must not interleave their
            // files, or an older list could replace a newer one on disk.
            if let Some(path) = &self.path
                && let Err(err) = save(path, &items)
            {
                tracing::warn!(%err, "could not save the notifications");
            }
        }
        self.events.publish(CoreEvent::NotificationsChanged);
    }
}

/// Writes `items` to `path` (to a temporary file first, then renamed over the old one). Windows
/// refuses to replace a file another program has open for a moment (a virus scanner, a backup),
/// so a refused rename is tried again a few times.
fn save(path: &std::path::Path, items: &[Notification]) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(items).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    let mut attempt = 0;
    loop {
        match std::fs::rename(&tmp, path) {
            Err(err)
                if attempt < 5
                    && (err.kind() == std::io::ErrorKind::PermissionDenied
                        || (cfg!(windows) && err.raw_os_error() == Some(32))) =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(40 * attempt));
            }
            Err(err) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(err);
            }
            Ok(()) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_and_saves_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("n.json");
        let bus = EventBus::new();
        let notes = Notifications::open(&path, &bus);
        for i in 0..(KEEP + 5) {
            notes.post(NewNotification::warning(
                NotificationSource::App,
                "t",
                format!("m{i}"),
            ));
        }
        let list = notes.list();
        assert_eq!(list.len(), KEEP);
        assert_eq!(list[0].message, format!("m{}", KEEP + 4));
        assert!(list.iter().all(|n| !n.read));

        notes.mark_all_read();
        let reopened = Notifications::open(&path, &bus);
        assert_eq!(reopened.list().len(), KEEP);
        assert!(reopened.list().iter().all(|n| n.read));

        let first = reopened.list()[0].id;
        reopened.dismiss(first);
        assert_eq!(reopened.list().len(), KEEP - 1);
        reopened.clear();
        assert!(Notifications::open(&path, &bus).list().is_empty());
    }

    #[test]
    fn entries_with_an_action_can_be_replaced() {
        let bus = EventBus::new();
        let notes = Notifications::in_memory(&bus);
        notes.post(
            NewNotification::info(NotificationSource::Plugins, "t", "old")
                .action(NotificationAction::PluginUpdates),
        );
        notes.post(NewNotification::warning(
            NotificationSource::App,
            "t",
            "other",
        ));
        notes.dismiss_with_action(&NotificationAction::PluginUpdates);
        let list = notes.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message, "other");
    }

    #[test]
    fn an_entry_can_be_changed_in_place() {
        let bus = EventBus::new();
        let notes = Notifications::in_memory(&bus);
        let mine = notes.post(
            NewNotification::info(NotificationSource::Plugins, "t", "old")
                .action(NotificationAction::PluginUpdates),
        );
        notes.post(NewNotification::warning(
            NotificationSource::App,
            "t",
            "other",
        ));
        assert!(notes.modify(mine.id, |n| {
            n.message = "new".into();
            n.id = Uuid::new_v4();
        }));
        let list = notes.list();
        assert_eq!(list[1].id, mine.id, "keeps its place and id");
        assert_eq!(list[1].message, "new");
        assert!(!notes.modify(Uuid::new_v4(), |n| n.message = "x".into()));
        assert_eq!(
            notes.with_action(&NotificationAction::PluginUpdates).len(),
            1
        );
    }

    #[test]
    fn concurrent_changes_leave_the_newest_list_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("n.json");
        let bus = EventBus::new();
        let notes = Notifications::open(&path, &bus);
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let notes = notes.clone();
                std::thread::spawn(move || {
                    for i in 0..10 {
                        notes.post(NewNotification::warning(
                            NotificationSource::App,
                            "t",
                            format!("{t}-{i}"),
                        ));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(notes.list().len(), 80);
        assert_eq!(Notifications::open(&path, &bus).list(), notes.list());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn records_game_errors_and_failed_syncs() {
        let bus = EventBus::new();
        let notes = Notifications::in_memory(&bus);
        notes.watch(&bus);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        bus.subscribe(move |e| s.lock().unwrap().push(e.clone()));
        let id = Uuid::new_v4();
        bus.publish(CoreEvent::GameError {
            id,
            message: "boom".into(),
        });
        bus.publish(CoreEvent::ScanFailed {
            provider: "steam".into(),
            message: "offline".into(),
        });
        let list = notes.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].game_id, Some(id));
        assert_eq!(list[0].source, NotificationSource::Sync);
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, CoreEvent::Notified { .. }))
        );
    }
}
