//! Recording a daemon-run conversation's turns, so the web can replay the conversation itself.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::io;
use std::path::Path;

use tddy_discovery::subagent::{
    MessageDescriptor, MessageRole, PermissionRequestView, PromptOutcome, StopReason,
};
use tddy_service::acp_replay::{agent_text_frame, session_update_frame};
use tddy_service::convert_acp::{prompt_response, user_message_chunk};
use tddy_service::proto::acp::{
    session_update, AcpAgentMessage, SessionUpdate, StopReason as AcpStopReason, ToolCall,
    ToolCallId, ToolCallStatus, ToolKind,
};
use tddy_service::subagent_conversation_replay::append_conversation_frame;

/// Record one finished turn of `conversation_id` under `session_dir`: the prompt that started it
/// (`None` for a resume), the tool calls it made, its text, a pending permission request, and the stop
/// reason — as ACP frames stamped `at_unix_ms`.
///
/// Called once per turn, with the turn's outcome. Nothing is streamed into the transcript alongside it,
/// so a turn's frames are written exactly once.
pub fn record_turn(
    session_dir: &Path,
    conversation_id: &str,
    prompt: Option<&str>,
    outcome: &PromptOutcome,
    at_unix_ms: i64,
) -> io::Result<()> {
    for frame in turn_frames(prompt, outcome, at_unix_ms) {
        append_conversation_frame(session_dir, conversation_id, &frame)?;
    }
    Ok(())
}

/// The wall-clock stamp a turn is recorded at: now, in milliseconds since the Unix epoch.
pub fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// The frames of one turn in the order it happened: the prompt, the tool calls, the answer, the pending
/// permission request, then the stop reason.
fn turn_frames(prompt: Option<&str>, outcome: &PromptOutcome, at: i64) -> Vec<AcpAgentMessage> {
    let mut frames = Vec::new();
    if let Some(prompt) = prompt {
        frames.push(session_update_frame(
            user_message_chunk(prompt.to_string()),
            at,
        ));
    }
    frames.extend(
        outcome
            .messages
            .iter()
            .filter(|message| matches!(message.role, MessageRole::Tool))
            .map(|message| tool_message_frame(message, at)),
    );
    frames.extend(
        outcome
            .content
            .iter()
            .filter(|block| block.block_type == "text" && !block.text.is_empty())
            .map(|block| agent_text_frame(&block.text, at)),
    );
    if let Some(request) = &outcome.permission_request {
        frames.push(permission_frame(request, at));
    }
    if let Some(stop_reason) = acp_stop_reason(outcome.stop_reason) {
        frames.push(prompt_response(0, stop_reason));
    }
    frames
}

/// A tool call a turn's message reports, as a tool-call entry named by its tool. A message that says its
/// call failed records the entry failed.
fn tool_message_frame(message: &MessageDescriptor, at: i64) -> AcpAgentMessage {
    let title = message
        .tool
        .clone()
        .or_else(|| message.tool_calls.first().map(|call| call.name.clone()))
        .unwrap_or_default();
    let status = if message.is_error {
        ToolCallStatus::Failed
    } else {
        ToolCallStatus::Completed
    };
    session_update_frame(
        SessionUpdate {
            update: Some(session_update::Update::ToolCall(ToolCall {
                tool_call_id: Some(ToolCallId {
                    value: format!("message:{}", message.id.as_str()),
                }),
                title,
                kind: ToolKind::Other as i32,
                status: status as i32,
                ..Default::default()
            })),
        },
        at,
    )
}

/// A pending permission request, as a tool-call entry that names the tool it asks to run. The whole
/// request, options included, is its input, so the detail dialog shows what the operator is asked.
fn permission_frame(request: &PermissionRequestView, at: i64) -> AcpAgentMessage {
    session_update_frame(
        SessionUpdate {
            update: Some(session_update::Update::ToolCall(ToolCall {
                tool_call_id: Some(ToolCallId {
                    value: request.request_id.clone(),
                }),
                title: format!("Permission: {}", request.tool_call.title),
                kind: ToolKind::Other as i32,
                status: ToolCallStatus::Pending as i32,
                raw_input: serde_json::to_string(request).ok(),
                ..Default::default()
            })),
        },
        at,
    )
}

/// The ACP stop reason a turn's outcome ends with. A turn paused on a permission request has not ended,
/// so it has none — its pending request entry stands in for it. `ContextExhausted` is the model refusing
/// the turn; `YieldedToCaller` ends the turn where the caller asked it to stop.
fn acp_stop_reason(reason: StopReason) -> Option<AcpStopReason> {
    match reason {
        StopReason::EndTurn | StopReason::YieldedToCaller => Some(AcpStopReason::EndTurn),
        StopReason::Cancelled => Some(AcpStopReason::Cancelled),
        StopReason::MaxTokens => Some(AcpStopReason::MaxTokens),
        StopReason::MaxTurnRequests => Some(AcpStopReason::MaxTurnRequests),
        StopReason::ContextExhausted => Some(AcpStopReason::Refusal),
        StopReason::PermissionRequested => None,
    }
}
