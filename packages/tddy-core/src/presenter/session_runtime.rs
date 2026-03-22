//! Snapshot of workflow/session status for `TddyRemote` clients (web terminal, parity with TUI status bar).

use std::time::Duration;

use crate::presenter::presenter_events::SessionRuntimeSnapshot;
use crate::presenter::state::PresenterState;

/// Compact elapsed string (aligned with `tddy-tui::ui::format_elapsed`).
pub fn format_elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m {s}s")
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h}h {m}m")
    }
}

/// Build a [`SessionRuntimeSnapshot`] from current presenter state (after state mutations, before broadcast).
pub fn session_runtime_snapshot_from_state(state: &PresenterState) -> SessionRuntimeSnapshot {
    let goal = state.current_goal.clone().unwrap_or_default();
    let workflow_state = state.current_state.clone().unwrap_or_default();
    let elapsed = state.goal_start_time.elapsed();
    let elapsed_ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
    let agent = state.agent.clone();
    let model = state.model.clone();
    let elapsed_str = format_elapsed(elapsed);
    let status_line = format!(
        "Goal: {} │ State: {} │ {} │ {} {}",
        goal, workflow_state, elapsed_str, agent, model
    );
    SessionRuntimeSnapshot {
        status_line,
        goal,
        workflow_state,
        elapsed_ms,
        agent,
        model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::state::AppMode;

    #[test]
    fn snapshot_includes_goal_and_state() {
        let mut state = PresenterState {
            agent: "stub-agent".to_string(),
            model: "stub-model".to_string(),
            mode: AppMode::Running,
            current_goal: Some("acceptance-tests".to_string()),
            current_state: Some("GreenComplete".to_string()),
            goal_start_time: std::time::Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
        };
        let snap = session_runtime_snapshot_from_state(&state);
        assert!(snap.status_line.contains("acceptance-tests"));
        assert!(snap.status_line.contains("GreenComplete"));
        assert_eq!(snap.goal, "acceptance-tests");
        assert_eq!(snap.workflow_state, "GreenComplete");
        state.current_goal = None;
        let snap2 = session_runtime_snapshot_from_state(&state);
        assert!(snap2.status_line.contains("Goal:"));
    }
}
