//! Error types for tddy-core.

use thiserror::Error;

/// Errors from coding backends (Claude Code CLI, etc.).
#[derive(Error, Debug)]
pub enum BackendError {
    #[error("backend invocation failed: {0}")]
    InvocationFailed(String),

    #[error("binary not found: {0}")]
    BinaryNotFound(String),
}

/// Errors from the workflow state machine.
#[derive(Error, Debug)]
pub enum WorkflowError {
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),

    #[error("backend error: {0}")]
    Backend(#[from] BackendError),

    #[error("output parsing failed: {0}")]
    ParseError(#[from] ParseError),

    #[error("artifact write failed: {0}")]
    WriteFailed(String),

    /// Clarification questions from the LLM; caller should display and re-invoke with answers.
    #[error("clarification needed: {questions:?}")]
    ClarificationNeeded { questions: Vec<String> },
}

/// Errors from parsing LLM output.
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("missing PRD section")]
    MissingPrd,

    #[error("missing TODO section")]
    MissingTodo,

    #[error("malformed output: {0}")]
    Malformed(String),
}
