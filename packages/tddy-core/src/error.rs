//! Error types for tddy-core.

use crate::backend::ClarificationQuestion;
use thiserror::Error;

/// Errors from coding backends (Claude Code CLI, etc.).
#[derive(Error, Debug, Clone)]
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

    #[error("plan directory invalid: {0}")]
    PlanDirInvalid(String),

    #[error("session file missing: {0}")]
    SessionMissing(String),

    #[error("changeset.yaml missing: {0}")]
    ChangesetMissing(String),

    #[error("changeset.yaml invalid: {0}")]
    ChangesetInvalid(String),

    /// Clarification questions from the LLM; caller should display and re-invoke with answers.
    #[error("clarification needed: {questions:?}")]
    ClarificationNeeded {
        questions: Vec<ClarificationQuestion>,
        session_id: String,
    },
}

/// Errors from parsing LLM output.
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("missing primary document section")]
    MissingPrimaryDocumentSection,

    #[error("missing TODO section")]
    MissingTodo,

    #[error("malformed output: {0}")]
    Malformed(String),
}
