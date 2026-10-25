//! Codex's `exec --json` JSONL. Codex shows nothing of a turn before the turn ends, so the turn's
//! command executions become tool calls when it ends (or is cancelled), from the lines collected.

use serde_json::{json, Value};

use crate::backend::acp_driver::{AcpToolStatus, AcpUpdate};
use crate::stream::codex::{codex_jsonl_last_error_message, parse_codex_jsonl_output};

use super::transcript::{cancelled, ended, exit_error, Ending, Finished, Updates};
use super::HeadlessHarness;

pub(super) struct CodexTurn {
    updates: Updates,
    lines: Vec<String>,
}

impl CodexTurn {
    pub(super) fn new(updates: Updates) -> Self {
        Self {
            updates,
            lines: Vec::new(),
        }
    }

    pub(super) fn push_line(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    pub(super) fn finish(self, ending: Ending<'_>) -> Finished {
        self.emit_command_executions();
        let parsed = parse_codex_jsonl_output(&self.lines);
        let result = match ending {
            Ending::Cancelled => cancelled(parsed.result_text),
            Ending::Exited { status, .. } if status.success() => ended(parsed.result_text),
            Ending::Exited { status, stderr } => {
                let detail = codex_jsonl_last_error_message(&self.lines)
                    .unwrap_or_else(|| stderr.to_string());
                Err(exit_error(HeadlessHarness::Codex, status, &detail))
            }
        };
        Finished {
            session_id: parsed.session_id,
            result,
        }
    }

    /// One tool call per completed `command_execution` item, each followed by its completion.
    fn emit_command_executions(&self) {
        let mut ordinal = 0usize;
        for line in &self.lines {
            let Ok(event) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if event.get("type").and_then(Value::as_str) != Some("item.completed") {
                continue;
            }
            let Some(item) = event.get("item") else {
                continue;
            };
            if item.get("type").and_then(Value::as_str) != Some("command_execution") {
                continue;
            }
            ordinal += 1;
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .map_or_else(|| format!("codex-{ordinal}"), str::to_string);
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let failed = item
                .get("exit_code")
                .and_then(Value::as_i64)
                .is_some_and(|code| code != 0);
            self.updates.send(AcpUpdate::ToolCall {
                id: id.clone(),
                title: command.clone(),
                kind: "execute".to_string(),
                raw_input: Some(json!({ "command": command })),
            });
            self.updates.send(AcpUpdate::ToolCallUpdate {
                id,
                title: None,
                status: Some(if failed {
                    AcpToolStatus::Failed
                } else {
                    AcpToolStatus::Completed
                }),
                raw_output: None,
            });
        }
    }
}
