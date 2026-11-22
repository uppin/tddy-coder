//! The read-only rule of a project-home session, enforced where exec tools run (`#project-home` 4/7).
//!
//! A session whose recorded role is `project-home` may read and search its home worktree and nothing
//! else. The rule is applied here, on the daemon, rather than left to the agent's spawn-time
//! allowlist, which is harness-specific and which a harness that cannot enforce it ignores.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.

use std::path::Path;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::is_orchestrator_role;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_tool_engine::read_only::is_project_home_allowed_tool;

use super::path_guard::reject_exec_tool_path_traversal;

/// Every refusal a tool call run from this daemon's own sessions base passes before any I/O: a
/// path traversal, and a non-read-only tool in a project-home session.
pub(super) fn reject_refused_local_exec_tool(
    sessions_base: &Path,
    req: &ExecuteToolRequest,
) -> Result<(), Status> {
    reject_exec_tool_path_traversal(&req.tool_name, &req.args_json)?;
    reject_non_read_only_tool_for_project_home(sessions_base, req)
}

/// Whether `req` is an orchestrator's own `Write`, `StrReplace` or `Delete` of a path inside one of
/// its write grants (`#orchestra` 6/9) — the one write a read-only session may make. A subagent
/// conversation's call is never one: its worktree is not the orchestrator's.
fn is_write_inside_a_grant(session_dir: &Path, req: &ExecuteToolRequest) -> bool {
    req.conversation_id.is_empty()
        && super::orchestrator_reach::is_granted_write_tool(&req.tool_name)
        && super::orchestrator_reach::granted_write(session_dir, &req.args_json).is_some()
}

/// Refuse `tool_name` with `PermissionDenied` when `session_id` is a project-home session and the
/// tool is neither a read-only one nor one of the four project-home session tools (their own role
/// gate is in `project_home`). A session whose metadata cannot be read is refused too: the role
/// cannot be known, so the read-only rule cannot be ruled out.
fn reject_non_read_only_tool_for_project_home(
    sessions_base: &Path,
    req: &ExecuteToolRequest,
) -> Result<(), Status> {
    let (session_id, tool_name) = (&req.session_id, &req.tool_name);
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
        Status::failed_precondition(format!(
            "tool {tool_name} is refused: session {session_id}'s metadata cannot be read ({e})"
        ))
    })?;
    if metadata
        .session_role
        .as_deref()
        .is_some_and(is_orchestrator_role)
        && !is_project_home_allowed_tool(tool_name)
        && !is_write_inside_a_grant(&session_dir, req)
    {
        return Err(Status::permission_denied(format!(
            "tool {tool_name} is refused: a project-home session is read-only"
        )));
    }
    Ok(())
}
