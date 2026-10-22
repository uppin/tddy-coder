//! What every ACP-driven backend does with an agent's session notifications and permission requests.
//!
//! [`super::ClaudeAcpBackend`], [`super::CodexAcpBackend`] and [`super::OpenCodeAcpBackend`] each run
//! their agent through their own worker, but read its `session/update` stream the same way: message
//! chunks accumulate into the turn's output, and tool calls, tool results and plan entries become
//! [`ProgressEvent`]s on the turn's progress sink.

use super::ProgressSink;
use crate::stream::ProgressEvent;
use agent_client_protocol as acp;

/// The option a headless run selects for every permission request: approve this one call.
const ALLOW_ONCE: &str = "allow-once";

/// Fold one session update into the turn: message text is appended to `output` and reported to
/// `sink`; tool calls, terminal tool states and plan entries are reported to `sink` only.
pub(super) fn record_session_update(
    output: &mut String,
    sink: Option<&ProgressSink>,
    update: acp::SessionUpdate,
) {
    match update {
        acp::SessionUpdate::AgentMessageChunk(chunk) => {
            let text = content_text(chunk.content);
            if text.is_empty() {
                return;
            }
            output.push_str(&text);
            if let Some(sink) = sink {
                sink.emit(&ProgressEvent::TaskProgress {
                    description: text,
                    last_tool: None,
                });
            }
        }
        acp::SessionUpdate::ToolCall(tc) => {
            if let Some(sink) = sink {
                sink.emit(&ProgressEvent::ToolUse {
                    name: tc.title,
                    detail: None,
                    input_json: tc.raw_input.as_ref().and_then(json_text),
                    call_id: Some(tc.tool_call_id.0.to_string()),
                });
            }
        }
        acp::SessionUpdate::ToolCallUpdate(upd) => {
            if let (Some(sink), Some(event)) = (sink, tool_result(upd)) {
                sink.emit(&event);
            }
        }
        acp::SessionUpdate::Plan(plan) => {
            if let Some(sink) = sink {
                for entry in plan.entries {
                    sink.emit(&ProgressEvent::TaskStarted {
                        description: entry.content,
                    });
                }
            }
        }
        _ => {}
    }
}

/// The response that approves one tool call, as a headless run answers every permission request.
pub(super) fn allow_once() -> acp::RequestPermissionResponse {
    acp::RequestPermissionResponse::new(acp::RequestPermissionOutcome::Selected(
        acp::SelectedPermissionOutcome::new(acp::PermissionOptionId::new(ALLOW_ONCE)),
    ))
}

/// The text a content block contributes to the turn's output; a placeholder for non-text media.
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

/// ACP reports a tool's result as a status change to Completed or Failed on the same
/// `tool_call_id`, so only those states become a [`ProgressEvent::ToolResult`].
fn tool_result(upd: acp::ToolCallUpdate) -> Option<ProgressEvent> {
    let is_error = match upd.fields.status? {
        acp::ToolCallStatus::Failed => true,
        acp::ToolCallStatus::Completed => false,
        _ => return None,
    };
    Some(ProgressEvent::ToolResult {
        call_id: upd.tool_call_id.0.to_string(),
        result_json: upd
            .fields
            .raw_output
            .as_ref()
            .and_then(json_text)
            .unwrap_or_default(),
        is_error,
    })
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

    fn tool_update(status: acp::ToolCallStatus) -> acp::SessionUpdate {
        acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
            acp::ToolCallId::new("call-1"),
            acp::ToolCallUpdateFields::new()
                .status(status)
                .raw_output(serde_json::json!({"ok": true})),
        ))
    }

    #[test]
    fn message_chunks_accumulate_into_the_output_and_report_progress() {
        // Given
        let (sink, events) = recording_sink();
        let mut output = String::new();

        // When
        record_session_update(&mut output, Some(&sink), message("Hello, "));
        record_session_update(&mut output, Some(&sink), message("world"));

        // Then
        assert_eq!(output, "Hello, world");
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1],
            ProgressEvent::TaskProgress { description, .. } if description == "world"
        ));
    }

    #[test]
    fn an_empty_chunk_adds_nothing_and_reports_nothing() {
        // Given
        let (sink, events) = recording_sink();
        let mut output = String::new();

        // When
        record_session_update(&mut output, Some(&sink), message(""));

        // Then
        assert!(output.is_empty());
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn a_tool_result_is_reported_only_on_a_terminal_status() {
        // Given
        let (sink, events) = recording_sink();
        let mut output = String::new();

        // When
        record_session_update(
            &mut output,
            Some(&sink),
            tool_update(acp::ToolCallStatus::InProgress),
        );
        record_session_update(
            &mut output,
            Some(&sink),
            tool_update(acp::ToolCallStatus::Failed),
        );

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
    fn without_a_sink_the_output_still_accumulates() {
        // Given
        let mut output = String::new();

        // When
        record_session_update(&mut output, None, message("quiet"));

        // Then
        assert_eq!(output, "quiet");
    }

    #[test]
    fn every_permission_request_is_approved_once() {
        // When
        let response = allow_once();

        // Then
        assert!(matches!(
            response.outcome,
            acp::RequestPermissionOutcome::Selected(selected)
                if selected.option_id.0.as_ref() == ALLOW_ONCE
        ));
    }
}
