//! Workflow layer: the vocabulary a workflow is described in, plus session artifact layout.
//!
//! The shared data types every layer names — goal and state ids, the questions a backend asks, the
//! progress a run reports, the events a workflow sends — live here rather than inside whichever
//! behaviour module happened to define them. That is what lets `tddy-core`'s modules name a DTO
//! without naming each other.

pub mod artifact_paths;
pub mod events;
pub mod hints;
pub mod ids;
pub mod progress;
pub mod questions;

pub use artifact_paths::{
    canonical_artifact_write_path, canonical_attachment_write_path, list_session_attachments,
    read_session_artifact_utf8, read_session_artifact_utf8_or_placeholder,
    resolve_existing_session_artifact, session_artifacts_root, session_attachments_root,
    SessionAttachmentFile, SESSION_ARTIFACT_READ_PLACEHOLDER, SESSION_ATTACHMENTS_SUBDIR,
};

pub use events::{WorkflowCompletePayload, WorkflowEvent};
pub use hints::{GoalHints, PermissionHint};
pub use ids::{GoalId, WorkflowState};
pub use progress::ProgressEvent;
pub use questions::{ClarificationQuestion, QuestionOption};
