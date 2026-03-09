//! PresenterView trait — callbacks for state changes.

use crate::presenter::state::{ActivityEntry, AppMode};

/// View interface. Presenter calls these methods when state changes.
pub trait PresenterView {
    fn on_mode_changed(&mut self, mode: &AppMode);
    fn on_activity_logged(&mut self, entry: &ActivityEntry);
    fn on_goal_started(&mut self, goal: &str);
    fn on_state_changed(&mut self, from: &str, to: &str);
    fn on_workflow_complete(&mut self, result: &Result<String, String>);
    fn on_agent_output(&mut self, text: &str);
    fn on_inbox_changed(&mut self, inbox: &[String]);
}
