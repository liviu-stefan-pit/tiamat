use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("process registry: {0}")]
    Registry(String),
    #[error("job object: {0}")]
    Job(String),
    #[error("spawn failed: {0}")]
    Spawn(String),
    #[error("stop failed: {0}")]
    Stop(String),
    #[error("identity mismatch: {0}")]
    Identity(String),
    #[error("cleanup incomplete: {0}")]
    Cleanup(String),
    #[error("unsupported platform: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Db(#[from] crate::db::DbError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type ProcessResult<T> = Result<T, ProcessError>;
