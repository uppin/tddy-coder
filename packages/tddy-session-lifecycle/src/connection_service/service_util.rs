use tddy_daemon_kernel::trim_to_option;

use std::path::Path;

use tddy_core::BranchWorktreeIntent;

use tddy_rpc::Status;

use std::time::Duration;

/// Build a session's semantic index over its worktree into its session dir, blocking until the
/// index is terminal.
///
/// A missing embedder or a failed index is an error — no unindexed fallback — so a start that asked
/// for the index fails rather than coming up without it. The index's env pair, when a caller needs
/// one, is `tddy_semantic_index::semantic_index::semantic_index_env(session_dir)`.
pub(crate) async fn index_session_worktree(
    tddy_data_dir: &Path,
    task_registry: &tddy_task::TaskRegistry,
    session_id: &str,
    worktree_path: &Path,
    session_dir: &Path,
) -> Result<(), Status> {
    let embedder = tddy_semantic_index::production_embedder(tddy_data_dir).map_err(|e| {
        Status::failed_precondition(format!(
            "semantic index requested but no embedder is available: {e}"
        ))
    })?;
    tddy_semantic_index::semantic_index::run_semantic_index_blocking(
        worktree_path,
        session_dir,
        embedder,
        task_registry,
        session_id,
    )
    .await
    .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
    Ok(())
}

/// Resolve a starting session's branch intent and write the changeset its worktree setup reads.
///
/// The changeset names the orchestrator a stack child was spawned by, and a managed session's
/// recipe. A managed session also seeds the recipe's start goal, so `changeset.yaml` reflects the
/// workflow position immediately; the per-session controller advances it from there on
/// `transition`. Returns the intent, which decides whether the new branch is pushed.
pub(crate) fn write_initial_changeset(
    session_id: &str,
    branch: &crate::branch_intent::BranchIntentRequest<'_>,
    policy: crate::branch_intent::BranchIntentPolicy,
    project_main_branch_ref: Option<&str>,
    session_dir: &Path,
    orchestrator_session_id: Option<&str>,
    managed_recipe: Option<&dyn tddy_core::workflow::recipe::WorkflowRecipe>,
) -> Result<BranchWorktreeIntent, Status> {
    let crate::branch_intent::ResolvedBranchWorkflow { intent, workflow } =
        crate::branch_intent::resolve_branch_workflow(
            session_id,
            branch,
            policy,
            project_main_branch_ref,
        )?;
    let mut cs = tddy_core::Changeset {
        workflow: Some(workflow),
        orchestrator_session_id: orchestrator_session_id.map(str::to_string),
        recipe: managed_recipe.map(|r| r.name().to_string()),
        ..tddy_core::Changeset::default()
    };
    if let Some(recipe) = managed_recipe {
        tddy_core::changeset::update_state(
            &mut cs,
            tddy_core::workflow::ids::WorkflowState::new(recipe.start_goal().as_str()),
        );
    }
    tddy_core::write_changeset(session_dir, &cs)
        .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;
    Ok(intent)
}

/// Cut a session's git worktree from `repo_root` into `session_dir` (blocking: a fetch plus
/// `git worktree add`), based on `base_ref` when there is one, under the spawn deadline.
pub(crate) async fn create_session_worktree(
    timeout: Duration,
    op_label: &'static str,
    repo_root: &Path,
    session_dir: &Path,
    base_ref: Option<String>,
) -> Result<std::path::PathBuf, Status> {
    let repo_root = repo_root.to_path_buf();
    let session_dir = session_dir.to_path_buf();
    spawn_blocking_with_timeout(timeout, op_label, move || {
        tddy_core::setup_worktree_for_session_with_optional_chain_base(
            &repo_root,
            &session_dir,
            base_ref.as_deref(),
        )
        .map_err(|e| anyhow::anyhow!("worktree setup failed: {e}"))
    })
    .await
}

/// Runs blocking clone/spawn work with a wall-clock cap so hung NSS/git/spawn cannot block RPCs forever.
pub async fn spawn_blocking_with_timeout<T: Send + 'static>(
    timeout: Duration,
    op_label: &'static str,
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, Status> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(Ok(v))) => Ok(v),
        Ok(Ok(Err(e))) => {
            log::error!("{} failed: {}", op_label, e);
            Err(Status::internal(e.to_string()))
        }
        Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
        Err(_elapsed) => {
            log::error!(
                "{} timed out after {}s (spawn_worker_request_timeout_secs); blocking task may still run in the pool",
                op_label,
                timeout.as_secs()
            );
            Err(Status::deadline_exceeded(format!(
                "{}: timed out after {}s (see daemon log: spawner: child I/O paths; if same_user=false, parent blocks until pre_exec/initgroups completes)",
                op_label,
                timeout.as_secs()
            )))
        }
    }
}

