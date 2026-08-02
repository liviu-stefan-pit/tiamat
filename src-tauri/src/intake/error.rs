use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntakeError {
    #[error("path does not exist: {0}")]
    NotFound(PathBuf),
    #[error("unsupported path form: {0}")]
    UnsupportedPath(String),
    #[error("path escape rejected: {0}")]
    PathEscape(String),
    #[error("alternate data stream rejected: {0}")]
    AlternateDataStream(String),
    #[error("intake limits exceeded: {0}")]
    LimitsExceeded(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid intake: {0}")]
    Invalid(String),
}

pub type IntakeResult<T> = Result<T, IntakeError>;
