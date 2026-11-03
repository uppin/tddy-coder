use tddy_core::write_session_metadata;

use crate::cursor_cli_spawn::chat;
use tddy_daemon_kernel::config::resolve_cursor_binary_path;

use std::path::PathBuf;

use tddy_rpc::Status;

use tddy_service::proto::session::ResumeSessionResponse;

use tddy_rpc::Response;

use tddy_core::SessionMetadata;

use std::path::Path;

use tddy_daemon_kernel::config::DaemonConfig;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;

use std::sync::Arc;

pub async fn resume_cursor_cli_session(
    cli_manager: &Arc<CliSessionManager>,
    config: &DaemonConfig,
    // The session's OS user: a tmux-run cursor resumes as this user.
    os_user: &str,
    session_id: &str,
    session_dir: &Path,
    meta: SessionMetadata,
    // What the resumed agent's environment is given: the project's account identity pairs its
    // commits carry (none keeps the checkout's own) and its notify environment.
    git_environment: Vec<(String, String)>,
) -> Result<Response<ResumeSessionResponse>, Status> {
    let model = meta.model.clone().unwrap_or_default();
    let worktree_path = meta
        .repo_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| session_dir.to_path_buf());

    if !worktree_path.exists() {
        return Err(Status::failed_precondition(
            "worktree no longer exists; cannot resume cursor-cli session",
        ));
    }

    let binary_path = resolve_cursor_binary_path(config);
    // The chat to reattach to. A session started before chat ids were recorded has no way back to
    // its original chat, so its first resume adopts a fresh one and pins it below — every later
    // resume then reattaches instead of starting over again.
    let chat_id = match meta
        .cursor_chat_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(id) => id.to_string(),
        None => {
            let adopted = chat::mint_cursor_chat_id(&binary_path, &worktree_path)
                .await
                .map_err(|e| {
                    Status::internal(format!(
                        "failed to create the Cursor chat for session {session_id}: {e}"
                    ))
                })?;
            log::info!(
                target: "tddy_daemon::cursor_cli_spawn",
                "resume_cursor_cli_session {session_id}: no cursor_chat_id on record; adopted chat {adopted} for this and every later resume"
            );
            adopted
        }
    };

    let (pid, tmux_session) = if meta.tmux_session.is_some() {
        // A tmux-run session is re-attached when its tmux session is alive, else started fresh,
        // as the session's OS user (`#tmux-agent` 3/3).
        let launched = crate::cli_harness_tmux::resume_harness_in_tmux(
            cli_manager,
            crate::cli_harness_tmux::HarnessTmuxLaunch {
                os_user,
                session_id,
                worktree_path: worktree_path.clone(),
                kind: "cursor-cli",
                argv: CliSessionManager::build_cursor_argv(
                    &binary_path,
                    &model,
                    Some(&chat_id),
                    None,
                ),
                env: git_environment,
                on_exit: None,
            },
        )
        .await?;
        (launched.pane_pid, Some(launched.tmux_session))
    } else {
        let handle = cli_manager
            .resume_cursor(
                session_id,
                worktree_path.clone(),
                &model,
                &binary_path,
                Some(&chat_id),
                git_environment,
            )
            .await
            .map_err(|e| Status::internal(format!("failed to resume cursor-cli: {}", e)))?;
        (handle.pid, None)
    };
    let mut updated = meta;
    updated.tmux_session = tmux_session;
    updated.cursor_chat_id = Some(chat_id);
    updated.pid = Some(pid);
    updated.status = "active".to_string();
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_metadata(session_dir, &updated)
        .map_err(|e| Status::internal(format!("failed to update session metadata: {}", e)))?;

    Ok(Response::new(ResumeSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
    }))
}
