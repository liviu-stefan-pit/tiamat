use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("db error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("plan error: {0}")]
    Plan(String),
    #[error("lease error: {0}")]
    Lease(String),
    #[error("lock error: {0}")]
    Lock(String),
    #[error("routing error: {0}")]
    Routing(String),
    #[error("attempt error: {0}")]
    Attempt(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;
