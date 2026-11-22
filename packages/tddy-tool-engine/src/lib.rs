//! Tool dispatch engine for workspace sessions.
//!
//! Implements `execute_tool` which routes tool calls to their respective implementations.
//! All file paths are validated against the worktree root to prevent path traversal.
//!
//! Shared by `tddy-daemon` (remote-codebase workspace sessions) and `tddy-coder` (session
//! participant `ExecuteTool` / `ListExecTools`).

pub mod base_freshness_tools;
pub mod catalog;
pub(crate) mod contained_shell;
pub mod delivery_route;
pub(crate) mod edited_region;
pub mod lsp_root_tools;
pub mod path_access_tools;
pub mod project_home_answer_tools;
pub(crate) mod project_home_diff;
pub mod project_home_disk_refresh;
pub mod project_home_git_tools;
pub mod project_home_inbox_tools;
pub mod project_home_lifecycle_tools;
pub mod project_home_notify_tools;
pub mod project_home_terminal_tools;
pub mod project_home_terminate;
pub mod project_home_tools;
pub mod project_home_turn_tools;
pub mod project_home_watch_stream_tools;
pub mod project_home_watch_tools;
pub mod project_home_worktree_tools;
pub mod reach;
pub mod read_only;
pub(crate) mod read_window;
pub(crate) mod restructure_tools;
pub(crate) mod search_window;
pub mod session_file_tools;
pub mod session_follow;
pub mod session_message_tools;
pub mod shell;
pub mod start_ledger;
pub mod start_options;
pub mod start_session_args;
pub mod terminal_keys;
pub mod work_item_annotation_tools;
pub mod work_item_git_tools;
pub mod work_item_link_tools;
pub mod work_queue_access;
pub mod work_queue_lineage_tools;
pub mod work_queue_tools;
pub mod work_queue_waves_tools;

pub use catalog::{tool_catalog, ToolDef};
pub use edited_region::EDITED_REGION_CONTEXT_LINES;
pub use shell::{
    execute_tool_on_shell, session_shell, LocalShell, RemoteShell, Shell, ShellError, ShellKind,
};

use std::path::{Component, Path, PathBuf};

use tddy_task::TaskRegistry;

/// Outcome of a tool execution.
pub struct ToolOutcome {
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
    /// The task ID for this invocation (equals `job_id` when `job_running`, otherwise the
    /// task_id of the completed task).
    pub job_id: String,
    /// True immediately after a background shell job is launched.
    pub job_running: bool,
}

