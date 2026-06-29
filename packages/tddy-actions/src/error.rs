use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("invalid action spec: {0}")]
    InvalidSpec(String),
    #[error("unknown action kind: {0}")]
    UnknownKind(String),
    #[error("spawn failed: {0}")]
    Spawn(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
