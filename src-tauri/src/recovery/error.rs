use thiserror::Error;

use crate::db::DbError;
use crate::process::ProcessError;
use crate::workspace::WorkspaceError;

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("process error: {0}")]
    Process(#[from] ProcessError),
    #[error("workspace error: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("integrity failure: {0}")]
    Integrity(String),
    #[error("recovery blocked: {0}")]
    Blocked(String),
    #[error("fault injected: {0}")]
    FaultInjected(String),
    #[error("validation error: {0}")]
    Validation(String),
}

pub type RecoveryResult<T> = Result<T, RecoveryError>;
