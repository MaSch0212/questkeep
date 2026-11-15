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

    pub fn clear(&self) {
        self.change(Vec::clear);
    }

    fn change(&self, edit: impl FnOnce(&mut Vec<Notification>)) {
        let snapshot = {
            let mut items = self.lock();
            let before = items.clone();
            edit(&mut items);
            if *items == before {
                return;
            }
            items.clone()
        };
        if let Some(path) = &self.path {
            let written = serde_json::to_vec_pretty(&snapshot)
                .map_err(std::io::Error::other)
                .and_then(|bytes| {
                    let tmp = path.with_extension("json.tmp");
                    std::fs::write(&tmp, bytes)?;
                    std::fs::rename(&tmp, path)
                });
            if let Err(err) = written {
                tracing::warn!(%err, "could not save the notifications");
            }
        }
        self.events.publish(CoreEvent::NotificationsChanged);
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
