//! TuiView: implements PresenterView for the ratatui TUI.

use tddy_core::{ActivityEntry, AppMode, PresenterView};

use crate::view_state::ViewState;

/// TUI implementation of PresenterView. Holds view-local state.
pub struct TuiView {
    pub view_state: ViewState,
}

impl TuiView {
    pub fn new() -> Self {
        Self {
            view_state: ViewState::new(),
        }
    }

    pub fn view_state(&self) -> &ViewState {
        &self.view_state
    }

    pub fn view_state_mut(&mut self) -> &mut ViewState {
        &mut self.view_state
    }
}

impl Default for TuiView {
    fn default() -> Self {
        Self::new()
    }
}

impl PresenterView for TuiView {
    fn on_mode_changed(&mut self, mode: &AppMode) {
        self.view_state.on_mode_changed(mode);
    }

    fn on_activity_logged(&mut self, _entry: &ActivityEntry, activity_log_len: usize) {
        // Auto-scroll to bottom when new activity arrives.
        // Use usize::MAX to signal "show bottom" — render clamps to max_scroll.
        if activity_log_len > 0 {
            self.view_state.scroll_offset = usize::MAX;
        }
    }

    fn on_goal_started(&mut self, _goal: &str) {
        // No-op
    }

    fn on_state_changed(&mut self, _from: &str, _to: &str) {
        // No-op
    }

    fn on_workflow_complete(
        &mut self,
        _result: &Result<tddy_core::WorkflowCompletePayload, String>,
    ) {
        // No-op
    }

    fn on_agent_output(&mut self, _text: &str) {
        // No-op
    }

    fn on_inbox_changed(&mut self, _inbox: &[String]) {
        // No-op; inbox is in PresenterState
    }
}
