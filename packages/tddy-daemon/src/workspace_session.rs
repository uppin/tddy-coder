//! Workspace session type: git worktree-backed, tool-only (no PTY, no LiveKit bridge).
//!
//! A workspace session provides a git worktree and exposes it exclusively via
//! `ExecuteTool` RPCs. It is lighter than a `claude-cli` session: no PTY is spawned,
//! no LiveKit bridge is created, and no agent process runs.

use std::path::{Path, PathBuf};

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_rpc::{Response, Status};
use tddy_service::proto::connection::StartSessionResponse;

use crate::project_storage;
use crate::user_sessions_path::projects_path_for_user;

/// The branch a workspace session's worktree is cut on, as asked for by the request that created it.
///
/// A workspace session is also the codebase half of a split session
/// (docs/ft/daemon/remote-managed-worktree.md), where the caller chose a branch in the new-session
/// form and expects the agent to find it — so the intent travels with the forwarded request rather
/// than being reinvented on the codebase host.
#[derive(Debug, Default, Clone)]
pub struct WorkspaceBranchIntent<'a> {
    pub branch_worktree_intent: &'a str,
    pub new_branch_name: &'a str,
    pub selected_integration_base_ref: &'a str,
    pub selected_branch_to_work_on: &'a str,
}

/// Create a workspace session: resolve the project, create a git worktree, write `.session.yaml`,
/// and return a `StartSessionResponse` with empty LiveKit fields.
#[allow(clippy::too_many_arguments)]
pub async fn start_workspace_session(
    os_user: &str,
    session_id: &str,
    sessions_base: PathBuf,
    project_id: &str,
    branch: &WorkspaceBranchIntent<'_>,
    tddy_data_dir: &Path,
    request_timeout: std::time::Duration,
) -> Result<Response<StartSessionResponse>, Status> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(Status::invalid_argument(
            "project_id is required for workspace sessions",
        ));
    }

    // Resolve project registry.
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

    // Resolved before anything is created: a branch intent this daemon cannot honour is a malformed
    // request, and a request refused after a session directory exists leaves the caller — which for
    // a split session is another daemon — to clean up something it never wanted.
    let workflow = crate::branch_intent::resolve_branch_workflow(
        session_id,
        &crate::branch_intent::BranchIntentRequest {
            branch_worktree_intent: branch.branch_worktree_intent,
            new_branch_name: branch.new_branch_name,
            selected_integration_base_ref: branch.selected_integration_base_ref,
            selected_branch_to_work_on: branch.selected_branch_to_work_on,
        },
        crate::branch_intent::BranchIntentPolicy::workspace(),
        project.main_branch_ref.as_deref(),
    )?
    .workflow;

    // Create session directory.
    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

    // Write a minimal changeset so `setup_worktree_for_session_with_optional_chain_base` can read it.
    let cs = tddy_core::Changeset {
        workflow: Some(workflow),
        ..Default::default()
    };
    tddy_core::write_changeset(&session_dir, &cs)
        .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

    // Create the real git worktree (blocking: involves git fetch + git worktree add).
    let repo_root_clone = repo_root.clone();
    let session_dir_clone = session_dir.clone();
    let base_ref = Some(branch.selected_integration_base_ref.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let worktree_path = tokio::time::timeout(
        request_timeout,
        tokio::task::spawn_blocking(move || {
            tddy_core::setup_worktree_for_session_with_optional_chain_base(
                &repo_root_clone,
                &session_dir_clone,
                base_ref.as_deref(),
            )
            .map_err(|e| anyhow::anyhow!("worktree setup failed: {}", e))
        }),
    )
    .await
    .map_err(|_| Status::deadline_exceeded("start_workspace_session: create worktree timed out"))?
    .map_err(|join_err| Status::internal(join_err.to_string()))?
    .map_err(|e: anyhow::Error| Status::internal(e.to_string()))?;

    // Write .session.yaml — no PID (no agent process for workspace sessions).
    let now = chrono::Utc::now().to_rfc3339();
    let meta = tddy_core::SessionMetadata {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        created_at: now.clone(),
        updated_at: now,
        status: "active".to_string(),
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("workspace".to_string()),
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
    };
    tddy_core::write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;

    log::info!(
        target: "tddy_daemon::workspace_session",
        "started workspace session {} worktree={} user={}",
        session_id,
        worktree_path.display(),
        os_user
    );

    Ok(Response::new(StartSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
        branch_conflict: None,
    }))
}

/// Resolve the worktree root for a session by reading `.session.yaml`.
pub fn resolve_worktree_root_for_session(
    sessions_base: &Path,
    session_id: &str,
) -> Result<PathBuf, Status> {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let meta = tddy_core::read_session_metadata(&session_dir)
        .map_err(|_| Status::failed_precondition("session not found or .session.yaml missing"))?;
    meta.repo_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| Status::failed_precondition("session .session.yaml has no repo_path"))
}
