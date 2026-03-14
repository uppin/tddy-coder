//! PresenterView trait — callbacks for state changes.

use crate::presenter::state::{ActivityEntry, AppMode};
use crate::presenter::WorkflowCompletePayload;

/// No-op view for headless/daemon mode where no TUI is rendered.
#[derive(Debug, Default)]
pub struct NoopView;

impl PresenterView for NoopView {
    fn on_mode_changed(&mut self, _mode: &AppMode) {}
    fn on_activity_logged(&mut self, _entry: &ActivityEntry, _activity_log_len: usize) {}
    fn on_goal_started(&mut self, _goal: &str) {}
    fn on_state_changed(&mut self, _from: &str, _to: &str) {}
    fn on_workflow_complete(&mut self, _result: &Result<WorkflowCompletePayload, String>) {}
    fn on_agent_output(&mut self, _text: &str) {}
    fn on_inbox_changed(&mut self, _inbox: &[String]) {}
}

/// View interface. Presenter calls these methods when state changes.
pub trait PresenterView {
    fn on_mode_changed(&mut self, mode: &AppMode);
    fn on_activity_logged(&mut self, entry: &ActivityEntry, activity_log_len: usize);
    fn on_goal_started(&mut self, goal: &str);
    fn on_state_changed(&mut self, from: &str, to: &str);
    fn on_workflow_complete(&mut self, result: &Result<WorkflowCompletePayload, String>);
    fn on_agent_output(&mut self, text: &str);
    fn on_inbox_changed(&mut self, inbox: &[String]);
}
