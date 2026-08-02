use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("run not found: {0}")]
    RunNotFound(String),
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error("duplicate event id: {0}")]
    DuplicateEvent(String),
    #[error("migration error: {0}")]
    Migration(String),
    #[error("integrity check failed: {0}")]
    Integrity(String),
    #[error("validation error: {0}")]
    Validation(String),
}

pub type DbResult<T> = Result<T, DbError>;
