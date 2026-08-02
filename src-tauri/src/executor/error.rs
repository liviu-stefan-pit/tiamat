use thiserror::Error;

pub type ExecutorResult<T> = Result<T, ExecutorError>;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("executor: {0}")]
    Message(String),
    #[error("boundary escape: {0}")]
    BoundaryEscape(String),
    #[error("invalid phase result: {0}")]
    InvalidPhaseResult(String),
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("checkpoint refused: {0}")]
    CheckpointRefused(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Workspace(#[from] crate::workspace::WorkspaceError),
}