impl ToolOutcome {
    pub(crate) fn ok(result_json: impl Into<String>) -> Self {
        Self {
            result_json: result_json.into(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }

    pub(crate) fn err(msg: impl Into<String>) -> Self {
        let m = msg.into();
        Self {
            result_json: serde_json::json!({ "error": m }).to_string(),
            is_error: true,
            error_message: m,
            job_id: String::new(),
            job_running: false,
        }
    }

    /// Like [`Self::err`], but the result JSON is already built — the caller pinned a shape the
    /// plain `{ "error": msg }` wrapper cannot carry (e.g. an occurrence count beside the
    /// error). The message still comes from the JSON's `"error"` key, so `error_message` says
    /// what the result says.
    pub(crate) fn err_json(result: serde_json::Value) -> Self {
        let msg = result
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        Self {
            result_json: result.to_string(),
            is_error: true,
            error_message: msg,
            job_id: String::new(),
            job_running: false,
        }
    }
}

/// Validate and resolve a path argument, ensuring it stays within `worktree_root`.
///
/// Returns an `Err` string if the path escapes the worktree root.
pub(crate) fn contain_path(worktree_root: &Path, arg_path: &str) -> Result<PathBuf, String> {
    let root = worktree_root
        .canonicalize()
        .map_err(|e| format!("cannot canonicalize worktree root: {e}"))?;

    let candidate = if Path::new(arg_path).is_absolute() {
        PathBuf::from(arg_path)
    } else {
        root.join(arg_path)
    };

    // For existing paths, canonicalize and check containment.
    if candidate.exists() {
        let canon = candidate
            .canonicalize()
            .map_err(|e| format!("cannot canonicalize path: {e}"))?;
        if !canon.starts_with(&root) {
            return Err(format!(
                "resolved path escapes worktree: {}",
                canon.display()
            ));
        }
        return Ok(canon);
    }

    // For new (non-existent) paths: reject any `..` components immediately before the ancestor
    // walk, so that `root/foo/../../../etc/passwd` is caught here rather than by the walk.
    for component in candidate.components() {
        if component == Component::ParentDir {
            return Err(format!(
                "path contains '..' component: {}",
                candidate.display()
            ));
        }
    }

    // Walk up to the nearest existing ancestor and verify it is inside the worktree.
    let mut check = candidate.clone();
    loop {
        if check.exists() {
            let canon = check
                .canonicalize()
                .map_err(|e| format!("cannot canonicalize ancestor: {e}"))?;
            if !canon.starts_with(&root) {
                return Err(format!("path escapes worktree: {}", candidate.display()));
            }
            break;
        }
        if !check.pop() {
            return Err(format!("path escapes worktree: {}", candidate.display()));
        }
    }

    Ok(candidate)
}

/// Dispatch a tool call within the given `worktree_root`.
///
/// Returns a `ToolOutcome` — never panics or returns a gRPC-level error.
pub async fn execute_tool(
    worktree_root: &Path,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
) -> ToolOutcome {
    execute_tool_with_env(
        worktree_root,
        tool_name,
        args_json,
        registry,
        session_id,
        &[],
    )
    .await
}

/// Like [`execute_tool`], but with extra environment variables applied to spawned shell commands
/// (e.g. a managed session's per-session `TDDY_SOCKET` + `PATH` so host-side `tddy-tools transition`
/// reaches the session's `WorkflowController`).
pub async fn execute_tool_with_env(
    worktree_root: &Path,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
    extra_env: &[(String, String)],
) -> ToolOutcome {
    let args: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };

    let kind = format!("execute_tool:{tool_name}");

    match tool_name {
        "Shell" => tool_shell(worktree_root, &args, registry, session_id, &kind, extra_env).await,
        "Await" => tool_await(&args, registry).await,
        "LspDiagnostics" | "LspDefinition" | "LspReferences" | "LspHover" | "LspSymbols" => {
            tool_lsp(worktree_root, tool_name, &args).await
        }
        "ReadLints" => tool_read_lints(worktree_root).await,
        name if tddy_core::toolcall::restructure::is_restructure_tool(name) => {
            restructure_tools::tool_restructure(worktree_root, tool_name, &args).await
        }
        _ => {
            // Sync tool — run inline, then register as a terminal task for observability.
            let mut outcome = match tool_name {
                "Read" => tool_read(worktree_root, &args),
                "Write" => tool_write(worktree_root, &args),
                "StrReplace" => tool_str_replace(worktree_root, &args),
                "Delete" => tool_delete(worktree_root, &args),
                "Grep" => tool_grep(worktree_root, &args).await,
                "Glob" => tool_glob(worktree_root, &args),
                "SemanticSearch" => tool_semantic_search(worktree_root, &args, extra_env).await,
                other => ToolOutcome::err(format!("unknown tool: {other}")),
            };
            let task_id = register_sync_task(registry, session_id, &kind, &outcome).await;
            outcome.job_id = task_id;
            outcome
        }
    }
}

fn tool_read(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Read: missing 'path' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("Read: {e}")),
    };

    read_resolved(&resolved, args)
}

/// `Read` of an already-contained `resolved` path, windowed by the call's `offset` and `limit`.
pub(crate) fn read_resolved(resolved: &Path, args: &serde_json::Value) -> ToolOutcome {
    let offset = args.get("offset").and_then(|v| v.as_u64());
    let limit = args.get("limit").and_then(|v| v.as_u64());

    match std::fs::read_to_string(resolved) {
        Ok(content) => {
            ToolOutcome::ok(read_window::line_window(&content, offset, limit).to_string())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ToolOutcome {
            result_json: serde_json::json!({ "error": "file not found" }).to_string(),
            is_error: true,
            error_message: "file not found".to_string(),
            job_id: String::new(),
            job_running: false,
        },
        Err(e) => ToolOutcome::err(format!("Read: {e}")),
    }
}

fn tool_write(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Write: missing 'path' argument"),
    };
    let contents = match args.get("contents").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolOutcome::err("Write: missing 'contents' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("Write: {e}")),
    };

    if let Some(parent) = resolved.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ToolOutcome::err(format!("Write: create_dir_all failed: {e}"));
        }
    }

    match std::fs::write(&resolved, contents) {
        Ok(()) => {
            let bytes = contents.len();
            ToolOutcome::ok(serde_json::json!({ "bytes_written": bytes }).to_string())
        }
        Err(e) => ToolOutcome::err(format!("Write: {e}")),
    }
}

