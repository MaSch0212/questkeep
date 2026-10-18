use uuid::Uuid;

/// Errors surfaced by the core. The `Display` text is shown to users, so keep it readable.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Game not found ({0})")]
    GameNotFound(Uuid),
    #[error("Unknown library provider '{0}'")]
    UnknownProvider(String),
    #[error("{provider} scan failed: {message}")]
    Provider { provider: String, message: String },
    #[error("Could not launch game: {0}")]
    Launch(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
