//! Internal event bus. The Tauri shell forwards every event to the UI; future plugins will
//! subscribe here too, which is why events are plain serializable data.

use std::sync::{Arc, PoisonError, RwLock};

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

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
    LibraryScanned {
        provider: String,
        summary: ScanSummary,
    },
    GameStarted {
        id: Uuid,
    },
    GameStopped {
        id: Uuid,
        #[ts(type = "number")]
        session_seconds: u64,
    },
    /// A game could not be launched or tracked.
    GameError {
        id: Uuid,
        message: String,
    },
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
