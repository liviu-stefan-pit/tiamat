use std::path::PathBuf;

use thiserror::Error;

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("git command failed ({command}): {detail}")]
    Git { command: String, detail: String },

    #[error("source fingerprint mismatch: {0}")]
    SourceMutated(String),

    #[error("path escapes managed roots: {0}")]
    PathEscape(String),

    #[error("invalid write root: {0}")]
    InvalidWriteRoot(String),

    #[error("invalid read root: {0}")]
    InvalidReadRoot(String),

    #[error("workspace not found: {0}")]
    NotFound(PathBuf),

    #[error("retention blocked: {0}")]
    RetentionBlocked(String),

    #[error("promotion required before cleanup: {0}")]
    UnpromotedWork(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("{0}")]
    Message(String),
}
