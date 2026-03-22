//! Presenter module: application state and workflow orchestration (MVP pattern).

mod events;
mod intent;
mod presenter_events;
mod presenter_impl;
#[cfg(test)]
mod presenter_test_recipe;
mod state;
mod view;
pub mod workflow_runner;

pub use events::WorkflowCompletePayload;
pub use events::WorkflowEvent;

pub use intent::UserIntent;
pub use presenter_events::{PresenterEvent, PresenterHandle, ViewConnection};
pub use presenter_impl::{PendingWorkflowStart, Presenter};
pub use state::{ActivityEntry, ActivityKind, AppMode, ExitAction, PresenterState};
pub use view::PresenterView;
