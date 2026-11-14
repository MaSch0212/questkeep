//! Auto categories: whenever games are added or change (a sync, new metadata, an edit), a worker
//! thread gives them the categories whose filter they now match. See
//! [`Database::apply_auto_categories`] for what is left alone.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc;

use uuid::Uuid;

use crate::db::Database;
use crate::events::{CoreEvent, EventBus, WeakEventBus};

/// Starts the worker; it ends with the event bus.
pub fn watch(db: Arc<Database>, events: &EventBus) {
    let (sender, receiver) = mpsc::channel::<Vec<Uuid>>();
    let bus = events.downgrade();
    let spawned = std::thread::Builder::new()
        .name("auto-categories".into())
        .spawn(move || run(&db, &bus, &receiver));
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the auto category worker");
        return;
    }
    events.subscribe(move |event| {
        if let CoreEvent::GamesChanged { ids } = event {
            let _ = sender.send(ids.clone());
        }
    });
}

fn run(db: &Database, bus: &WeakEventBus, receiver: &mpsc::Receiver<Vec<Uuid>>) {
    while let Ok(first) = receiver.recv() {
        // Changes arriving together (a sync) are handled at once.
        let mut ids: HashSet<Uuid> = first.into_iter().collect();
        while let Ok(more) = receiver.try_recv() {
            ids.extend(more);
        }
        let ids: Vec<Uuid> = ids.into_iter().collect();
        match db.apply_auto_categories(Some(&ids)) {
            Ok(changed) if !changed.is_empty() => {
                tracing::info!(games = changed.len(), "gave games their auto categories");
                bus.publish(CoreEvent::CategoriesChanged);
                // Heard by this worker again; nothing changes the second time.
                bus.publish(CoreEvent::GamesChanged { ids: changed });
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(%err, "could not apply auto categories"),
        }
    }
}
