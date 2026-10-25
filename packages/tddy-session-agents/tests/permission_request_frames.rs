//! The daemon's half of a coding agent's permission request on the wire: the request rides the
//! final frame of the turn that paused on it.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A coding subagent's permission request is the caller's to answer

use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    PermissionChoice, PermissionRequestView, PermissionToolCall, PromptOutcome, StopReason,
};
use tddy_session_agents::agent_turn_frames;

#[test]
fn a_permission_request_rides_the_final_frame() {
    // Given
    let mut outcome = PromptOutcome::new(
        StopReason::PermissionRequested,
        Vec::new(),
        TokenUsage::default(),
    );
    outcome.permission_request = Some(PermissionRequestView {
        request_id: "perm-1".to_string(),
        tool_call: PermissionToolCall {
            title: "Run cargo test".to_string(),
            kind: "execute".to_string(),
            raw_input: None,
        },
        options: vec![PermissionChoice {
            option_id: "allow-once".to_string(),
            name: "Allow once".to_string(),
            kind: "allow_once".to_string(),
        }],
    });

    // When
    let frames = agent_turn_frames(&outcome);

    // Then
    let last = frames.last().expect("a final frame");
    assert!(last.last);
    assert_eq!(last.stop_reason, "PermissionRequested");
    let request: serde_json::Value =
        serde_json::from_str(&last.permission_request_json).expect("the request is JSON");
    assert_eq!(request["requestId"], "perm-1");
    assert_eq!(request["toolCall"]["title"], "Run cargo test");
}
