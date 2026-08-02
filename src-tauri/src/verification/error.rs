use thiserror::Error;

pub type VerificationResult<T> = Result<T, VerificationError>;

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("command policy denied: {0}")]
    PolicyDenied(String),
    #[error("test command failed to start: {0}")]
    Spawn(String),
    #[error("working directory escapes managed root: {0}")]
    PathEscape(String),
    #[error("verification: {0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
