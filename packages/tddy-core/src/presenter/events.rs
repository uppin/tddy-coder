//! Events sent from the workflow thread to the Presenter.

use std::path::PathBuf;

use crate::{ClarificationQuestion, ProgressEvent};

/// Payload when workflow completes successfully.
#[derive(Debug, Clone)]
pub struct WorkflowCompletePayload {
    pub summary: String,
    pub plan_dir: Option<PathBuf>,
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
    /// Plan approval gate: user must View, Approve, or Refine before proceeding.
    PlanApprovalNeeded {
        prd_content: String,
    },
    WorkflowComplete(Result<WorkflowCompletePayload, String>),
    AgentOutput(String),
}
