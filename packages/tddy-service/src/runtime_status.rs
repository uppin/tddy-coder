//! Session runtime status: proto snapshots aligned with the TUI status bar.

use std::time::Duration;

use tddy_core::PresenterEvent;

use crate::gen::{server_message, ServerMessage, SessionRuntimeStatus};

/// Verbatim status line for [`crate::gen::SessionRuntimeStatus::status_line`], matching
/// [`tddy_tui::ui::format_status_bar`] for the same logical fields.
pub fn session_runtime_status_status_line(
    goal: &str,
    workflow_state: &str,
    elapsed: Duration,
    agent: &str,
    model: &str,
) -> String {
    log::debug!(
        "session_runtime_status_status_line goal={} state={} elapsed_ms={}",
        goal,
        workflow_state,
        elapsed.as_millis()
    );
    tddy_tui::ui::format_status_bar(goal, workflow_state, elapsed, agent, model)
}

/// Build a [`SessionRuntimeStatus`] protobuf from discrete fields.
pub fn build_session_runtime_status_proto(
    session_id: &str,
    goal: &str,
    workflow_state: &str,
    elapsed: Duration,
    agent: &str,
    model: &str,
) -> SessionRuntimeStatus {
    log::debug!(
        "build_session_runtime_status_proto session_id_len={} goal_len={}",
        session_id.len(),
        goal.len()
    );
    SessionRuntimeStatus {
        session_id: session_id.to_string(),
        goal: goal.to_string(),
        workflow_state: workflow_state.to_string(),
        elapsed_ms: elapsed.as_millis() as u64,
        agent: agent.to_string(),
        model: model.to_string(),
        status_line: session_runtime_status_status_line(
            goal,
            workflow_state,
            elapsed,
            agent,
            model,
        ),
    }
}

/// Returns a [`ServerMessage`] with [`SessionRuntimeStatus`] when the presenter event implies a status snapshot.
pub fn maybe_emit_session_runtime_status(event: &PresenterEvent) -> Option<ServerMessage> {
    match event {
        PresenterEvent::SessionRuntimeStatus(fields) => {
            let proto = build_session_runtime_status_proto(
                &fields.session_id,
                &fields.goal,
                &fields.workflow_state,
                fields.elapsed,
                &fields.agent,
                &fields.model,
            );
            Some(ServerMessage {
                event: Some(server_message::Event::SessionRuntimeStatus(proto)),
            })
        }
        PresenterEvent::GoalStarted(goal) => {
            let proto =
                build_session_runtime_status_proto("", goal, "Starting", Duration::ZERO, "", "");
            Some(ServerMessage {
                event: Some(server_message::Event::SessionRuntimeStatus(proto)),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use tddy_tui::ui::format_status_bar;

    #[test]
    fn tui_status_line_matches_format_status_bar() {
        let goal = "acceptance-tests";
        let workflow_state = "GreenComplete";
        let elapsed = Duration::from_secs(125);
        let agent = "composer";
        let model = "opus-4";
        let expected = format_status_bar(goal, workflow_state, elapsed, agent, model);
        let got = session_runtime_status_status_line(goal, workflow_state, elapsed, agent, model);
        assert_eq!(
            got, expected,
            "SessionRuntimeStatus.status_line must match format_status_bar output byte-for-byte"
        );
    }

    #[test]
    fn build_session_runtime_status_proto_elapsed_ms_matches_duration() {
        let elapsed = Duration::from_secs(125);
        let proto =
            build_session_runtime_status_proto("sid", "goal", "state", elapsed, "agent", "model");
        assert_eq!(
            proto.elapsed_ms,
            elapsed.as_millis() as u64,
            "elapsed_ms must reflect the snapshot duration for remote viewers"
        );
    }

    #[test]
    fn build_session_runtime_status_proto_status_line_matches_format_status_bar() {
        let goal = "unit-test-goal";
        let workflow_state = "Running";
        let elapsed = Duration::from_secs(42);
        let agent = "stub-agent";
        let model = "stub-model";
        let proto =
            build_session_runtime_status_proto("sid", goal, workflow_state, elapsed, agent, model);
        let expected = format_status_bar(goal, workflow_state, elapsed, agent, model);
        assert_eq!(
            proto.status_line, expected,
            "status_line must match format_status_bar (also covered by session_runtime_status_status_line)"
        );
    }

    #[test]
    fn presenter_event_hook_emits_session_runtime_status_for_goal_started() {
        let ev = PresenterEvent::GoalStarted("feature".to_string());
        assert!(
            maybe_emit_session_runtime_status(&ev).is_some(),
            "GoalStarted should yield a SessionRuntimeStatus snapshot for gRPC subscribers"
        );
    }
}
