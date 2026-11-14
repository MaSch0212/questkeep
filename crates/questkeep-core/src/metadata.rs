//! Computing the metadata the library shows from the stored sources, shared by the library and
//! the metadata fetcher.

use std::sync::{Arc, PoisonError, RwLock};

use uuid::Uuid;

use crate::db::Database;
use crate::error::Result;
use crate::model::{HLTB_SOURCE, IGDB_SOURCE, MetadataPriority};
use crate::providers::ProviderRegistry;

/// Recomputes a library entry's shown metadata with the user's current source priority.
#[derive(Clone)]
pub struct MetadataResolver {
    db: Arc<Database>,
    priority: Arc<RwLock<MetadataPriority>>,
    /// The stores; plugins add and remove some while the app runs.
    providers: ProviderRegistry,
}

impl MetadataResolver {
    pub fn new(
        db: Arc<Database>,
        providers: &ProviderRegistry,
        priority: MetadataPriority,
    ) -> Self {
        Self {
            db,
            priority: Arc::new(RwLock::new(priority)),
            providers: providers.clone(),
        }
    }

    /// The movable sources: every store, IGDB and HowLongToBeat.
    pub fn sources(&self) -> Vec<String> {
        let mut sources: Vec<String> = self.providers.iter().map(|p| p.id().to_string()).collect();
        sources.push(IGDB_SOURCE.to_string());
        sources.push(HLTB_SOURCE.to_string());
        sources
    }

    pub fn priority(&self) -> MetadataPriority {
        self.priority
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Sets the priority; returns whether it changed (then call [`resolve_all`](Self::resolve_all)).
    pub fn set_priority(&self, priority: MetadataPriority) -> bool {
        let mut current = self
            .priority
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if *current == priority {
            return false;
        }
        *current = priority;
        true
    }

    /// Recomputes the entry a game row belongs to.
    pub fn resolve(&self, game_id: Uuid) -> Result<()> {
        let entry = self.db.entry_of(game_id)?;
        self.db
            .resolve_entry(entry, &self.priority(), &self.sources())
    }

    /// Recomputes every entry.
    pub fn resolve_all(&self) -> Result<usize> {
        self.db.resolve_all(&self.priority(), &self.sources())
    }
}