fn tool_str_replace(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("StrReplace: missing 'path' argument"),
    };
    let old_string = match args.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolOutcome::err("StrReplace: missing 'old_string' argument"),
    };
    let new_string = match args.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolOutcome::err("StrReplace: missing 'new_string' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("StrReplace: {e}")),
    };

    let content = match std::fs::read_to_string(&resolved) {
        Ok(c) => c,
        Err(e) => return ToolOutcome::err(format!("StrReplace: read failed: {e}")),
    };

    // The offset is taken here rather than re-derived after the write, because it is the same
    // byte in both: everything before the match is untouched by the replacement.
    let Some(edit_offset) = content.find(old_string) else {
        // `err_json` rather than `err`: a no-match edit still knows its occurrence count, and a
        // caller deciding whether an edit landed reads that zero instead of an absence it
        // would have to guess at.
        return ToolOutcome::err_json(serde_json::json!({
            "error": "StrReplace: old_string not found in file",
            "matchedOccurrences": 0
        }));
    };
    let count = content.matches(old_string).count();
    if count > 1 {
        return ToolOutcome::err(format!(
            "StrReplace: old_string matches {count} times (must be unique)"
        ));
    }

    let new_content = content.replacen(old_string, new_string, 1);
    let region = edited_region::edited_region(&new_content, edit_offset);
    match std::fs::write(&resolved, &new_content) {
        Ok(()) => ToolOutcome::ok(
            serde_json::json!({
                "replaced": true,
                "matchedOccurrences": count,
                "bytes_written": new_content.len(),
                "edited_region": region.text,
                "edited_line": region.line,
            })
            .to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("StrReplace: write failed: {e}")),
    }
}

fn tool_delete(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Delete: missing 'path' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("Delete: {e}")),
    };

    match std::fs::remove_file(&resolved) {
        Ok(()) => ToolOutcome::ok(serde_json::json!({ "deleted": true }).to_string()),
        Err(e) => ToolOutcome::err(format!("Delete: {e}")),
    }
}

/// The dependency-free half of the MCP→daemon dynamic tool proxy, moved here from `tddy-tools`'
/// `server.rs` by `#unbundle` node 5.
///
/// # This is where a duplication ends
///
/// `tddy-tools`' `server::exec_tool_catalog()` was a hand-copied clone of this crate's
/// [`catalog::tool_catalog`] — same ten tools, same descriptions, same schema strings, in a struct
/// with the same three fields under a different name. It was kept in step by a guard test in
/// `tddy-tools` and a matching one in the daemon's `tool_catalog_sync.rs`; the codebase already
/// knew about the duplication and was paying to maintain it.
///
/// There is now one catalog. `tddy-tools` derives its `RemoteToolDef`s from
/// [`catalog::tool_catalog`] at the single point that needs the MCP shape, the same way it already
/// derives the `Lsp*` tools from `tddy_lsp_executor`. The `tddy-tools`-side guard test went with
/// the copy it guarded; the daemon's stayed, because it guards a different pair — this catalog
/// against `tddy_sandbox::workspace_exec_tool_names`, the allowlist a sandboxed `claude` is
/// spawned with — and that pair has not collapsed.
///
/// # What node 8 builds against
///
/// [`catalog::tool_catalog`] for the tool set, [`crate::execute_tool`] to run one, and
/// [`dynamic_proxy::is_native_tool_denied_in_remote_mode`] for the remote-mode refusal.
///
/// # What is deliberately not here
///
/// The MCP shape of these tools — `RemoteToolDef`, `build_dynamic_tool_list`,
/// `dynamic_tool_router` and `dispatch_dynamic_tool` — stays in `tddy-tools`, which is the crate
/// that speaks MCP. Putting it here would mean `rmcp` in a crate every workspace-session host
/// links, and `dispatch_dynamic_tool` additionally resolves the call against the session's live
/// agent roster, which is a `tddy-service` concern. Advertisement is not filtered by that roster:
/// a tool an agent has taken over is still advertised and refused at dispatch, because
/// `--allowedTools` is fixed when `claude` spawns and an agent attaching at minute forty can only
/// take a tool over by having the call refused where it is made
/// (docs/ft/daemon/session-agent-roster.md § Enforced at two layers).
pub mod dynamic_proxy {
    /// Whether `tool_name` is a native mutation tool that must be hard-denied when the agent is
    /// running against a remote codebase (`TDDY_REMOTE_SESSION_ID` set).
    ///
    /// A denial rather than an absence: in remote mode the local working directory is not the
    /// worktree the agent is editing, so a native write would corrupt it silently. The agent needs
    /// to be told it was refused, not that the tool was never there — the replacement is this
    /// crate's own `Write`, dispatched against the real worktree.
    pub fn is_native_tool_denied_in_remote_mode(tool_name: &str) -> bool {
        matches!(tool_name, "Write" | "Edit" | "NotebookEdit")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The three tools a remote-mode agent must not reach are the three that write to disk.
        #[test]
        fn denies_every_native_tool_that_writes_to_the_local_disk() {
            // Given / When / Then
            for tool in ["Write", "Edit", "NotebookEdit"] {
                assert!(
                    is_native_tool_denied_in_remote_mode(tool),
                    "{tool} writes to disk and must be denied in remote mode"
                );
            }
        }

        /// The denial is a list, not a default: refusing a read or the approval prompt would take
        /// the agent's ability to ask for anything.
        #[test]
        fn leaves_reads_and_the_approval_prompt_alone() {
            // Given / When / Then
            for tool in [
                "Read",
                "Grep",
                "approval_prompt",
                "submit",
                "AskUserQuestion",
            ] {
                assert!(
                    !is_native_tool_denied_in_remote_mode(tool),
                    "{tool} does not write to the local disk and must not be denied"
                );
            }
        }
    }
}

