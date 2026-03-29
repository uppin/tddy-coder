//! PresenterEvent — events broadcast to gRPC subscribers (and other listeners).
//! PresenterHandle — bridge between Presenter and gRPC service.
//! ViewConnection — snapshot + event subscription for per-connection virtual TUIs.

use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};

use tokio::sync::broadcast;

use crate::presenter::intent::UserIntent;
use crate::presenter::state::{ActivityEntry, AppMode, CriticalPresenterState, PresenterState};

/// Payload for [`PresenterEvent::ModeChanged`]: mode plus fields that are not inferable from [`AppMode`] alone.
#[derive(Debug, Clone)]
pub struct ModeChangedDetails {
    pub mode: AppMode,
    /// When true, the user is entering plan refinement text via the prompt bar while the PRD stays visible.
    pub plan_refinement_pending: bool,
    /// Workflow output directory for `.agents/skills` resolution in the feature prompt slash menu.
    pub skills_project_root: Option<PathBuf>,
}

/// Events the Presenter broadcasts to subscribers (e.g. gRPC clients).
/// Mirrors PresenterView callbacks for remote observation.
#[derive(Debug, Clone)]
pub enum PresenterEvent {
    ModeChanged(ModeChangedDetails),
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
    /// Presenter requested the view loop to exit (e.g. successful Continue with agent).
    /// Distinct from [`IntentReceived`](PresenterEvent::IntentReceived): sent after session resolution.
    ShouldQuit,
}

/// Handle passed to gRPC service: broadcast sender for events, mpsc sender for intents.
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
    /// Shared critical state for lag recovery. When the broadcast overflows,
    /// the TUI resyncs goal and workflow state from this instead of relying
    /// on lost `GoalStarted`/`StateChanged` events.
    pub critical_state: Arc<Mutex<CriticalPresenterState>>,
}
