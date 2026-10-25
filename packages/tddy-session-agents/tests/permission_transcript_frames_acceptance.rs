//! A turn that paused on a permission request records it as a tool call the web reads by its id, with an
//! input that says whether the main agent handed the request to the user.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    PermissionChoice, PermissionRequestView, PermissionToolCall, PromptOutcome, StopReason,
};
use tddy_service::proto::acp::{acp_agent_message, session_update, AcpAgentMessage, ToolCall};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;
use tddy_session_agents::conversation_transcript::record_turn;

fn a_turn_paused_on_perm_1() -> PromptOutcome {
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
    outcome
}

/// The tool call a frame carries under `tool_call_id`, if any frame does.
fn tool_call_named(frames: &[AcpAgentMessage], tool_call_id: &str) -> Option<ToolCall> {
    frames.iter().find_map(|frame| match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(note)) => {
            match note
                .update
                .as_ref()
                .and_then(|update| update.update.as_ref())
            {
                Some(session_update::Update::ToolCall(call))
                    if call.tool_call_id.as_ref().map(|id| id.value.as_str())
                        == Some(tool_call_id) =>
                {
                    Some(call.clone())
                }
                _ => None,
            }
        }
        _ => None,
    })
}

#[test]
fn a_turn_paused_on_a_permission_request_records_it_under_the_permission_id() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");

    // When
    record_turn(
        session_dir.path(),
        "conv-1",
        Some("test it"),
        &a_turn_paused_on_perm_1(),
        1_000,
    )
    .expect("the turn is recorded");

    // Then
    let frames = read_conversation_transcript(session_dir.path(), "conv-1")
        .expect("read")
        .expect("recorded");
    let call = tool_call_named(&frames, "permission:perm-1").unwrap_or_else(|| {
        panic!("the request is recorded under permission:perm-1, not the bare id: {frames:?}")
    });
    let input: serde_json::Value =
        serde_json::from_str(call.raw_input.as_deref().expect("the request's input"))
            .expect("the input is JSON");
    assert_eq!(input["requestId"], "perm-1");
    assert_eq!(input["title"], "Run cargo test");
    assert_eq!(input["deferredToUser"], false);
}
