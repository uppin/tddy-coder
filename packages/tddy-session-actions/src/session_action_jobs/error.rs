use thiserror::Error;

use crate::session_actions::SessionActionsError;

/// Errors surfaced by [`super::invoke_session_action`], wait, stop, and the on-disk registry.
#[derive(Debug, Error)]
pub enum SessionActionJobsError {
    #[error("unknown session action job: {0}")]
    UnknownJob(String),

    #[error(transparent)]
    Action(#[from] SessionActionsError),

    #[error("changeset read failed: {0}")]
    ChangesetRead(String),

    #[error("session action job I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("session action job state invalid: {0}")]
    JobState(String),
}

impl SessionActionJobsError {
    /// Stable wire/CLI discriminator (agents parse this, not prose).
    pub fn stable_code(&self) -> &'static str {
        match self {
            SessionActionJobsError::UnknownJob(_) => "unknown_job",
            SessionActionJobsError::Action(_) => "session_actions_error",
            SessionActionJobsError::ChangesetRead(_) => "changeset_read",
            SessionActionJobsError::Io(_) => "io_error",
            SessionActionJobsError::JobState(_) => "job_state",
        }
    }
}