/// Await a `tddy-supervisor`-brokered operation under the same deadline the forked spawn backend
/// gets from [`spawn_blocking_with_timeout`].
///
/// An unreachable or refusing supervisor fails the RPC. There is deliberately no local spawn to fall
/// back to: doing the work here would run a session as the daemon's own user, which is the isolation
/// the supervisor exists to provide.
pub async fn await_supervised_with_timeout<T>(
    timeout: Duration,
    op_label: &'static str,
    operation: impl std::future::Future<Output = anyhow::Result<T>>,
) -> Result<T, Status> {
    match tokio::time::timeout(timeout, operation).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => {
            log::error!("{} failed: {:#}", op_label, e);
            Err(Status::internal(format!("{e:#}")))
        }
        Err(_elapsed) => {
            log::error!(
                "{} timed out after {}s (spawn_worker_request_timeout_secs) waiting for tddy-supervisor",
                op_label,
                timeout.as_secs()
            );
            Err(Status::deadline_exceeded(format!(
                "{}: tddy-supervisor did not answer within {}s",
                op_label,
                timeout.as_secs()
            )))
        }
    }
}

/// After a `new_branch_from_base` worktree is created, optionally push the freshly created branch to
/// its remote. Reads the actual created branch from the session's changeset (it may carry a
/// collision suffix), resolves the remote from the persisted integration base ref
/// (`<remote>/<branch>`) — falling back to main-worktree detection then `origin` — runs
/// `git push -u <remote> <branch>` from the worktree, and records `Changeset.remote_pushed = true`.
/// A push failure fails the session start — no silent fallback.
pub(crate) async fn push_new_branch_to_origin_if_requested(
    create_remote_branch: bool,
    intent: BranchWorktreeIntent,
    session_dir: &Path,
    worktree_path: &Path,
    timeout: Duration,
) -> Result<(), Status> {
    if !create_remote_branch || !matches!(intent, BranchWorktreeIntent::NewBranchFromBase) {
        return Ok(());
    }
    let session_dir = session_dir.to_path_buf();
    let worktree_path = worktree_path.to_path_buf();
    spawn_blocking_with_timeout(
        timeout,
        "StartSession: push new branch to remote",
        move || {
            let mut cs = tddy_core::read_changeset(&session_dir)
                .map_err(|e| anyhow::anyhow!("read changeset for remote push: {e}"))?;
            let branch = cs
                .branch
                .clone()
                .ok_or_else(|| anyhow::anyhow!("no branch recorded after worktree setup"))?;
            // Resolve the remote from the persisted integration base ref (`<remote>/<branch>`),
            // falling back to main-worktree detection then `origin` as the last resort.
            let remote = cs
                .effective_worktree_integration_base_ref
                .as_deref()
                .and_then(|r| r.split_once('/').map(|(remote, _)| remote.to_string()))
                .or_else(|| tddy_core::worktree::detect_default_remote_name(&worktree_path))
                .unwrap_or_else(|| "origin".to_string());
            tddy_core::worktree::push_new_branch_to_remote(&worktree_path, &branch, &remote)
                .map_err(|e| anyhow::anyhow!(e))?;
            cs.remote_pushed = true;
            tddy_core::write_changeset(&session_dir, &cs)
                .map_err(|e| anyhow::anyhow!("write changeset after remote push: {e}"))?;
            Ok(())
        },
    )
    .await
}

/// Derives the agent and recipe to relaunch a resumed session with, from its persisted
/// `.session.yaml`. Empty/whitespace-only values are treated as absent (`None`), mirroring the
/// spawner's trimming, so a legacy session with no persisted agent/recipe restores as `None`.
pub(crate) fn resume_agent_and_recipe(
    metadata: &tddy_core::SessionMetadata,
) -> (Option<String>, Option<String>) {
    (
        metadata.agent.as_deref().and_then(trim_to_option),
        metadata.recipe.as_deref().and_then(trim_to_option),
    )
}
