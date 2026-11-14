//! Internal event bus. The Tauri shell forwards every event to the UI; future plugins will
//! subscribe here too, which is why events are plain serializable data.

use std::sync::{Arc, PoisonError, RwLock};

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::backup::BackupInfo;
use crate::model::ScanSummary;

/// Something that happened in the core.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CoreEvent {
    /// Games were added or their data changed.
    GamesChanged {
        ids: Vec<Uuid>,
    },
    GameRemoved {
        id: Uuid,
    },
    /// Categories were created, renamed, deleted or imported.
    CategoriesChanged,
    /// A plugin was installed, loaded, unloaded, reloaded or removed: the lists of plugins and
    /// stores changed.
    PluginsChanged,
    /// A full sync is about to scan these providers, one after another.
    ScansQueued {
        providers: Vec<String>,
        /// Started by Questkeep itself (the automatic sync), not by the user: the UI shows no
        /// result message for it.
        background: bool,
    },
    ScanStarted {
        provider: String,
        /// See `ScansQueued::background`.
        background: bool,
    },
    LibraryScanned {
        provider: String,
        summary: ScanSummary,
    },
    ScanFailed {
        provider: String,
        message: String,
    },
    GameStarted {
        id: Uuid,
    },
    GameStopped {
        id: Uuid,
        #[ts(type = "number")]
        session_seconds: u64,
    },
    /// A watched install finished: the game is installed now (see `Library::poll_installs`).
    InstallFinished {
        id: Uuid,
    },
    /// A game could not be launched or tracked.
    GameError {
        id: Uuid,
        message: String,
    },
    /// How many jobs a background queue still has. Sent when work is queued, while it runs (at
    /// most a few times a second) and when the queue is empty (`pending` 0).
    BackgroundWork {
        queue: WorkQueue,
        pending: u32,
    },
    /// A backup is being written: `done` of `total` files (`total` 0 while it starts).
    BackupProgress {
        done: u32,
        total: u32,
    },
    /// A backup ended: the new archive, or why it failed (also when cancelled).
    BackupFinished {
        backup: Option<BackupInfo>,
        error: Option<String>,
    },
    /// Quitting waits for a choice because a backup is running (the shell asks the UI).
    QuitDuringBackup,
}

/// A background download queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WorkQueue {
    /// Cover downloads.
    Covers,
    /// Store metadata, backgrounds and icons.
    Details,
}

type Subscriber = Arc<dyn Fn(&CoreEvent) + Send + Sync>;

/// Cheap to clone; all clones share the same subscriber list.
#[derive(Clone, Default)]
pub struct EventBus {
    subscribers: Arc<RwLock<Vec<Subscriber>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a callback for every future event. Callbacks run on the publishing thread and
    /// must be fast and non-blocking.
    pub fn subscribe(&self, callback: impl Fn(&CoreEvent) + Send + Sync + 'static) {
        self.subscribers
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Arc::new(callback));
    }

    pub fn publish(&self, event: CoreEvent) {
        tracing::debug!(?event, "core event");
        // Clone the list so callbacks may subscribe without deadlocking.
        let subscribers: Vec<Subscriber> = self
            .subscribers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for subscriber in subscribers {
            subscriber(&event);
        }
    }
}

impl EventBus {
    /// A handle that publishes while the bus lives, without keeping it alive (for workers the
    /// bus's own subscribers feed).
    pub fn downgrade(&self) -> WeakEventBus {
        WeakEventBus {
            subscribers: Arc::downgrade(&self.subscribers),
        }
    }
}

/// See [`EventBus::downgrade`].
#[derive(Clone)]
pub struct WeakEventBus {
    subscribers: std::sync::Weak<RwLock<Vec<Subscriber>>>,
}

impl WeakEventBus {
    /// Publishes on the bus if it still exists.
    pub fn publish(&self, event: CoreEvent) {
        if let Some(subscribers) = self.subscribers.upgrade() {
            EventBus { subscribers }.publish(event);
        }
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.subscribers.read().map(|s| s.len()).unwrap_or_default();
        f.debug_struct("EventBus")
            .field("subscribers", &count)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn delivers_events_to_all_subscribers() {
        let bus = EventBus::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        for _ in 0..2 {
            let seen = seen.clone();
            bus.subscribe(move |e| seen.lock().unwrap().push(e.clone()));
        }
        let id = Uuid::new_v4();
        bus.clone().publish(CoreEvent::GameStarted { id });
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn serializes_with_type_tag() {
        let id = Uuid::nil();
        let json = serde_json::to_value(CoreEvent::GameStopped {
            id,
            session_seconds: 5,
        })
        .unwrap();
        assert_eq!(json["type"], "gameStopped");
        assert_eq!(json["sessionSeconds"], 5);
    }
}
