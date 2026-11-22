//! An orchestrator's LSP roots, served on this host (`#orchestra` 8/9).
//!
//! `Lsp*` calls of an orchestrator-role session run on [`lsp_root_for`]'s root: its home worktree,
//! a worktree in its reach, or a root it added. `AddLspRoot` records a root in the session's
//! [`SessionLspRoots`] and warms its index; a root outside the reach is first requested from the
//! operator through the same flow as `RequestPathAccess` (read mode, the root as its path).
//! `RemoveLspRoot` drops a root the session added, with no request: the index daemon's idle reaping
//! unloads it once no session uses it.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::is_orchestrator_role;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::lsp_roots::{AddedLspRoot, SessionLspRoots};
use tddy_tool_engine::lsp_root_tools::{
    execute_lsp_root_tool, is_orchestrator_lsp_tool, lsp_root_for, AddLspRootArgs, AddedRoot,
    LspRootPort, ADD_LSP_ROOT,
};
use tddy_tool_engine::path_access_tools::{
    PathAccessAnswer, PathAccessPort, RequestPathAccessArgs, RequestedAccess,
};
use tddy_worktree_service::session_target::HomeCaller;

use super::path_access::DaemonPathAccess;
use super::project_home::caller::orchestrator_caller;
use super::project_home::caller::record_call;
use super::ExecToolRpcHandler;

/// [`LspRootPort`] over this host for one orchestrator call.
struct DaemonLspRoots {
    session_dir: PathBuf,
    /// The caller's request port; its read reach decides whether a root needs a request.
    path_access: DaemonPathAccess,
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Ask the registered language-server executor about `root` in the background, so the index loads
/// it before the first real query. Best effort: a host with no executor, or one that does not serve
/// the root, loads it on the first query as before.
// TODO(docs/dev/todo/2026-11-21-add-lsp-root-warms-by-symbol-query.md): the executor has no warm
// call and this handler holds no index channel for `code_index.Warm`; an empty symbol search is the
// cheapest query that makes the index load the root.
fn warm(root: PathBuf) {
    let Some(executor) = tddy_core::toolcall::lsp::lsp_executor() else {
        return;
    };
    drop(tokio::task::spawn_blocking(move || {
        if !executor.is_available(&root) {
            return;
        }
        let query = tddy_core::toolcall::lsp::LspQuery {
            target: String::new(),
            file: String::new(),
            line: 0,
            character: 0,
            symbol_query: Some(String::new()),
        };
        if let Err(e) = executor.symbols(&root, &query) {
            log::debug!("warming the LSP root {} failed: {e}", root.display());
        }
    }));
}

#[async_trait]
impl LspRootPort for DaemonLspRoots {
    async fn add_lsp_root(
        &self,
        caller: &HomeCaller,
        args: AddLspRootArgs,
    ) -> Result<AddedRoot, String> {
        let requested = PathBuf::from(&args.root);
        if !requested.is_dir() {
            return Err(format!("{} is not a directory", args.root));
        }
        let root = canonical(&requested);
        let store = SessionLspRoots::open(&self.session_dir);
        let added = store.roots().map_err(|e| e.to_string())?;
        let already_added = added.iter().any(|a| a.root == root);
        let root_path = root.display().to_string();
        let outside_reach = !already_added
            && !self
                .path_access
                .paths_outside_reach(
                    caller,
                    std::slice::from_ref(&root_path),
                    RequestedAccess::Read,
                )
                .await?
                .is_empty();
        if outside_reach {
            let answer = self
                .path_access
                .request_path_access(
                    caller,
                    RequestPathAccessArgs {
                        paths: vec![root_path],
                        mode: RequestedAccess::Read,
                        reason: format!("{ADD_LSP_ROOT}: {}", args.reason),
                    },
                )
                .await?;
            if answer != PathAccessAnswer::Allowed {
                return Ok(AddedRoot::Refused);
            }
        }
        if !already_added {
            store
                .add(AddedLspRoot {
                    root: root.clone(),
                    reason: args.reason,
                    added_at_unix_ms: now_unix_ms(),
                })
                .map_err(|e| e.to_string())?;
        }
        warm(root);
        Ok(AddedRoot::Added)
    }

