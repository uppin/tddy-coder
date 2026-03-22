//! PresenterEvent — events broadcast to gRPC subscribers (and other listeners).
//! PresenterHandle — bridge between Presenter and gRPC service.
//! ViewConnection — snapshot + event subscription for per-connection virtual TUIs.

use std::sync::mpsc;

use tokio::sync::broadcast;

use crate::presenter::intent::UserIntent;
use crate::presenter::state::{ActivityEntry, AppMode, PresenterState};

/// Fields for [`PresenterEvent::SessionRuntimeStatus`], aligned with the TUI status bar.
#[derive(Debug, Clone)]
pub struct SessionRuntimeFields {
    pub session_id: String,
    pub goal: String,
    pub workflow_state: String,
    pub elapsed: std::time::Duration,
    pub agent: String,
    pub model: String,
}

/// Events the Presenter broadcasts to subscribers (e.g. gRPC clients).
/// Mirrors PresenterView callbacks for remote observation.
#[derive(Debug, Clone)]
pub enum PresenterEvent {
    ModeChanged(AppMode),
    ActivityLogged(ActivityEntry),
    GoalStarted(String),
    StateChanged {
        from: String,
        to: String,
    },
    WorkflowComplete(Result<crate::presenter::WorkflowCompletePayload, String>),
    AgentOutput(String),
    InboxChanged(Vec<String>),
    IntentReceived(UserIntent),
    /// User confirmed coding backend at session start (before workflow runs).
    BackendSelected {
        agent: String,
        model: String,
    },
    /// TUI-equivalent status snapshot for remote viewers (gRPC / LiveKit).
    SessionRuntimeStatus(SessionRuntimeFields),
    /// Presenter requested the view loop to exit (e.g. successful Continue with agent).
    /// Distinct from [`IntentReceived`](PresenterEvent::IntentReceived): sent after session resolution.
    ShouldQuit,
}

/// Handle passed to gRPC service: broadcast sender for events, mpsc sender for intents.
#[derive(Clone)]
pub struct PresenterHandle {
    pub event_tx: tokio::sync::broadcast::Sender<PresenterEvent>,
    pub intent_tx: mpsc::Sender<UserIntent>,
}

/// Connection for a newly attached view (e.g. per-connection virtual TUI).
/// Provides state snapshot and event subscription so the view can initialize and receive updates.
pub struct ViewConnection {
    /// Current state at connection time; view uses this for initial render.
    pub state_snapshot: PresenterState,
    /// Subscription to live events; view updates its local state from these.
    pub event_rx: broadcast::Receiver<PresenterEvent>,
    /// Sender for UserIntents; view forwards user input here.
    pub intent_tx: mpsc::Sender<UserIntent>,
}
