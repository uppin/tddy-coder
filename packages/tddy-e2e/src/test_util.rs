//! Test utilities for E2E tests.

use tddy_core::{ActivityEntry, AppMode, PresenterView};

/// Minimal PresenterView for tests (no-op).
pub struct NoopView;

impl PresenterView for NoopView {
    fn on_mode_changed(&mut self, _mode: &AppMode) {}
    fn on_activity_logged(&mut self, _entry: &ActivityEntry, _activity_log_len: usize) {}
    fn on_goal_started(&mut self, _goal: &str) {}
    fn on_state_changed(&mut self, _from: &str, _to: &str) {}
    fn on_workflow_complete(&mut self, _result: &Result<String, String>) {}
    fn on_agent_output(&mut self, _text: &str) {}
    fn on_inbox_changed(&mut self, _inbox: &[String]) {}
}
