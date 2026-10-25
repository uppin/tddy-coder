//! `oz agent run --output-format ndjson`: the agent's text, and each tool call paired with its
//! result. A `tool_call` carries no id, so the turn synthesises one per call and pairs a
//! `tool_result` with the most recent unpaired `tool_call` of the same tool.

use serde::Deserialize;
use serde_json::json;

use crate::backend::acp_driver::{AcpToolStatus, AcpUpdate};

use super::transcript::{cancelled, ended, exit_error, Ending, Finished, Updates};
use super::HeadlessHarness;

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    event_type: String,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    file_paths: Vec<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    exit_code: Option<i64>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    diff: Option<String>,
}

/// A tool call whose result has not arrived yet.
struct PendingCall {
    id: String,
    tool: String,
}

pub(super) struct OzTurn {
    updates: Updates,
    session_id: Option<String>,
    text: String,
    calls: usize,
    pending: Vec<PendingCall>,
}

impl OzTurn {
    pub(super) fn new(updates: Updates) -> Self {
        Self {
            updates,
            session_id: None,
            text: String::new(),
            calls: 0,
            pending: Vec::new(),
        }
    }

    pub(super) fn push_line(&mut self, line: &str) {
        let event: Event = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(e) => {
                log::debug!("[headless] oz stream line is not an event: {e}");
                return;
            }
        };
        match event.kind.as_str() {
            "system" if event.event_type == "conversation_started" => {
                if let Some(id) = event.conversation_id.as_deref().filter(|id| !id.is_empty()) {
                    self.session_id = Some(id.to_string());
                }
            }
            "agent" => self
                .text
                .push_str(event.text.as_deref().unwrap_or_default()),
            "tool_call" => self.tool_call(&event),
            "tool_result" => self.tool_result(&event),
            _ => {}
        }
    }

    fn tool_call(&mut self, event: &Event) {
        self.calls += 1;
        let id = format!("oz-{}", self.calls);
        let (title, kind, raw_input) = match event.tool.as_str() {
            "run_command" => {
                let command = event.command.clone().unwrap_or_else(|| event.tool.clone());
                (command.clone(), "execute", json!({ "command": command }))
            }
            "edit_files" => {
                let title = event
                    .title
                    .clone()
                    .or_else(|| event.file_paths.first().cloned())
                    .unwrap_or_else(|| event.tool.clone());
                (title, "edit", json!({ "file_paths": event.file_paths }))
            }
            other => (other.to_string(), "other", json!({ "tool": other })),
        };
        self.pending.push(PendingCall {
            id: id.clone(),
            tool: event.tool.clone(),
        });
        self.updates.send(AcpUpdate::ToolCall {
            id,
            title,
            kind: kind.to_string(),
            raw_input: Some(raw_input),
        });
    }

    fn tool_result(&mut self, event: &Event) {
        let Some(position) = self
            .pending
            .iter()
            .rposition(|call| call.tool == event.tool)
        else {
            log::debug!(
                "[headless] oz tool_result for {} has no pending tool_call",
                event.tool
            );
            return;
        };
        let call = self.pending.remove(position);
        let failed = event
            .status
            .as_deref()
            .is_some_and(|status| !matches!(status, "complete" | "completed"))
            || event.exit_code.is_some_and(|code| code != 0);
        self.updates.send(AcpUpdate::ToolCallUpdate {
            id: call.id,
            title: None,
            status: Some(if failed {
                AcpToolStatus::Failed
            } else {
                AcpToolStatus::Completed
            }),
            raw_output: event
                .output
                .clone()
                .or_else(|| event.diff.clone())
                .map(serde_json::Value::String),
        });
    }

    pub(super) fn finish(self, ending: Ending<'_>) -> Finished {
        let session_id = self.session_id.clone();
        let result = match ending {
            Ending::Cancelled => cancelled(self.text),
            Ending::Exited { status, .. } if status.success() => ended(self.text),
            Ending::Exited { status, stderr } => {
                Err(exit_error(HeadlessHarness::Oz, status, stderr))
            }
        };
        Finished { session_id, result }
    }
}
