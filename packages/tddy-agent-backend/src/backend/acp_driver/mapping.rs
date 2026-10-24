//! The ACP wire shapes the driver speaks, mapped to the crate's own: session updates to [`AcpUpdate`]
//! and [`ProgressEvent`]s, permission requests and answers, MCP servers, stop reasons and the models
//! a session offers. Every ACP-driven backend reads an agent's updates through this one mapping.

use agent_client_protocol as acp;

use super::{
    AcpStopReason, AcpToolStatus, AcpUpdate, McpServerSpec, PermissionDecision,
    PermissionOptionView, PermissionRequest,
};
use crate::backend::ProgressSink;
use crate::error::BackendError;
use crate::stream::ProgressEvent;

/// What one session update says, when the driver reports it. Updates the driver does not report —
/// available commands, mode, configuration and usage changes — map to `None`.
pub(super) fn acp_update(update: acp::SessionUpdate) -> Option<AcpUpdate> {
    match update {
        acp::SessionUpdate::AgentMessageChunk(chunk) => {
            non_empty_text(chunk.content).map(AcpUpdate::Text)
        }
        acp::SessionUpdate::AgentThoughtChunk(chunk) => {
            non_empty_text(chunk.content).map(AcpUpdate::Thought)
        }
        acp::SessionUpdate::ToolCall(call) => Some(AcpUpdate::ToolCall {
            id: call.tool_call_id.0.to_string(),
            title: call.title,
            kind: tool_kind_name(call.kind).to_string(),
            raw_input: call.raw_input,
        }),
        acp::SessionUpdate::ToolCallUpdate(update) => Some(AcpUpdate::ToolCallUpdate {
            id: update.tool_call_id.0.to_string(),
            title: update.fields.title,
            status: update.fields.status.and_then(tool_status),
            raw_output: update.fields.raw_output,
        }),
        acp::SessionUpdate::Plan(plan) => Some(AcpUpdate::Plan(
            plan.entries
                .into_iter()
                .map(|entry| entry.content)
                .collect(),
        )),
        _ => None,
    }
}

/// Report one update to a progress sink, as the TUI shows it: message text as progress, a tool call
/// as tool use, a tool's terminal state as its result, and plan entries as started tasks. Reasoning
/// is not reported as progress.
pub(super) fn report_progress(update: &AcpUpdate, sink: &ProgressSink) {
    match update {
        AcpUpdate::Text(text) => sink.emit(&ProgressEvent::TaskProgress {
            description: text.clone(),
            last_tool: None,
        }),
        AcpUpdate::Thought(_) => {}
        AcpUpdate::ToolCall {
            id,
            title,
            raw_input,
            ..
        } => sink.emit(&ProgressEvent::ToolUse {
            name: title.clone(),
            detail: None,
            input_json: raw_input.as_ref().and_then(json_text),
            call_id: Some(id.clone()),
        }),
        AcpUpdate::ToolCallUpdate {
            id,
            status,
            raw_output,
            ..
        } => {
            if let Some(is_error) = tool_outcome(*status) {
                sink.emit(&tool_result(id, raw_output.as_ref(), is_error));
            }
        }
        AcpUpdate::Plan(entries) => report_plan(entries, sink),
    }
}

/// Each plan entry is a task the agent has started.
fn report_plan(entries: &[String], sink: &ProgressSink) {
    for entry in entries {
        sink.emit(&ProgressEvent::TaskStarted {
            description: entry.clone(),
        });
    }
}

/// The request a permission the agent asked for becomes for the handler.
pub(super) fn permission_request(request: &acp::RequestPermissionRequest) -> PermissionRequest {
    PermissionRequest {
        session_id: request.session_id.0.to_string(),
        tool_call_id: request.tool_call.tool_call_id.0.to_string(),
        title: request.tool_call.fields.title.clone(),
        raw_input: request.tool_call.fields.raw_input.clone(),
        options: request
            .options
            .iter()
            .map(|option| PermissionOptionView {
                option_id: option.option_id.0.to_string(),
                name: option.name.clone(),
                kind: permission_kind_name(option.kind).to_string(),
            })
            .collect(),
    }
}

