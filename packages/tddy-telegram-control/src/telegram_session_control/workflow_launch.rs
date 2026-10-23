//! Starting the workflow session an operator's picker taps chose: from the session's recorded
//! recipe and chain parent to the "Workflow started" message.

use super::*;

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// Spawn `session_id` as a workflow session of `project_id`, running `agent`, and tell
    /// `chat_id` it started.
    pub(super) async fn spawn_telegram_workflow(
        &self,
        chat_id: i64,
        session_id: &str,
        project_id: &str,
        agent: Option<&str>,
    ) -> anyhow::Result<()> {
        let deps = self.workflow_spawn_deps()?;
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let recipe = read_recipe_from_changeset(&session_dir)?;
        let project = project_for_workflow_spawn(&deps, project_id)?;
        self.merge_chain_child_integration_base(&session_dir, Path::new(&project.main_repo_path))
            .await?;
        let result = deps
            .spawn_session(project_id, agent, recipe.as_deref(), session_id)
            .await?;
        follow_spawned_workflow(&deps, &result)?;
        self.announce_started_workflow(chat_id, &result, &project)
            .await
    }

    fn workflow_spawn_deps(&self) -> anyhow::Result<Arc<TelegramWorkflowSpawn>> {
        self.workflow_spawn
            .clone()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))
    }

    /// A chain child starts from its parent's integration base, or from the base the operator set
    /// explicitly on the child. A session with no parent is left as it is.
    async fn merge_chain_child_integration_base(
        &self,
        session_dir: &Path,
        repo_path: &Path,
    ) -> anyhow::Result<()> {
        let Some(parent_id) = read_session_metadata(session_dir)
            .ok()
            .and_then(|meta| meta.previous_session_id)
        else {
            return Ok(());
        };
        let explicit = explicit_integration_base_ref(session_dir);
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "spawn_telegram_workflow: chain child parent_session_id={} explicit_worktree_integration_base_ref={:?}",
            parent_id,
            explicit.as_deref()
        );
        let sessions_base = self.sessions_base.clone();
        let session_dir = session_dir.to_path_buf();
        let repo_path = repo_path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            merge_chain_integration_base_with_explicit_operator_overrides(
                &sessions_base,
                parent_id.as_str(),
                &session_dir,
                &repo_path,
                explicit.as_deref(),
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("chain merge spawn_blocking join: {e}"))??;
        Ok(())
    }

    async fn announce_started_workflow(
        &self,
        chat_id: i64,
        result: &tddy_spawn::spawner::SpawnResult,
        project: &ProjectData,
    ) -> anyhow::Result<()> {
        let sid_short = &result.session_id[..8.min(result.session_id.len())];
        let done = format!(
            "Workflow started (session {sid_short}…). gRPC {} LiveKit room `{}`.",
            result.grpc_port, result.livekit_room
        );
        self.sender.send_message(chat_id, &done).await?;
        self.notify_account_identity_unavailable(chat_id, &result.session_id, project)
            .await
    }
}

/// The project `project_id` names in the registry the workflow spawn reads.
fn project_for_workflow_spawn(
    deps: &TelegramWorkflowSpawn,
    project_id: &str,
) -> anyhow::Result<ProjectData> {
    let projects_dir = projects_dir_for_telegram_workflow_spawn(deps)?;
    project_storage::find_project(&projects_dir, project_id)?
        .ok_or_else(|| anyhow::anyhow!("project not found"))
}

/// The integration base the operator set on the session's changeset, if any.
fn explicit_integration_base_ref(session_dir: &Path) -> Option<String> {
    read_changeset(session_dir)
        .ok()
        .and_then(|cs| cs.worktree_integration_base_ref)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Register the spawned session's gRPC port and start following its presenter stream.
fn follow_spawned_workflow(
    deps: &TelegramWorkflowSpawn,
    result: &tddy_spawn::spawner::SpawnResult,
) -> anyhow::Result<()> {
    deps.child_grpc_by_session
        .lock()
        .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?
        .insert(result.session_id.clone(), result.grpc_port);
    if let Some(ref hooks) = deps.telegram_hooks {
        // TODO(session-notifications): publish this session's presenter events onto the
        // notification bus too, so a Telegram-started workflow session raises the same drawer
        // indicator a web-started one does. It needs the bus (and the sessions base its label
        // is read from) on `TelegramWorkflowSpawn`, which every inbound-control harness
        // constructs by hand; adding it is a change of its own. The Telegram surface for these
        // sessions is unaffected — it never went through the bus.
        tddy_session_lifecycle::presenter_observer_task::spawn_presenter_observer_task(
            Arc::clone(hooks) as SharedOperatorChannel,
            None,
            &result.session_id,
            result.grpc_port,
        );
    }
    Ok(())
}
