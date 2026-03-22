//! Presenter module: application state and workflow orchestration (MVP pattern).

mod events;
mod intent;
mod presenter_events;
mod presenter_impl;
mod session_runtime;
mod state;
mod view;
mod workflow_runner;

pub use events::WorkflowCompletePayload;
pub use events::WorkflowEvent;

pub use intent::UserIntent;
pub use presenter_events::{
    PresenterEvent, PresenterHandle, SessionRuntimeSnapshot, ViewConnection,
};
pub use presenter_impl::{PendingWorkflowStart, Presenter};
pub use session_runtime::session_runtime_snapshot_from_state;
pub use state::{ActivityEntry, ActivityKind, AppMode, ExitAction, PresenterState};
pub use view::PresenterView;
