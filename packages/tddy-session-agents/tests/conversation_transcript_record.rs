//! A daemon-run conversation's finished turn, recorded as the ACP frames the web replays.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    ContentBlock, PermissionChoice, PermissionRequestView, PermissionToolCall, PromptOutcome,
    StopReason,
};
use tddy_service::proto::acp::{acp_agent_message, session_update, AcpAgentMessage};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;
use tddy_session_agents::conversation_transcript::record_turn;

/// What each recorded frame is, as `user:<text>`, `agent:<text>`, `tool:<title>` or
/// `permission:<title>`.
fn shapes(frames: &[AcpAgentMessage]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(notification)) => notification
                .update
                .as_ref()
                .and_then(|update| update.update.as_ref())
                .map(|update| match update {
                    session_update::Update::UserMessageChunk(chunk) => {
                        format!("user:{}", text_of(chunk))
                    }
                    session_update::Update::AgentMessageChunk(chunk) => {
                        format!("agent:{}", text_of(chunk))
                    }
                    session_update::Update::ToolCall(call) => format!("tool:{}", call.title),
                    other => format!("other:{other:?}"),
                }),
            _ => None,
        })
        .collect()
}

fn text_of(chunk: &tddy_service::proto::acp::ContentChunk) -> String {
    use tddy_service::proto::acp::content_block::Block;
    match chunk.content.as_ref().and_then(|c| c.block.as_ref()) {
        Some(Block::Text(text)) => text.text.clone(),
        _ => String::new(),
    }
}

#[test]
fn a_finished_turn_records_its_prompt_and_answer() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");
    let outcome = PromptOutcome::new(
        StopReason::EndTurn,
        vec![ContentBlock::text("main is in src/main.rs")],
        TokenUsage::default(),
    );

    // When
    record_turn(
        session_dir.path(),
        "conv-1",
        Some("where is main?"),
        &outcome,
        1_000,
    )
    .expect("the turn is recorded");

    // Then
    let frames = read_conversation_transcript(session_dir.path(), "conv-1")
        .expect("read")
        .expect("recorded");
    assert_eq!(
        shapes(&frames),
        ["user:where is main?", "agent:main is in src/main.rs"]
    );
}

#[test]
fn a_turn_paused_on_a_permission_request_records_the_request() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");
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
    record_turn(
        session_dir.path(),
        "conv-1",
        Some("test it"),
        &outcome,
        1_000,
    )
    .expect("the turn is recorded");

    // Then
    let frames = read_conversation_transcript(session_dir.path(), "conv-1")
        .expect("read")
        .expect("recorded");
    assert!(
        shapes(&frames).contains(&"tool:Permission: Run cargo test".to_string()),
        "the pending request is an entry naming the tool: {:?}",
        shapes(&frames)
    );
}
