/// Core error types for the deterministic event-log engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "native")]
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[cfg(not(feature = "native"))]
    #[error("Store error: {0}")]
    Sqlite(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid command: {0}")]
    InvalidCommand(String),

    #[error("Event sequence violation: expected seq {expected}, got {actual}")]
    SequenceViolation { expected: u64, actual: u64 },

    #[error("Nothing to undo (at event seq 0)")]
    NothingToUndo,

    #[error("Nothing to redo (at latest event seq {seq})")]
    NothingToRedo { seq: u64 },

    #[error("Event not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
