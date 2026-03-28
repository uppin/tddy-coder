//! Events sent from the workflow thread to the Presenter.

use std::path::PathBuf;

use crate::{ClarificationQuestion, ProgressEvent};

/// Payload when workflow completes successfully.
#[derive(Debug, Clone)]
pub struct WorkflowCompletePayload {
    pub summary: String,
    pub session_dir: Option<PathBuf>,
}

/// Events the workflow thread sends to the Presenter.
#[derive(Debug)]
#[allow(dead_code)] // Progress used when workflow emits progress events
pub enum WorkflowEvent {
    Progress(ProgressEvent),
    StateChange {
        from: String,
        to: String,
    },
    GoalStarted(String),
    ClarificationNeeded {
        questions: Vec<ClarificationQuestion>,
    },
    /// Session document approval: user must View, Approve, or Refine before proceeding.
    SessionDocumentApprovalNeeded {
        content: String,
    },
    /// Plan needs `feature_input` but none came from CLI or `changeset.yaml`; block on `answer_rx`.
    /// Presenter should switch to [`crate::presenter::state::AppMode::FeatureInput`].
    AwaitingFeatureInput,
    WorkflowComplete(Result<WorkflowCompletePayload, String>),
    AgentOutput(String),
    /// Worktree created and switched after plan approval. Path is the worktree directory.
    WorktreeSwitched {
        path: PathBuf,
    },
}
