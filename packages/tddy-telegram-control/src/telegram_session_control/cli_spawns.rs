use crate::telegram_session_control::*;

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// Handle the model-picker callback (`tcm:<model_idx>|p:<proj_idx>|s:<session_id>`).
    ///
    /// Persists the chosen model into `changeset.yaml`, then triggers
    /// [`Self::spawn_telegram_claude_cli`] to set up the worktree and launch `claude`.
    pub async fn handle_telegram_claude_model_callback(
        &self,
        chat_id: i64,
        model_idx: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let model_id = CLAUDE_CLI_MODELS
            .get(model_idx)
            .map(|m| m.id.as_str())
            .ok_or_else(|| anyhow::anyhow!("invalid claude model index {model_idx}"))?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index {proj_idx}"))?;
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        self.persist_changeset_model(&session_dir, model_id)?;
        self.spawn_telegram_claude_cli(chat_id, session_id, &project.project_id)
            .await
    }

    /// Spawn a Claude Code CLI session for an already-configured changeset (branch intent +
    /// `initial_prompt` + `model` written by previous keyboard callbacks).
    ///
    /// Creates the git worktree, launches `claude --model <m> --session-id <id> [<prompt>]` via
    /// [`tddy_session_lifecycle::cli_session_manager::CliSessionManager`], writes `.session.yaml`,
    /// and replies with the session id and attach instructions.
    async fn spawn_telegram_claude_cli(
        &self,
        chat_id: i64,
        session_id: &str,
        project_id: &str,
    ) -> anyhow::Result<()> {
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?
            .clone();

        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let snap = read_changeset_routing_snapshot(&session_dir).unwrap_or_else(|_| {
            ChangesetRoutingSnapshot {
                recipe: None,
                initial_prompt: None,
                demo_options: None,
                run_optional_step_x: None,
                workflow: None,
                session_type: None,
                model: None,
            }
        });

        let model = snap.model.clone().unwrap_or_default();
        if model.trim().is_empty() {
            anyhow::bail!("model not set for claude-cli session (model keyboard not completed)");
        }
        let initial_prompt = snap.initial_prompt.clone();

        let projects_dir = projects_dir_for_telegram_workflow_spawn(&deps)?;
        let project = project_storage::find_project(&projects_dir, project_id)?
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_id}"))?;
        let repo_root = std::path::Path::new(&project.main_repo_path);
        if !repo_root.exists() {
            anyhow::bail!(
                "project main repo path does not exist: {}",
                repo_root.display()
            );
        }

        // The changeset on disk already has the correct branch intent (written by the branch
        // callback) and model (raw-merged by persist_changeset_model).
        let cs = tddy_core::read_changeset(&session_dir)?;
        let new_branch_name = cs
            .workflow
            .as_ref()
            .and_then(|w| w.new_branch_name.as_deref())
            .unwrap_or("");
        let chain_base = tddy_core::resolve_chain_base_for_session_spawn(
            &self.sessions_base,
            None,
            repo_root,
            // The Telegram start names no planned node: it starts a session, never a pr-stack
            // node, so there is nothing for the node-keyed lookup to prefer.
            "",
            new_branch_name,
            cs.worktree_integration_base_ref.as_deref(),
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Setup the worktree (blocking: involves git fetch + git worktree add).
        let repo_root_owned = repo_root.to_path_buf();
        let session_dir_clone = session_dir.clone();
        let worktree_path: std::path::PathBuf = tokio::task::spawn_blocking(move || {
            tddy_core::setup_worktree_for_session_with_optional_chain_base(
                &repo_root_owned,
                &session_dir_clone,
                chain_base.as_deref(),
            )
            .map_err(|e| anyhow::anyhow!("worktree setup failed: {e}"))
        })
        .await
        .map_err(|e| anyhow::anyhow!("worktree join error: {e}"))?
        .map_err(|e| anyhow::anyhow!("worktree setup error: {e}"))?;

        let binary_path = deps
            .config
            .claude_cli
            .as_ref()
            .map(|c| c.binary_path.as_str())
            .unwrap_or("claude")
            .to_string();

        let manager = Arc::clone(&deps.claude_cli_manager);
        let session_id_owned = session_id.to_string();
        let model_owned = model.clone();
        let initial_prompt_owned = initial_prompt.clone();
        let worktree_clone = worktree_path.clone();

        let handle = manager
            .start(
                &session_id_owned,
                worktree_clone,
                &model_owned,
                &binary_path,
                initial_prompt_owned.as_deref(),
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("failed to spawn claude-cli: {e}"))?;

        let pid = handle.pid;

        // Write .session.yaml.
        let now = chrono::Utc::now().to_rfc3339();
        let meta = tddy_core::SessionMetadata {
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
            session_type: Some("claude-cli".to_string()),
            model: Some(model.clone()),
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
            agent_daemon_instance_id: None,
            agent_session_id: None,
            ssh_config_host: None,
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| anyhow::anyhow!("write session metadata: {e}"))?;

        // Bridged here and now, unlike the daemon's own claude-cli start, which records how its
        // terminal is exposed and leaves the joining to the first LiveKit consumer.
        //
        // Because here that consumer has already arrived. The reply below hands a human the room
        // and identity to attach with (`tddy-tools pty-relay --server-identity`), and `pty-relay`
        // joins the room and waits for that participant — it calls nothing that would open the
        // session first. Deferring would make the message an invitation to an empty room, and
        // reporting the coordinates only once someone had asked over LiveKit is a contradiction
        // when the asking *is* the message. What is deferred elsewhere is LiveKit work on a local
        // operation; a Telegram start's last act is a LiveKit announcement.
        let (lk_room, _lk_url, lk_server_identity) = if let Some(lk) =
            tddy_spawn::spawner::livekit_creds_from_config(&deps.config)
        {
            let room_name = tddy_spawn::spawner::resolve_livekit_room_name(
                lk.common_room.as_deref(),
                session_id,
            );
            let server_identity = tddy_spawn::spawner::livekit_server_identity_for_session(
                lk.daemon_instance_id.as_deref(),
                session_id,
            );
            let at = tddy_daemon_extensions::TerminalAddress {
                room: room_name,
                identity: server_identity,
            };
            match self
                .terminal_exposure
                .expose(
                    &at,
                    tddy_session_lifecycle::cli_session_manager::pty_terminal_service(Arc::clone(
                        &handle,
                    )),
                    // A Telegram-started session is part of no stack, so its block carries the
                    // identity and static fields only — published all the same, so the drawer
                    // names it the same way it names every other session (D37).
                    Some(
                        tddy_core::session_participant_metadata::session_metadata_json(
                            &tddy_core::session_participant_metadata::SessionParticipantMetadata {
                                agent: "claude".to_string(),
                                model: model.clone(),
                                repo_path: worktree_path.to_string_lossy().to_string(),
                                session_id: session_id.to_string(),
                                ..Default::default()
                            },
                        ),
                    ),
                )
                .await
            {
                // The running exposure is dropped rather than kept: a Telegram-started session's
                // terminal is announced to a human in the reply below and never asked about again
                // from here.
                Ok(_serving) => (at.room, lk.url.clone(), at.identity),
                // Nothing wired to expose through is the port's "not exposed", not a failure: a
                // daemon without LiveKit credentials reaches here with its null object, and a
                // warning on every start would cry wolf.
                Err(refused) if exposure_is_unwired(&refused) => {
                    (String::new(), String::new(), String::new())
                }
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::telegram_session_control",
                        "spawn_telegram_claude_cli: terminal exposure failed ({e}); gRPC path still works"
                    );
                    (String::new(), String::new(), String::new())
                }
            }
        } else {
            (String::new(), String::new(), String::new())
        };

        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "spawn_telegram_claude_cli: started session {} pid={} worktree={} project={}",
            session_id,
            pid,
            worktree_path.display(),
            project_id,
        );

        let sid_short = &session_id[..8.min(session_id.len())];
        let attach_hint = if lk_room.is_empty() {
            "Attach via the web UI or `tddy-tools pty-relay`.".to_string()
        } else {
            format!("LiveKit room `{lk_room}`. Attach via the web UI or `tddy-tools pty-relay --server-identity {lk_server_identity}`.")
        };
        let done = format!("Claude Code CLI session started ({sid_short}…). {attach_hint}");
        self.sender.send_message(chat_id, &done).await?;
        self.notify_account_identity_unavailable(chat_id, session_id, &project)
            .await
    }

    pub async fn handle_telegram_cursor_model_callback(
        &self,
        chat_id: i64,
        model_idx: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let model_id = CURSOR_CLI_MODELS
            .get(model_idx)
            .map(|m| m.id.as_str())
            .ok_or_else(|| anyhow::anyhow!("invalid cursor model index {model_idx}"))?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index {proj_idx}"))?;
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        self.persist_changeset_model(&session_dir, model_id)?;
        self.spawn_telegram_cursor_cli(chat_id, session_id, &project.project_id)
            .await
    }

    async fn spawn_telegram_cursor_cli(
        &self,
        chat_id: i64,
        session_id: &str,
        project_id: &str,
    ) -> anyhow::Result<()> {
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?
            .clone();

        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let snap = read_changeset_routing_snapshot(&session_dir).unwrap_or_default();
        let model = snap.model.clone().unwrap_or_default();
        if model.trim().is_empty() {
            anyhow::bail!("model not set for cursor-cli session (model keyboard not completed)");
        }
        let initial_prompt = snap.initial_prompt.clone();

        let projects_dir = projects_dir_for_telegram_workflow_spawn(&deps)?;
        let project = project_storage::find_project(&projects_dir, project_id)?
            .ok_or_else(|| anyhow::anyhow!("project not found: {project_id}"))?;
        let repo_root = std::path::Path::new(&project.main_repo_path);
        if !repo_root.exists() {
            anyhow::bail!(
                "project main repo path does not exist: {}",
                repo_root.display()
            );
        }

        let cs = tddy_core::read_changeset(&session_dir)?;
        let new_branch_name = cs
            .workflow
            .as_ref()
            .and_then(|w| w.new_branch_name.as_deref())
            .unwrap_or("");
        let chain_base = tddy_core::resolve_chain_base_for_session_spawn(
            &self.sessions_base,
            None,
            repo_root,
            // The Telegram start names no planned node: it starts a session, never a pr-stack
            // node, so there is nothing for the node-keyed lookup to prefer.
            "",
            new_branch_name,
            cs.worktree_integration_base_ref.as_deref(),
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        let repo_root_owned = repo_root.to_path_buf();
        let session_dir_clone = session_dir.clone();
        let worktree_path: std::path::PathBuf = tokio::task::spawn_blocking(move || {
            tddy_core::setup_worktree_for_session_with_optional_chain_base(
                &repo_root_owned,
                &session_dir_clone,
                chain_base.as_deref(),
            )
            .map_err(|e| anyhow::anyhow!("worktree setup failed: {e}"))
        })
        .await
        .map_err(|e| anyhow::anyhow!("worktree join error: {e}"))?
        .map_err(|e| anyhow::anyhow!("worktree setup error: {e}"))?;

        let hook_token = tddy_session_lifecycle::cursor_cli_spawn::install_cursor_hooks_in_worktree(
            &deps.config,
            &worktree_path,
            session_id,
            &deps.os_user,
        );

        let binary_path = tddy_session_lifecycle::config::resolve_cursor_binary_path(&deps.config);
        // The Cursor chat this session owns for its whole lifetime: minted here, persisted in
        // `.session.yaml` below, and passed as `--resume <id>` on every later spawn so a resume
        // continues this chat instead of opening a new one.
        let cursor_chat_id = tddy_session_lifecycle::cursor_cli_spawn::mint_cursor_chat_id(
            &binary_path,
            &worktree_path,
        )
        .await
        .map_err(|e| {
            anyhow::anyhow!("failed to create the Cursor chat for session {session_id}: {e}")
        })?;
        let manager = Arc::clone(&deps.claude_cli_manager);
        let handle = manager
            .start_cursor(
                session_id,
                worktree_path.clone(),
                &model,
                &binary_path,
                Some(&cursor_chat_id),
                initial_prompt.as_deref(),
                Vec::new(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("failed to spawn cursor-cli: {e}"))?;

        let pid = handle.pid;
        let now = chrono::Utc::now().to_rfc3339();
        let meta = tddy_core::SessionMetadata {
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
            model: Some(model.clone()),
            cursor_chat_id: Some(cursor_chat_id),
            activity_status: None,
            hook_token: Some(hook_token),
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
            agent_daemon_instance_id: None,
            agent_session_id: None,
            ssh_config_host: None,
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| anyhow::anyhow!("write session metadata: {e}"))?;

        let sid_short = &session_id[..8.min(session_id.len())];
        let done = format!(
            "Cursor Agent CLI session started ({sid_short}…). Attach via the web UI or `tddy-tools pty-relay`."
        );
        self.sender.send_message(chat_id, &done).await?;
        self.notify_account_identity_unavailable(chat_id, session_id, &project)
            .await
    }
}

/// Whether `refused` is the exposure port's answer for "nothing is wired" — `FailedPrecondition`,
/// what [`tddy_daemon_extensions::NoTerminalExposure`] returns — as opposed to a real failure.
fn exposure_is_unwired(refused: &tddy_rpc::Status) -> bool {
    refused.code == tddy_rpc::Code::FailedPrecondition
}

#[cfg(test)]
mod exposure_refusal_tests {
    use super::exposure_is_unwired;
    use tddy_rpc::Status;

    #[test]
    fn the_null_exposures_refusal_is_not_a_failure() {
        let refused = Status::failed_precondition("no terminal exposure is wired");

        assert!(exposure_is_unwired(&refused));
    }

    #[test]
    fn a_failed_join_is_a_failure() {
        let refused = Status::internal("could not join the room");

        assert!(!exposure_is_unwired(&refused));
    }
}
