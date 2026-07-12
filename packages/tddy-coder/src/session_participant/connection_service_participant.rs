//! Session-scoped `ConnectionService` handlers served from the coder's LiveKit participant.
//!
//! `DeleteSession` / `SignalSession` are **not** served here — the web routes them directly to the
//! daemon participant (`daemon-{instanceId}`), which owns process teardown and must be reachable
//! even when the coder participant is stuck (changeset `2026-07-12-fast-session-change`).

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A tool exposed by `ListExecTools`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
}

/// Result of `ExecuteTool`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteToolResult {
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
}

/// Result of `ClaimTerminalControl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimResult {
    pub granted: bool,
    pub control_token: String,
}

/// Outcome of executing a tool, returned by the [`ToolExecutor`] seam.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolOutcome {
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
    pub job_id: String,
    pub job_running: bool,
}

/// Seam for invoking a tool. The production wiring (in `run.rs`) injects an executor backed by the
/// shared tool engine; unit tests inject a fake.
pub trait ToolExecutor: Send + Sync {
    fn execute(&self, tool_name: &str, args_json: &str) -> ToolOutcome;
}

/// One recorded tool-call execution, persisted as a JSONL row. Field shape matches the daemon's
/// `tool_call_log::ToolCallRecord` so `ListSessionToolCalls` reads the same file identically.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRecord {
    pub task_id: String,
    pub tool_name: String,
    pub args_json: String,
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
    pub job_running: bool,
    pub created_unix_ms: u64,
}

/// Session-scoped `ConnectionService` handler. The `run.rs` adapter wraps these methods in the
/// generated `ConnectionService` trait and registers them on the participant's `ServiceEntry` list.
///
/// Methods served here: `ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`,
/// `ClaimTerminalControl`, `WatchTerminalControl`. Delete/signal are intentionally absent
/// (daemon-direct).
pub struct SessionConnectionService {
    pub session_id: String,
    pub session_token: String,
    pub tool_calls_path: PathBuf,
    pub tools: Vec<ToolDef>,
    pub executor: Arc<dyn ToolExecutor>,
}

impl SessionConnectionService {
    /// Returns the session's exec tool catalog.
    pub fn list_exec_tools(&self) -> Vec<ToolDef> {
        self.tools.clone()
    }

    /// Executes a tool and appends a `tool-calls.jsonl` entry (schema-compatible with the daemon).
    pub fn execute_tool(&self, tool_name: &str, args_json: &str) -> ExecuteToolResult {
        let outcome = self.executor.execute(tool_name, args_json);
        let record = ToolCallRecord {
            task_id: outcome.job_id.clone(),
            tool_name: tool_name.to_string(),
            args_json: args_json.to_string(),
            result_json: outcome.result_json.clone(),
            is_error: outcome.is_error,
            error_message: outcome.error_message.clone(),
            job_running: outcome.job_running,
            created_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };
        if let Err(e) = append_tool_call(&self.tool_calls_path, &record) {
            log::warn!(
                target: "tddy_coder::session_participant",
                "tool_call_log: failed to persist tool call for session {}: {}",
                self.session_id,
                e
            );
        }
        ExecuteToolResult {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
        }
    }

    /// Grants terminal control for the session's own terminal. The coder owns its terminal, so the
    /// lease is served directly (the daemon's control registry is irrelevant for tddy-coder).
    pub fn claim_terminal_control(&self, _screen_id: &str, _steal: bool) -> ClaimResult {
        ClaimResult {
            granted: true,
            control_token: format!("coder-{}", self.session_id),
        }
    }
}

/// Append one tool-call record to the session's `tool-calls.jsonl` (a full file path).
fn append_tool_call(path: &std::path::Path, record: &ToolCallRecord) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create {}: {}", parent.display(), e))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| anyhow::anyhow!("open {}: {}", path.display(), e))?;
    let line = serde_json::to_string(record)
        .map_err(|e| anyhow::anyhow!("serialize ToolCallRecord: {}", e))?;
    use std::io::Write;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| anyhow::anyhow!("write {}: {}", path.display(), e))
}

/// `ToolExecutor` used by `run.rs` when registering `ConnectionService` on the coder's own
/// participant. `ExecuteTool` for a tddy-coder session is not yet wired to the coder's workflow
/// action surface — this executor returns an honest error rather than a silent fallback.
///
/// FIXME(2026-07-12-fast-session-change): bridge `ExecuteTool(tool_name, args_json)` to the coder's
/// tool engine (build executor / workflow actions) so the web can run exec tools against the
/// session participant directly.
pub struct CoderSessionToolExecutor;

impl ToolExecutor for CoderSessionToolExecutor {
    fn execute(&self, tool_name: &str, _args_json: &str) -> ToolOutcome {
        ToolOutcome {
            result_json: String::new(),
            is_error: true,
            error_message: format!(
                "ExecuteTool '{tool_name}' is not yet wired for tddy-coder sessions (FIXME 2026-07-12-fast-session-change)"
            ),
            job_id: String::new(),
            job_running: false,
        }
    }
}

/// The exec tool catalog the coder exposes via `ListExecTools` on its session participant.
///
/// FIXME(2026-07-12-fast-session-change): populate from the coder's real tool surface (build tools,
/// workflow actions). Empty for now — the web lists no exec tools for a coder session until the
/// catalog is wired, which is honest (no silent fallback).
pub fn coder_session_tool_catalog() -> Vec<ToolDef> {
    Vec::new()
}

#[cfg(test)]
mod tests;
