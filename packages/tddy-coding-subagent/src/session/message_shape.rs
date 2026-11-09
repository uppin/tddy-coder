use tddy_agent_backend::{AcpToolStatus, AcpUpdate};
use tddy_discovery::subagent::{preview_of, MessageRole, ToolCallDescriptor};

/// One tool call a turn made, folded from its `tool_call` and `tool_call_update`s.
pub(super) struct ToolCallRecord {
    id: String,
    name: String,
    arguments: String,
    failed: bool,
}

/// The turn's tool calls in the order the agent made them, each marked failed when any of its
/// updates said so.
pub(super) fn collect_tool_calls(updates: Vec<AcpUpdate>) -> Vec<ToolCallRecord> {
    let mut calls: Vec<ToolCallRecord> = Vec::new();
    for update in updates {
        match update {
            AcpUpdate::ToolCall {
                id,
                title,
                raw_input,
                ..
            } => calls.push(ToolCallRecord {
                id,
                name: title,
                arguments: raw_input.map(|input| input.to_string()).unwrap_or_default(),
                failed: false,
            }),
            AcpUpdate::ToolCallUpdate { id, status, .. } => {
                if let Some(call) = calls.iter_mut().find(|call| call.id == id) {
                    call.failed |= status == Some(AcpToolStatus::Failed);
                }
            }
            // A permission request's record is the session's account of the request, not a call the agent made.
            AcpUpdate::Text(_) | AcpUpdate::Thought(_) | AcpUpdate::Plan(_) => {}
            AcpUpdate::ToolCallRecord { .. } => {}
        }
    }
    calls
}

/// A message descriptor before it has an id; its preview not yet cut.
pub(super) struct MessageShape {
    pub(super) role: MessageRole,
    pub(super) tool: Option<String>,
    pub(super) tool_calls: Vec<ToolCallDescriptor>,
    pub(super) is_error: bool,
    pub(super) preview: String,
}

impl MessageShape {
    pub(super) fn tool_call(call: ToolCallRecord) -> Self {
        Self {
            role: MessageRole::Tool,
            tool: Some(call.name.clone()),
            tool_calls: vec![ToolCallDescriptor {
                name: call.name,
                arguments: preview_of(&call.arguments),
            }],
            is_error: call.failed,
            preview: call.arguments,
        }
    }

    pub(super) fn assistant(text: &str) -> Self {
        Self {
            role: MessageRole::Assistant,
            tool: None,
            tool_calls: Vec::new(),
            is_error: false,
            preview: text.to_string(),
        }
    }
}