/// The `exec_tools.ExecToolService` entry — `#unbundle` node 8, family L.
///
/// This crate defines the ten tools, executes them, and — with this — serves them. One catalog, one
/// executor, one served coordinate.
///
/// Until node 5 there were three copies of that catalog: this crate's, a hand-written
/// `RemoteToolDef` clone in `tddy-tools`, and a guard test in the daemon, kept in step by matched
/// tests in both crates. Node 5 collapsed the clone. Node 8 deletes the guards, because with one
/// catalog they compare a value to itself — and a test whose failure is impossible reads as coverage
/// without being any.
pub mod exec_tool_entry;
pub mod exec_tool_service;
mod lsp_tools;
mod project_home_catalogue;
mod project_home_listing;
mod project_home_target_host;
mod project_home_worktree_read;
mod search_tools;
mod shell_tool;
pub mod tool_call_log;
use crate::lsp_tools::tool_lsp;
use crate::lsp_tools::tool_read_lints;
use crate::lsp_tools::tool_semantic_search;
use crate::search_tools::tool_glob;
use crate::search_tools::tool_grep;
use crate::shell_tool::register_sync_task;
use crate::shell_tool::tool_await;
use crate::shell_tool::tool_shell;
pub use exec_tool_entry::{build_exec_tool_entry, EXEC_TOOL_SERVICE, IN_JAIL_RELAYABLE_EXEC_TOOLS};
pub use exec_tool_service::{ExecToolHandler, ExecToolServiceImpl};

#[cfg(test)]
mod unbundle_exec_tool_entry_tests {
    #[test]
    fn names_the_service_family_l_moves_to() {
        struct EmptyExec;
        #[async_trait::async_trait]
        impl tddy_service::proto::exec_tools::ExecToolService for EmptyExec {
            type StreamExecuteToolStream = futures_util::stream::Empty<
                Result<tddy_service::proto::exec_tools::ExecuteToolChunk, tddy_rpc::Status>,
            >;
            async fn execute_tool(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ExecuteToolRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::exec_tools::ExecuteToolResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("stub"))
            }
            async fn stream_execute_tool(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ExecuteToolRequest>,
            ) -> Result<tddy_rpc::Response<Self::StreamExecuteToolStream>, tddy_rpc::Status>
            {
                Err(tddy_rpc::Status::unimplemented("stub"))
            }
            async fn list_exec_tools(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ListExecToolsRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::exec_tools::ListExecToolsResponse>,
                tddy_rpc::Status,
            > {
                Ok(tddy_rpc::Response::new(
                    tddy_service::proto::exec_tools::ListExecToolsResponse { tools: vec![] },
                ))
            }
            async fn list_session_tool_calls(
                &self,
                _request: tddy_rpc::Request<
                    tddy_service::proto::exec_tools::ListSessionToolCallsRequest,
                >,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::exec_tools::ListSessionToolCallsResponse>,
                tddy_rpc::Status,
            > {
                Ok(tddy_rpc::Response::new(
                    tddy_service::proto::exec_tools::ListSessionToolCallsResponse {
                        tool_calls: vec![],
                    },
                ))
            }
            async fn conversation_worktree(
                &self,
                _request: tddy_rpc::Request<
                    tddy_service::proto::exec_tools::ConversationWorktreeRequest,
                >,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::exec_tools::ConversationWorktreeResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("stub"))
            }
            async fn request_approval(
                &self,
                _request: tddy_rpc::Request<tddy_service::proto::exec_tools::ApprovalRequest>,
            ) -> Result<
                tddy_rpc::Response<tddy_service::proto::exec_tools::ApprovalResponse>,
                tddy_rpc::Status,
            > {
                Err(tddy_rpc::Status::unimplemented("stub"))
            }
        }
        assert_eq!(
            super::build_exec_tool_entry(EmptyExec).name,
            "exec_tools.ExecToolService"
        );
    }
}