/// The answer the agent receives for a handler's decision, verbatim.
pub(super) fn permission_outcome(decision: PermissionDecision) -> acp::RequestPermissionOutcome {
    match decision {
        PermissionDecision::Selected { option_id } => acp::RequestPermissionOutcome::Selected(
            acp::SelectedPermissionOutcome::new(acp::PermissionOptionId::new(option_id)),
        ),
        PermissionDecision::Cancelled => acp::RequestPermissionOutcome::Cancelled,
    }
}

/// A stdio MCP server, as `session/new` and `session/load` take it.
pub(super) fn mcp_server(spec: McpServerSpec) -> acp::McpServer {
    acp::McpServer::Stdio(
        acp::McpServerStdio::new(spec.name, spec.command)
            .args(spec.args)
            .env(
                spec.env
                    .into_iter()
                    .map(|(name, value)| acp::EnvVariable::new(name, value))
                    .collect(),
            ),
    )
}

/// The crate's stop reason for ACP's. A reason this driver does not know is an error, not a guess.
pub(super) fn stop_reason(reason: acp::StopReason) -> Result<AcpStopReason, BackendError> {
    match reason {
        acp::StopReason::EndTurn => Ok(AcpStopReason::EndTurn),
        acp::StopReason::MaxTokens => Ok(AcpStopReason::MaxTokens),
        acp::StopReason::MaxTurnRequests => Ok(AcpStopReason::MaxTurnRequests),
        acp::StopReason::Refusal => Ok(AcpStopReason::Refusal),
        acp::StopReason::Cancelled => Ok(AcpStopReason::Cancelled),
        other => Err(BackendError::InvocationFailed(format!(
            "the ACP agent ended the turn with a stop reason this driver does not know: {other:?}"
        ))),
    }
}

/// The model ids a session offers, and the one it is on. A session that advertises no model state
/// offers none.
pub(super) fn offered_models(
    state: Option<&acp::SessionModelState>,
) -> (Vec<String>, Option<String>) {
    match state {
        Some(state) => (
            state
                .available_models
                .iter()
                .map(|model| model.model_id.0.to_string())
                .collect(),
            Some(state.current_model_id.0.to_string()),
        ),
        None => (Vec::new(), None),
    }
}

/// A tool call's terminal state, as the progress event the TUI shows for its result.
fn tool_result(
    call_id: &str,
    raw_output: Option<&serde_json::Value>,
    is_error: bool,
) -> ProgressEvent {
    ProgressEvent::ToolResult {
        call_id: call_id.to_string(),
        result_json: raw_output.and_then(json_text).unwrap_or_default(),
        is_error,
    }
}

/// Whether a tool call's status is its result, and if so whether the result is an error. ACP reports
/// a tool's result as a status change to Completed or Failed on the same id; any other status is not
/// a result yet.
fn tool_outcome(status: Option<AcpToolStatus>) -> Option<bool> {
    match status {
        Some(AcpToolStatus::Failed) => Some(true),
        Some(AcpToolStatus::Completed) => Some(false),
        _ => None,
    }
}

/// The driver's status for a tool call. A status this driver does not know is reported as absent,
/// which is how an update without a status reads.
fn tool_status(status: acp::ToolCallStatus) -> Option<AcpToolStatus> {
    match status {
        acp::ToolCallStatus::Pending => Some(AcpToolStatus::Pending),
        acp::ToolCallStatus::InProgress => Some(AcpToolStatus::InProgress),
        acp::ToolCallStatus::Completed => Some(AcpToolStatus::Completed),
        acp::ToolCallStatus::Failed => Some(AcpToolStatus::Failed),
        _ => None,
    }
}

/// The protocol's spelling of a tool kind. A kind this driver does not know reads as `other`, as
/// the protocol itself reads an unknown kind.
fn tool_kind_name(kind: acp::ToolKind) -> &'static str {
    match kind {
        acp::ToolKind::Read => "read",
        acp::ToolKind::Edit => "edit",
        acp::ToolKind::Delete => "delete",
        acp::ToolKind::Move => "move",
        acp::ToolKind::Search => "search",
        acp::ToolKind::Execute => "execute",
        acp::ToolKind::Think => "think",
        acp::ToolKind::Fetch => "fetch",
        acp::ToolKind::SwitchMode => "switch_mode",
        _ => "other",
    }
}

