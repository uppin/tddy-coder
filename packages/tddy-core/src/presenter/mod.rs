//! Presenter module: application state and workflow orchestration (MVP pattern).

mod activity_prompt_log;
mod agent_activity;
mod events;
mod intent;
mod presenter_events;
mod presenter_impl;
#[cfg(test)]
mod presenter_test_recipe;
mod state;
mod view;
pub mod workflow_runner;
mod worktree_display;

pub use events::WorkflowCompletePayload;
pub use events::WorkflowEvent;

pub use intent::UserIntent;
pub use presenter_events::{ModeChangedDetails, PresenterEvent, PresenterHandle, ViewConnection};
pub use presenter_impl::{PendingWorkflowStart, Presenter};
pub use state::{
    ActivityEntry, ActivityKind, AppMode, CriticalPresenterState, ExitAction, PresenterState,
};
pub use view::PresenterView;
pub use worktree_display::format_worktree_for_status_bar;
