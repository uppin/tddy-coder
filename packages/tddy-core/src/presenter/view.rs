//! PresenterView trait — callbacks for state changes.
//! Used by TuiView and apply_event in tddy-tui to map PresenterEvent to view updates.

use crate::presenter::state::{ActivityEntry, AppMode};
use crate::presenter::WorkflowCompletePayload;

/// View interface. Consumers map PresenterEvent to these callbacks when updating local state.
pub trait PresenterView {
    fn on_mode_changed(&mut self, mode: &AppMode);
    fn on_activity_logged(&mut self, entry: &ActivityEntry, activity_log_len: usize);
    fn on_goal_started(&mut self, goal: &str);
    fn on_state_changed(&mut self, from: &str, to: &str);
    fn on_workflow_complete(&mut self, result: &Result<WorkflowCompletePayload, String>);
    fn on_agent_output(&mut self, text: &str);
    fn on_inbox_changed(&mut self, inbox: &[String]);
}