/// The protocol's spelling of an option kind. A kind newer than this driver is `unknown`; no
/// handler in this crate selects an option of an unknown kind.
fn permission_kind_name(kind: acp::PermissionOptionKind) -> &'static str {
    match kind {
        acp::PermissionOptionKind::AllowOnce => "allow_once",
        acp::PermissionOptionKind::AllowAlways => "allow_always",
        acp::PermissionOptionKind::RejectOnce => "reject_once",
        acp::PermissionOptionKind::RejectAlways => "reject_always",
        _ => "unknown",
    }
}

/// The text a content block contributes to a turn's output; a placeholder for non-text media.
fn content_text(content: acp::ContentBlock) -> String {
    match content {
        acp::ContentBlock::Text(t) => t.text,
        acp::ContentBlock::Image(_) => "<image>".to_string(),
        acp::ContentBlock::Audio(_) => "<audio>".to_string(),
        acp::ContentBlock::ResourceLink(r) => r.uri,
        acp::ContentBlock::Resource(_) => "<resource>".to_string(),
        _ => String::new(),
    }
}

fn non_empty_text(content: acp::ContentBlock) -> Option<String> {
    let text = content_text(content);
    (!text.is_empty()).then_some(text)
}

fn json_text(value: &serde_json::Value) -> Option<String> {
    serde_json::to_string(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A sink that keeps every event it is given, and the events it kept.
    fn recording_sink() -> (ProgressSink, Arc<Mutex<Vec<ProgressEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let kept = events.clone();
        let sink = ProgressSink::new(move |event| kept.lock().unwrap().push(event.clone()));
        (sink, events)
    }

    fn message(text: &str) -> acp::SessionUpdate {
        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(text.to_string().into()))
    }

    fn tool_update(status: acp::ToolCallStatus) -> AcpUpdate {
        AcpUpdate::ToolCallUpdate {
            id: "call-1".to_string(),
            title: None,
            status: tool_status(status),
            raw_output: Some(serde_json::json!({"ok": true})),
        }
    }

    #[test]
    fn a_message_chunk_is_text_and_an_empty_one_is_nothing() {
        // When / Then
        assert_eq!(
            acp_update(message("Hello, ")),
            Some(AcpUpdate::Text("Hello, ".to_string()))
        );
        assert_eq!(acp_update(message("")), None);
    }

    #[test]
    fn a_text_update_reports_progress() {
        // Given
        let (sink, events) = recording_sink();

        // When
        report_progress(&AcpUpdate::Text("world".to_string()), &sink);

        // Then
        let events = events.lock().unwrap();
        assert!(matches!(
            &events[0],
            ProgressEvent::TaskProgress { description, .. } if description == "world"
        ));
    }

    #[test]
    fn a_tool_result_is_reported_only_on_a_terminal_status() {
        // Given
        let (sink, events) = recording_sink();

        // When
        report_progress(&tool_update(acp::ToolCallStatus::InProgress), &sink);
        report_progress(&tool_update(acp::ToolCallStatus::Failed), &sink);

        // Then
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(matches!(
            &events[0],
            ProgressEvent::ToolResult { call_id, result_json, is_error: true }
                if call_id == "call-1" && result_json == r#"{"ok":true}"#
        ));
    }

    #[test]
    fn a_tool_call_keeps_its_id_kind_and_input() {
        // Given
        let call =
            acp::ToolCall::new("tool-Read", "Read").raw_input(serde_json::json!({"path": "a.rs"}));

        // When
        let update = acp_update(acp::SessionUpdate::ToolCall(call));

        // Then
        assert_eq!(
            update,
            Some(AcpUpdate::ToolCall {
                id: "tool-Read".to_string(),
                title: "Read".to_string(),
                kind: "other".to_string(),
                raw_input: Some(serde_json::json!({"path": "a.rs"})),
            })
        );
    }

    #[test]
    fn a_permission_option_kind_has_the_protocols_spelling() {
        // When / Then
        assert_eq!(
            permission_kind_name(acp::PermissionOptionKind::AllowOnce),
            "allow_once"
        );
        assert_eq!(
            permission_kind_name(acp::PermissionOptionKind::RejectAlways),
            "reject_always"
        );
    }

    #[test]
    fn a_protocol_stop_reason_keeps_its_meaning() {
        // When / Then
        assert_eq!(
            stop_reason(acp::StopReason::Cancelled).unwrap(),
            AcpStopReason::Cancelled
        );
        assert_eq!(
            stop_reason(acp::StopReason::EndTurn).unwrap(),
            AcpStopReason::EndTurn
        );
    }
}
