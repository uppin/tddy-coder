//! Presenter module: application state and workflow orchestration (MVP pattern).

mod activity_prompt_log;
mod agent_activity;
pub mod agent_output_log_merge;
pub mod agent_session_runner;
mod events;
mod intent;
mod presenter_events;
mod presenter_impl;
#[cfg(test)]
mod presenter_test_recipe;
mod state;
pub mod state_groups;
mod view;
pub mod workflow_runner;
mod worktree_display;

pub use events::WorkflowCompletePayload;
pub use events::WorkflowEvent;

pub use agent_output_log_merge::AgentOutputActivityLogMerge;
pub use agent_session_runner::{
    agent_driven_enabled, backend_is_agent_driven, run_agent_session, AgentSessionConfig,
};
pub use intent::UserIntent;
pub use presenter_events::{ModeChangedDetails, PresenterEvent, PresenterHandle, ViewConnection};
pub use presenter_impl::{PendingWorkflowStart, Presenter};
pub use state::{
    ActivityEntry, ActivityKind, AppMode, CriticalPresenterState, ExitAction, PresenterState,
};
pub use state_groups::{
    ActivityRecorder, BackendSelection, PendingQuestions, PendingToolCallResponse,
    RecipeResolverFn, ViewChannels, WorkflowRun,
};
pub use view::PresenterView;
pub use worktree_display::format_worktree_for_status_bar;
