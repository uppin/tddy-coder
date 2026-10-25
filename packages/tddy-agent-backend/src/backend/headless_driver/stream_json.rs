//! Claude's and Cursor's `--output-format stream-json`. The closing `result` event's text is the
//! turn's reply; assistant text is kept only as the fallback when no result event arrives. Claude's
//! `tool_use` and `tool_result` blocks become ACP tool-call updates. Cursor's streamed tool events
//! (`tool_call`) carry no id to pair a result with, so a Cursor turn surfaces no tool calls yet.

use serde::Deserialize;

use crate::backend::acp_driver::{AcpToolStatus, AcpUpdate};
use crate::stream::claude::tool_use_detail;

use super::transcript::{cancelled, ended, exit_error, Ending, Finished, Updates};
use super::HeadlessHarness;

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    is_error: bool,
    /// Set on a sub-agent's traffic, which this turn does not surface.
    #[serde(default)]
    parent_tool_use_id: Option<String>,
    #[serde(default)]
    message: Option<Message>,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    content: Vec<Block>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Block {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[serde(default)]
        id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(default)]
        tool_use_id: String,
        #[serde(default)]
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

/// The closing `result` event.
#[derive(Debug)]
struct ResultEvent {
    text: Option<String>,
    is_error: bool,
}

pub(super) struct StreamJsonTurn {
    harness: HeadlessHarness,
    updates: Updates,
    session_id: Option<String>,
    assistant_text: String,
    result: Option<ResultEvent>,
}

impl StreamJsonTurn {
    pub(super) fn new(harness: HeadlessHarness, updates: Updates) -> Self {
        Self {
            harness,
            updates,
            session_id: None,
            assistant_text: String::new(),
            result: None,
        }
    }

    pub(super) fn push_line(&mut self, line: &str) {
        let event: Event = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(e) => {
                log::debug!(
                    "[headless] {} stream line is not an event: {e}",
                    self.harness.id()
                );
                return;
            }
        };
        if event.parent_tool_use_id.is_some() {
            return;
        }
        match event.kind.as_str() {
            "system" => self.note_session(event.session_id),
            "assistant" => {
                for block in event
                    .message
                    .into_iter()
                    .flat_map(|message| message.content)
                {
                    match block {
                        Block::Text { text } => self.assistant_text.push_str(&text),
                        Block::ToolUse { id, name, input } => {
                            self.updates.send(tool_call(id, name, input));
                        }
                        Block::ToolResult { .. } | Block::Other => {}
                    }
                }
            }
            "user" => {
                for block in event
                    .message
                    .into_iter()
                    .flat_map(|message| message.content)
                {
                    if let Block::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } = block
                    {
                        self.updates
                            .send(tool_result(tool_use_id, content, is_error));
                    }
                }
            }
            "result" => {
                self.note_session(event.session_id);
                self.result = Some(ResultEvent {
                    text: event.result,
                    is_error: event.is_error,
                });
            }
            _ => {}
        }
    }

    fn note_session(&mut self, session_id: Option<String>) {
        if let Some(session_id) = session_id.filter(|id| !id.is_empty()) {
            self.session_id = Some(session_id);
        }
    }

    /// The text a turn has produced so far: the result's when it has arrived, else the assistant's.
    fn text_so_far(&self) -> String {
        match &self.result {
            Some(ResultEvent {
                text: Some(text),
                is_error: false,
            }) => text.clone(),
            _ => self.assistant_text.clone(),
        }
    }

    pub(super) fn finish(self, ending: Ending<'_>) -> Finished {
        let session_id = self.session_id.clone();
        let result = match ending {
            Ending::Cancelled => cancelled(self.text_so_far()),
            Ending::Exited { status, stderr } => match &self.result {
                Some(ResultEvent {
                    text: Some(text),
                    is_error: false,
                }) => ended(text.clone()),
                Some(ResultEvent {
                    text,
                    is_error: true,
                }) => Err(exit_error(
                    self.harness,
                    status,
                    text.as_deref().unwrap_or(stderr),
                )),
                _ if status.success() => ended(self.assistant_text.clone()),
                _ => Err(exit_error(self.harness, status, stderr)),
            },
        };
        Finished { session_id, result }
    }
}

/// The ACP kind of a Claude tool.
fn acp_kind(tool: &str) -> &'static str {
    match tool {
        "Read" => "read",
        "Write" | "Edit" | "MultiEdit" => "edit",
        "Bash" => "execute",
        "Glob" | "Grep" => "search",
        _ => "other",
    }
}

fn tool_call(id: String, name: String, input: serde_json::Value) -> AcpUpdate {
    let title = match tool_use_detail(&name, &input) {
        Some(detail) => format!("{name} {detail}"),
        None => name.clone(),
    };
    AcpUpdate::ToolCall {
        id,
        title,
        kind: acp_kind(&name).to_string(),
        raw_input: (!input.is_null()).then_some(input),
    }
}

fn tool_result(tool_use_id: String, content: serde_json::Value, is_error: bool) -> AcpUpdate {
    AcpUpdate::ToolCallUpdate {
        id: tool_use_id,
        title: None,
        status: Some(if is_error {
            AcpToolStatus::Failed
        } else {
            AcpToolStatus::Completed
        }),
        raw_output: (!content.is_null()).then_some(content),
    }
}
