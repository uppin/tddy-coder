//! Cursor Agent CLI session spawn/resume helpers for `ConnectionServiceImpl`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::{
    build_cursor_hooks_settings, write_session_metadata, BranchWorktreeIntent, Changeset,
    ChangesetWorkflow, HookCommandParams, SessionMetadata,
};
use tddy_rpc::{Response, Status};
use tddy_service::proto::connection::{ResumeSessionResponse, StartSessionResponse};
use uuid::Uuid;

use crate::cli_session_manager::CliSessionManager;
use crate::config::{resolve_cursor_binary_path, DaemonConfig};
use crate::connection_service::spawn_blocking_with_timeout;
use crate::project_storage;
use crate::user_sessions_path::projects_path_for_user;

/// Write `.cursor/hooks.json` under `worktree_path` for a cursor-cli session.
///
/// Returns the generated per-session hook token (also embedded in hook commands).
pub fn install_cursor_hooks_in_worktree(
    config: &DaemonConfig,
    worktree_path: &Path,
    session_id: &str,
    os_user: &str,
) -> String {
    let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
        crate::config::resolve_cursor_cli_tddy_tools_path(config).as_deref(),
    );

    let daemon_url = crate::config::resolve_cursor_cli_daemon_url(config).unwrap_or_else(|| {
        let port = config.listen.web_port.unwrap_or(8899);
        format!("http://127.0.0.1:{port}")
    });

    let hook_token = Uuid::new_v4().to_string();
    let hooks_settings = build_cursor_hooks_settings(&HookCommandParams {
        tddy_tools_path: &tddy_tools_path,
        daemon_url: &daemon_url,
        session_id,
        os_user,
        hook_token: &hook_token,
    });
    let cursor_dir = worktree_path.join(".cursor");
    if let Err(e) = std::fs::create_dir_all(&cursor_dir).and_then(|_| {
        serde_json::to_string_pretty(&hooks_settings)
            .map_err(|e| std::io::Error::other(e.to_string()))
            .and_then(|json| std::fs::write(cursor_dir.join("hooks.json"), json))
    }) {
        log::warn!(
            "session {session_id}: failed to write .cursor/hooks.json — hooks will not fire: {e}"
        );
    }
    hook_token
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_cursor_cli_session_inner(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    cli_manager: &Arc<CliSessionManager>,
    os_user: &str,
    session_id: &str,
    sessions_base: PathBuf,
    model: &str,
    project_id: &str,
    branch_worktree_intent: &str,
    new_branch_name: &str,
    selected_integration_base_ref: &str,
    selected_branch_to_work_on: &str,
    initial_prompt: &str,
) -> Result<Response<StartSessionResponse>, Status> {
    if model.trim().is_empty() {
        return Err(Status::invalid_argument(
            "model is required for cursor-cli sessions",
        ));
    }
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(Status::invalid_argument(
            "project_id is required for cursor-cli sessions",
        ));
    }
    let projects_dir = projects_path_for_user(os_user, Some(tddy_data_dir))
        .ok_or_else(|| Status::internal("could not resolve projects path"))?;
    let project = project_storage::find_project(&projects_dir, project_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("project not found"))?;
    let repo_root = PathBuf::from(&project.main_repo_path);
    if !repo_root.exists() {
        return Err(Status::invalid_argument(
            "project main repo path does not exist",
        ));
    }

    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

    let short_id = &session_id[..8.min(session_id.len())];
    let (intent, resolved_new_branch, resolved_selected_branch) = match branch_worktree_intent
        .trim()
    {
        "new_branch_from_base" => {
            let branch = if new_branch_name.trim().is_empty() {
                format!("cursor-cli/{short_id}")
            } else {
                new_branch_name.trim().to_string()
            };
            (BranchWorktreeIntent::NewBranchFromBase, Some(branch), None)
        }
        "work_on_selected_branch" => {
            if selected_branch_to_work_on.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "selected_branch_to_work_on is required when branch_worktree_intent is work_on_selected_branch",
                ));
            }
            (
                BranchWorktreeIntent::WorkOnSelectedBranch,
                None,
                Some(selected_branch_to_work_on.trim().to_string()),
            )
        }
        _ => (
            BranchWorktreeIntent::NewBranchFromBase,
            Some(format!("cursor-cli/{short_id}")),
            None,
        ),
    };

    let cs_workflow = ChangesetWorkflow {
        branch_worktree_intent: Some(intent),
        new_branch_name: resolved_new_branch,
        selected_integration_base_ref: if selected_integration_base_ref.trim().is_empty() {
            None
        } else {
            Some(selected_integration_base_ref.trim().to_string())
        },
        selected_branch_to_work_on: resolved_selected_branch,
        ..ChangesetWorkflow::default()
    };
    let cs = Changeset {
        workflow: Some(cs_workflow),
        ..Changeset::default()
    };
    tddy_core::write_changeset(&session_dir, &cs)
        .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

    let repo_root_clone = repo_root.clone();
    let session_dir_clone = session_dir.clone();
    let timeout = config.spawn_worker_request_timeout();
    let worktree_path = spawn_blocking_with_timeout(
        timeout,
        "start_cursor_cli_session: create worktree",
        move || {
            tddy_core::setup_worktree_for_session_with_optional_chain_base(
                &repo_root_clone,
                &session_dir_clone,
                None,
            )
            .map_err(|e| anyhow::anyhow!("worktree setup failed: {}", e))
        },
    )
    .await?;

    let hook_token = install_cursor_hooks_in_worktree(config, &worktree_path, session_id, os_user);

    let binary_path = resolve_cursor_binary_path(config);
    let initial_prompt_opt = {
        let p = initial_prompt.trim();
        if p.is_empty() {
            None
        } else {
            Some(p.to_string())
        }
    };

    let handle = cli_manager
        .start_cursor(
            session_id,
            worktree_path.clone(),
            model,
            &binary_path,
            initial_prompt_opt.as_deref(),
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn cursor-cli: {}", e)))?;

    let pid = handle.pid;
    let now = chrono::Utc::now().to_rfc3339();
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        created_at: now.clone(),
        updated_at: now,
        status: "active".to_string(),
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(pid),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("cursor-cli".to_string()),
        model: Some(model.to_string()),
        activity_status: None,
        hook_token: Some(hook_token),
        sandbox: None,
        agent: None,
        recipe: None,
        specialized_agents: Vec::new(),
    };
    write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;

    log::info!(
        target: "tddy_daemon::connection_service",
        "started cursor-cli session {} pid={} worktree={} user={}",
        session_id,
        pid,
        worktree_path.display(),
        os_user
    );

    Ok(Response::new(StartSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
    }))
}

pub async fn resume_cursor_cli_session(
    cli_manager: &Arc<CliSessionManager>,
    config: &DaemonConfig,
    session_id: &str,
    session_dir: &Path,
    meta: SessionMetadata,
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
    let handle = cli_manager
        .resume_cursor(session_id, worktree_path.clone(), &model, &binary_path)
        .await
        .map_err(|e| Status::internal(format!("failed to resume cursor-cli: {}", e)))?;

    let pid = handle.pid;
    let mut updated = meta;
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
