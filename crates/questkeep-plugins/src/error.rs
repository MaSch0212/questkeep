//! Plugin errors. The `Display` text is shown to users.

use std::path::Path;

use questkeep_core::CoreError;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// The plugin, its manifest or a request about it is not acceptable; the text says why.
    #[error("{0}")]
    Invalid(String),
    #[error("Could not access {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("There is no plugin \"{0}\".")]
    NotFound(String),
    /// A store of the plugin could not be loaded.
    #[error("{0}")]
    Load(String),
}

impl PluginError {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        PluginError::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

pub type Result<T, E = PluginError> = std::result::Result<T, E>;

impl From<PluginError> for CoreError {
    fn from(err: PluginError) -> Self {
        CoreError::Invalid(err.to_string())
    }
}
