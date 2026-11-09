use std::sync::Arc;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;

pub(crate) fn write_split_agent_metadata(
    session_id: &str,
    codebase_instance_id: &str,
    codebase_session_id: &str,
    req: &StartSessionRequest,
    session_dir: std::path::PathBuf,
    hook_token: String,
    handle: &Arc<tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle>,
) -> Result<(), Status> {
    let meta = tddy_core::SessionMetadata {
        // No repository on this host — the pairing below is how the worktree is found.
        repo_path: None,
        pid: Some(handle.pid),
        model: Some(req.model.trim().to_string()),
        hook_token: Some(hook_token),
        codebase_daemon_instance_id: Some(codebase_instance_id.to_string()),
        codebase_session_id: Some(codebase_session_id.to_string()),
        managed_codebase: req.managed_codebase,
        ..crate::service_util::starting_session_metadata(
            session_id,
            req.project_id.trim(),
            "claude-cli",
        )
    };
    tddy_core::write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
    Ok(())
}
