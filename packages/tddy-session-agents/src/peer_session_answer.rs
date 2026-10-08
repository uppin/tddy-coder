pub fn peer_has_no_such_session(status: &Status) -> bool {
    matches!(
        status.code,
        tddy_rpc::Code::FailedPrecondition | tddy_rpc::Code::NotFound
    )
}

use crate::exec_tool_caller;
use std::path::{Path, PathBuf};
use tddy_core::session_lifecycle::validate_session_id_segment;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::ExecuteToolRequest;

/// The codebase daemon and the workspace session on it that a session is paired with, or `None`
/// when the session is co-located.
///
/// The pairing is the *pair* — a recorded daemon with no session id names a host but nothing on it
/// to resume, re-wire or delete, so half a pairing is read as none rather than acted on. Every
/// caller needs both, so the check lives here instead of at each of them.
pub fn split_pairing(meta: &tddy_core::SessionMetadata) -> Option<(&str, &str)> {
    fn non_blank(field: &Option<String>) -> Option<&str> {
        field.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }
    Some((
        non_blank(&meta.codebase_daemon_instance_id)?,
        non_blank(&meta.codebase_session_id)?,
    ))
}

/// Resolve the worktree root for a session by reading `.session.yaml`.
pub fn resolve_worktree_root_for_session(
    sessions_base: &Path,
    session_id: &str,
) -> Result<PathBuf, Status> {
    resolve_worktree_root_in_session_dir(&tddy_core::session_lifecycle::unified_session_dir_path(
        sessions_base,
        session_id,
    ))
}

/// Resolve, on this daemon, the sessions base and the worktree an exec tool runs in — for a
/// caller [`authorize_exec_tool_caller`] has already accepted.
pub fn resolve_exec_tool_worktree(
    config: &DaemonConfig,
    user_resolver: &SessionUserResolver,
    tddy_data_dir: &Path,
    req: &ExecuteToolRequest,
) -> Result<(PathBuf, PathBuf), Status> {
    let os_user = &exec_tool_caller::authorize_exec_tool_caller(config, user_resolver, req)?;

    validate_session_id_segment(&req.session_id)
        .map_err(|e| Status::invalid_argument(e.message()))?;
    // A conversation id becomes a directory and a branch name; one that could escape either is a
    // routing failure, settled here so no tool of the call has run when it is refused.
    if !req.conversation_id.is_empty() {
        tddy_subagent_worktree::ConversationId::parse(&req.conversation_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
    }

    let sessions_base =
        tddy_daemon_kernel::user_paths::sessions_base_for_user(os_user, Some(tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
    let worktree_root = crate::peer_session_answer::resolve_worktree_root_for_session(
        &sessions_base,
        &req.session_id,
    )?;
    Ok((sessions_base, worktree_root))
}

/// The worktree root the `.session.yaml` in `session_dir` records.
pub fn resolve_worktree_root_in_session_dir(session_dir: &Path) -> Result<PathBuf, Status> {
    let meta = tddy_core::read_session_metadata(session_dir)
        .map_err(|_| Status::failed_precondition("session not found or .session.yaml missing"))?;
    meta.repo_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| Status::failed_precondition("session .session.yaml has no repo_path"))
}
