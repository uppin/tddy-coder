//! Events sent from the workflow thread to the Presenter.

use crate::{ClarificationQuestion, ProgressEvent};

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
    WorkflowComplete(Result<String, String>),
    DemoPrompt,
    AgentOutput(String),
}
