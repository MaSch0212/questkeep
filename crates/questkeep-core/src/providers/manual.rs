//! Games added by hand. They are never scanned; the provider exists so the source is registered
//! like any other.

use super::{LibraryProvider, ProviderError};
use crate::model::{ImportedGame, MANUAL_SOURCE};

pub struct ManualProvider;

impl LibraryProvider for ManualProvider {
    fn id(&self) -> &str {
        MANUAL_SOURCE
    }

    fn name(&self) -> &str {
        "Manual"
    }

    fn can_scan(&self) -> bool {
        false
    }

    fn scan_installed(&self) -> Result<Vec<ImportedGame>, ProviderError> {
        Err(ProviderError::Unsupported)
    }
}
