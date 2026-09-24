use tddy_rpc::Status;

use std::sync::Arc;

use std::path::Path;

use std::path::PathBuf;

/// Launch inputs for a managed claude-cli session, produced by
/// [`DaemonSessionHost::prepare_managed_workflow`]: the workflow wiring (whose listener must be
/// kept alive for the session's lifetime), the orchestration-prompt file to append to claude's
/// system prompt, and the per-session env (`TDDY_SOCKET` + `PATH`) for host-side `tddy-tools`.
pub(crate) struct ManagedLaunch {
    pub(crate) workflow: crate::session_toolcall::ManagedWorkflow,
    pub(crate) prompt_file: PathBuf,
    pub(crate) env: Vec<(String, String)>,
}

/// Free-function form of [`DaemonSessionHost::prepare_managed_workflow`] so the shared
/// claude-cli spawn logic ([`spawn_claude_cli_session_inner`]) — which has no `self` — can reuse it.
/// `child_spawn_handler`, when present, is bound to the managed session's toolcall listener so the
/// agent's `pr_spawn_child` relay reaches a spawner (used for PR-stack orchestrators).
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_managed_workflow_inner(
    tddy_data_dir: &Path,
    session_id: &str,
    recipe: Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>,
    session_dir: &Path,
    worktree_path: &Path,
    prompt_dir: &Path,
    tddy_tools_path: &str,
    resume_at: Option<tddy_core::workflow::ids::GoalId>,
    child_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>>,
    conversation_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
) -> Result<ManagedLaunch, Status> {
    let mw = match resume_at {
        Some(goal) => crate::session_toolcall::resume_managed_workflow(
            session_id,
            recipe,
            session_dir,
            worktree_path,
            tddy_data_dir,
            &std::env::temp_dir(),
            goal,
            child_spawn_handler,
            conversation_spawn_handler,
        ),
        None => crate::session_toolcall::set_up_managed_workflow(
            session_id,
            recipe,
            session_dir,
            worktree_path,
            tddy_data_dir,
            &std::env::temp_dir(),
            child_spawn_handler,
            conversation_spawn_handler,
        ),
    }
    .map_err(Status::internal)?;

    let prompt_path = prompt_dir.join("orchestration-prompt.txt");
    std::fs::write(&prompt_path, &mw.orchestration_prompt)
        .map_err(|e| Status::internal(format!("failed to write orchestration prompt: {e}")))?;

    // A managed session's `tddy-tools` MCP process needs these to locate the orchestrator's
    // changeset (`TDDY_SESSION_DIR`) and run `git` against the repo (`TDDY_REPO_DIR`) — the
    // PR-management tools read both. The tddy-coder TUI backends set them; the daemon's managed
    // claude-cli launch must set them here too, or those tools have no session/repo in scope.
    let mut env: Vec<(String, String)> = vec![
        (
            "TDDY_SOCKET".to_string(),
            mw.listener.socket_path().to_string_lossy().into_owned(),
        ),
        (
            "TDDY_SESSION_DIR".to_string(),
            session_dir.to_string_lossy().into_owned(),
        ),
        (
            "TDDY_REPO_DIR".to_string(),
            worktree_path.to_string_lossy().into_owned(),
        ),
    ];
    if let Some(dir) = std::path::Path::new(tddy_tools_path)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        let existing = std::env::var("PATH").unwrap_or_default();
        env.push(("PATH".to_string(), format!("{}:{existing}", dir.display())));
    }
    Ok(ManagedLaunch {
        workflow: mw,
        prompt_file: prompt_path,
        env,
    })
}
