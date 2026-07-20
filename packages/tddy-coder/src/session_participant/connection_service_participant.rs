//! Session-scoped `ConnectionService` handlers served from the coder's LiveKit participant.
//!
//! `DeleteSession` / `SignalSession` are **not** served here — the web routes them directly to the
//! daemon participant (`daemon-{instanceId}`), which owns process teardown and must be reachable
//! even when the coder participant is stuck (changeset `2026-07-12-fast-session-change`).
//!
//! `ExecuteTool` dispatches through the shared [`tddy_tool_engine`] against the session's
//! `worktree_root` (the coder's agent working directory), backed by a per-session
//! [`tddy_task::TaskRegistry`] for background shell jobs.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tddy_task::TaskRegistry;

/// A tool exposed by `ListExecTools`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
}

/// Result of `ExecuteTool` (plus the background-job fields the web response carries).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteToolResult {
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
    pub job_id: String,
    pub job_running: bool,
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

/// Seam for invoking a tool. Production wires [`CoderSessionToolExecutor`] (backed by the shared
/// `tddy-tool-engine`); unit tests inject a fake.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_name: &str, args_json: &str) -> ToolOutcome;
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
    /// Session worktree where started bash terminals are spawned (the coder's agent working dir).
    pub worktree: PathBuf,
    /// Manager for this session's started bash terminals (the terminal "tabs").
    pub terminal_manager: Arc<super::terminal_manager::TerminalManager>,
}

impl SessionConnectionService {
    /// Returns the session's exec tool catalog.
    pub fn list_exec_tools(&self) -> Vec<ToolDef> {
        self.tools.clone()
    }

    /// Executes a tool and appends a `tool-calls.jsonl` entry (schema-compatible with the daemon).
    pub async fn execute_tool(&self, tool_name: &str, args_json: &str) -> ExecuteToolResult {
        let outcome = self.executor.execute(tool_name, args_json).await;
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
            job_id: outcome.job_id,
            job_running: outcome.job_running,
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

/// `ToolExecutor` backed by the shared `tddy-tool-engine`, dispatching against the session's
/// `worktree_root` (the coder's agent working directory). Background shell jobs (`Shell` with
/// `block_until_ms=0`) land in this session's [`TaskRegistry`]; their live status is reachable via
/// the engine's `Await` tool, and completed calls are also persisted to `tool-calls.jsonl` by
/// [`SessionConnectionService::execute_tool`].
pub struct CoderSessionToolExecutor {
    pub worktree_root: PathBuf,
    pub task_registry: TaskRegistry,
    pub session_id: String,
}

#[async_trait]
impl ToolExecutor for CoderSessionToolExecutor {
    async fn execute(&self, tool_name: &str, args_json: &str) -> ToolOutcome {
        let outcome = tddy_tool_engine::execute_tool(
            &self.worktree_root,
            tool_name,
            args_json,
            &self.task_registry,
            &self.session_id,
        )
        .await;
        ToolOutcome {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }
}

/// The exec tool catalog the coder exposes via `ListExecTools` on its session participant — the
/// shared `tddy-tool-engine` catalog mapped to the coder's [`ToolDef`].
pub fn coder_session_tool_catalog() -> Vec<ToolDef> {
    tddy_tool_engine::tool_catalog()
        .into_iter()
        .map(|t| ToolDef {
            name: t.name,
            description: t.description,
            input_schema_json: t.input_schema_json,
        })
        .collect()
}

/// The FULL session toolset for `ListExecTools` (web Inspector → Tools). Shells out to
/// `tddy-tools list-tools` — the single source of truth in tddy-tools — so the panel shows the MCP
/// workflow tools (`spawn_conversation`, `pr_*`, …), the exec catalog, subagent tools, and the Bash
/// CLI subcommands, instead of only the static engine catalog.
///
/// On shell-out failure this logs an error and degrades to [`coder_session_tool_catalog`] (never a
/// silent empty list). FIXME(inspector-tools): a hard failure quietly narrows the panel back to the
/// 10 engine tools; revisit if the error should surface to the UI instead.
pub fn coder_session_tool_catalog_full() -> Vec<ToolDef> {
    match list_tools_via_tddy_tools() {
        Ok(tools) if !tools.is_empty() => tools,
        Ok(_) => {
            log::error!(
                "`tddy-tools list-tools` returned no tools; using the shared engine catalog"
            );
            coder_session_tool_catalog()
        }
        Err(e) => {
            log::error!("`tddy-tools list-tools` failed ({e}); using the shared engine catalog");
            coder_session_tool_catalog()
        }
    }
}

/// Resolve the `tddy-tools` binary like `verify_tddy_tools_available`: a sibling of the current
/// executable first, else rely on `PATH`.
fn tddy_tools_binary() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("tddy-tools");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    std::path::PathBuf::from("tddy-tools")
}

fn list_tools_via_tddy_tools() -> anyhow::Result<Vec<ToolDef>> {
    let output = std::process::Command::new(tddy_tools_binary())
        .arg("list-tools")
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "tddy-tools list-tools exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        name: String,
        description: String,
        input_schema_json: String,
    }
    let entries: Vec<Entry> = serde_json::from_slice(&output.stdout)?;
    Ok(entries
        .into_iter()
        .map(|e| ToolDef {
            name: e.name,
            description: e.description,
            input_schema_json: e.input_schema_json,
        })
        .collect())
}

#[cfg(test)]
mod tests;