    async fn remove_lsp_root(&self, _caller: &HomeCaller, root: PathBuf) -> Result<(), String> {
        let store = SessionLspRoots::open(&self.session_dir);
        let removed = store.remove(&root).map_err(|e| e.to_string())?
            || store.remove(&canonical(&root)).map_err(|e| e.to_string())?;
        if removed {
            Ok(())
        } else {
            Err(format!(
                "{} is not a root you added: only roots added with {ADD_LSP_ROOT} can be removed",
                root.display()
            ))
        }
    }

    async fn list_lsp_roots(&self, _caller: &HomeCaller) -> Result<Vec<PathBuf>, String> {
        Ok(SessionLspRoots::open(&self.session_dir)
            .roots()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|added| added.root)
            .collect())
    }
}

fn response_of(outcome: tddy_tool_engine::ToolOutcome) -> ExecuteToolResponse {
    ExecuteToolResponse {
        result_json: outcome.result_json,
        is_error: outcome.is_error,
        error_message: outcome.error_message,
        job_id: outcome.job_id,
        job_running: outcome.job_running,
    }
}

impl ExecToolRpcHandler {
    /// Serve `req`'s `AddLspRoot`, `RemoveLspRoot` or `ListLspRoots` for an orchestrator caller and
    /// record the call.
    pub(super) async fn run_lsp_root_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let caller = orchestrator_caller(sessions_base, &req.session_id, &req.tool_name)?;
        let home = unified_session_dir_path(sessions_base, &req.session_id);
        let port = DaemonLspRoots {
            session_dir: home,
            path_access: self
                .path_access_port(req, sessions_base, worktree_root)
                .await?,
        };
        let outcome = execute_lsp_root_tool(&port, &caller, &req.tool_name, &req.args_json).await;
        let response = response_of(outcome);
        record_call(sessions_base, req, &response);
        Ok(response)
    }

    /// Run an orchestrator's `Lsp*` call on the root it names ([`lsp_root_for`]); `None` when the
    /// call is not that (another tool, another role, a subagent conversation).
    pub(super) async fn run_orchestrator_lsp(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> Result<Option<ExecuteToolResponse>, Status> {
        if !is_orchestrator_lsp_tool(&req.tool_name) || !req.conversation_id.is_empty() {
            return Ok(None);
        }
        let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
        let is_orchestrator = tddy_core::read_session_metadata(&session_dir)
            .ok()
            .and_then(|metadata| metadata.session_role)
            .as_deref()
            .is_some_and(is_orchestrator_role);
        if !is_orchestrator {
            return Ok(None);
        }
        let args: serde_json::Value = serde_json::from_str(&req.args_json)
            .map_err(|e| Status::invalid_argument(format!("invalid args_json: {e}")))?;
        let reach = self
            .orchestrator_read_reach(&req.session_token, session_dir.clone(), worktree_root)
            .await?;
        let added: Vec<PathBuf> = SessionLspRoots::open(&session_dir)
            .roots()
            .map_err(|e| Status::internal(format!("cannot read the session's LSP roots: {e}")))?
            .into_iter()
            .map(|added| added.root)
            .collect();
        let response = match lsp_root_for(&args, &reach, &added) {
            Ok(root) => {
                self.local_exec_tools
                    .run_exec_tool_locally(req, sessions_base, &root)
                    .await
            }
            Err(refusal) => ExecuteToolResponse {
                is_error: true,
                error_message: format!("{}: {refusal}", req.tool_name),
                ..Default::default()
            },
        };
        record_call(sessions_base, req, &response);
        Ok(Some(response))
    }
}
