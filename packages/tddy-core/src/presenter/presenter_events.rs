//! PresenterEvent — events broadcast to gRPC subscribers (and other listeners).
//! PresenterHandle — bridge between Presenter and gRPC service.

use std::sync::mpsc;

use crate::presenter::intent::UserIntent;
use crate::presenter::state::{ActivityEntry, AppMode};

/// Events the Presenter broadcasts to subscribers (e.g. gRPC clients).
/// Mirrors PresenterView callbacks for remote observation.
#[derive(Debug, Clone)]
pub enum PresenterEvent {
    ModeChanged(AppMode),
    ActivityLogged(ActivityEntry),
    GoalStarted(String),
    StateChanged { from: String, to: String },
    WorkflowComplete(Result<crate::presenter::WorkflowCompletePayload, String>),
    AgentOutput(String),
    InboxChanged(Vec<String>),
    IntentReceived(UserIntent),
}

/// Handle passed to gRPC service: broadcast sender for events, mpsc sender for intents.
pub struct PresenterHandle {
    pub event_tx: tokio::sync::broadcast::Sender<PresenterEvent>,
    pub intent_tx: mpsc::Sender<UserIntent>,
}
