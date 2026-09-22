//! The project, branch-intent, branch, branch-conflict and agent pickers, ending in the
//! workflow spawn the agent pick triggers.

use super::*;

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// After recipe selection: ask whether to fork a new branch from the integration base or work on an existing branch.
    pub async fn send_intent_pick_keyboard(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let data_nb = format!("{CB_TELEGRAM_INTENT}nb|s:{session_id}");
        let data_ws = format!("{CB_TELEGRAM_INTENT}ws|s:{session_id}");
        debug_assert!(
            data_nb.len() <= 64 && data_ws.len() <= 64,
            "Telegram callback_data exceeds 64 bytes: nb_len={} ws_len={} session_id_len={}",
            data_nb.len(),
            data_ws.len(),
            session_id.len()
        );
        let rows: InlineKeyboardRows = vec![vec![
            ("New branch + worktree".to_string(), data_nb),
            ("Work on existing branch".to_string(), data_ws),
        ]];
        self.sender
            .send_message_with_keyboard(chat_id, "Choose branch/worktree intent:", rows)
            .await?;
        Ok(())
    }

    /// Persist [`BranchWorktreeIntent`] from Telegram and continue to project selection.
    pub async fn handle_telegram_intent_callback(
        &self,
        chat_id: i64,
        intent: BranchWorktreeIntent,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let mut cs = match read_changeset(&session_dir) {
            Ok(c) => c,
            Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
            Err(e) => anyhow::bail!("read changeset: {e}"),
        };
        cs.workflow
            .get_or_insert_with(Default::default)
            .branch_worktree_intent = Some(intent);
        write_changeset(&session_dir, &cs).map_err(|e| anyhow::anyhow!("write changeset: {e}"))?;
        self.send_project_pick_keyboard(chat_id, session_id).await
    }

    /// After branch/worktree intent is chosen: prompt for a project (then branch, then agent) like the web UI.
    pub async fn send_project_pick_keyboard(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            return Ok(());
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        if projects.is_empty() {
            self.sender
                .send_message(
                    chat_id,
                    "Recipe saved. No projects found for this user — add one via the web UI, then run /start-workflow again.",
                )
                .await?;
            return Ok(());
        }
        let mut rows: InlineKeyboardRows = Vec::new();
        for (i, p) in projects.iter().enumerate() {
            let label = format!("{} ({})", p.name, p.project_id);
            let data = format!("{CB_TELEGRAM_PROJECT}{i}|s:{session_id}");
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        self.sender
            .send_message_with_keyboard(chat_id, "Choose a project for this workflow:", rows)
            .await?;
        Ok(())
    }

    /// After project pick: show default integration base + recent `origin/*` branches (paginated).
    async fn send_branch_pick_keyboard(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
        project: &ProjectData,
        list_offset: usize,
    ) -> anyhow::Result<()> {
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects_dir = projects_dir_for_telegram_workflow_spawn(deps)?;
        let default_ref =
            effective_integration_base_ref_for_project(&projects_dir, &project.project_id)?;
        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            anyhow::bail!("project main repo path does not exist");
        }
        let remote =
            effective_remote_name_for_project(&projects_dir, &project.project_id, repo_path)?;
        let page_peek = match list_recent_remote_branches_skip(
            repo_path,
            &remote,
            list_offset,
            BRANCH_PAGE_SIZE + 1,
        ) {
            Ok(b) => b,
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::telegram_session_control",
                    "list_recent_remote_branches_skip: {}",
                    e
                );
                Vec::new()
            }
        };
        let has_more = page_peek.len() > BRANCH_PAGE_SIZE;
        let branches: Vec<String> = page_peek.into_iter().take(BRANCH_PAGE_SIZE).collect();
        let short_default = default_ref
            .strip_prefix("origin/")
            .unwrap_or(default_ref.as_str());
        let default_label = format!("Default ({short_default})");
        let mut rows: InlineKeyboardRows = Vec::new();
        let data0 = if list_offset == 0 {
            format!("{CB_TELEGRAM_BRANCH}0|p:{proj_idx}|s:{session_id}")
        } else {
            format!("{CB_TELEGRAM_BRANCH}0|o:{list_offset}|p:{proj_idx}|s:{session_id}")
        };
        debug_assert!(
            data0.len() <= 64,
            "Telegram callback_data exceeds 64 bytes: len={} data={data0:?}",
            data0.len()
        );
        rows.push(vec![(default_label, data0)]);
        for (i, br) in branches.iter().enumerate() {
            let idx = i + 1;
            let label = take_utf8_prefix(br, 52).to_string();
            let data = if list_offset == 0 {
                format!("{CB_TELEGRAM_BRANCH}{idx}|p:{proj_idx}|s:{session_id}")
            } else {
                format!("{CB_TELEGRAM_BRANCH}{idx}|o:{list_offset}|p:{proj_idx}|s:{session_id}")
            };
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        if has_more {
            let next_off = list_offset + branches.len();
            let more_data =
                format!("{CB_TELEGRAM_BRANCH_MORE}{next_off}|p:{proj_idx}|s:{session_id}");
            debug_assert!(
                more_data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={more_data:?}",
                more_data.len()
            );
            rows.push(vec![("More…".to_string(), more_data)]);
        }
        let intro = format!(
            "Project `{}` selected. Choose integration base (remote branch):",
            project.project_id
        );
        self.sender
            .send_message_with_keyboard(chat_id, &intro, rows)
            .await?;
        Ok(())
    }

    pub async fn handle_telegram_project_callback(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        self.send_branch_pick_keyboard(chat_id, proj_idx, session_id, project, 0)
            .await
    }

    /// Show another page of remote branches after **More…** (`tbm:…` callback).
    pub async fn handle_telegram_branch_more_callback(
        &self,
        chat_id: i64,
        next_list_offset: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        self.send_branch_pick_keyboard(chat_id, proj_idx, session_id, project, next_list_offset)
            .await
    }

    pub async fn handle_telegram_branch_callback(
        &self,
        chat_id: i64,
        branch_idx: usize,
        list_offset: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            anyhow::bail!("project main repo path does not exist");
        }
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let mut cs = match read_changeset(&session_dir) {
            Ok(c) => c,
            Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
            Err(e) => anyhow::bail!("read changeset: {e}"),
        };
        let intent = cs.workflow.as_ref().and_then(|w| w.branch_worktree_intent);
        let projects_dir = projects_dir_for_telegram_workflow_spawn(deps)?;
        let remote =
            effective_remote_name_for_project(&projects_dir, &project.project_id, repo_path)?;
        // Read routing snapshot before modifying cs — we need session_type to set new_branch_name
        // for claude-cli sessions (NewBranchFromBase requires new_branch_name to be set before
        // setup_worktree_for_session_with_optional_chain_base is called).
        let pre_snap = read_changeset_routing_snapshot(&session_dir).unwrap_or_default();
        let is_claude_cli = pre_snap.session_type.as_deref() == Some("claude-cli");
        let is_cursor_cli = pre_snap.session_type.as_deref() == Some("cursor-cli");
        // Compute the derived branch name now while cs is still immutable (before get_or_insert_with
        // takes a mutable borrow).
        let derived_claude_cli_branch = if is_claude_cli {
            claude_cli_branch_name_from_changeset(&cs)
        } else {
            None
        };
        let derived_cursor_cli_branch = if is_cursor_cli {
            Some(cursor_cli_branch_name_from_session_id(session_id))
        } else {
            None
        };
        if branch_idx == 0 {
            cs.worktree_integration_base_ref = None;
            if intent == Some(BranchWorktreeIntent::WorkOnSelectedBranch) {
                cs.workflow
                    .get_or_insert_with(Default::default)
                    .selected_branch_to_work_on = None;
            }
            if intent == Some(BranchWorktreeIntent::NewBranchFromBase) {
                let default_ref =
                    effective_integration_base_ref_for_project(&projects_dir, &project.project_id)?;
                let wf = cs.workflow.get_or_insert_with(Default::default);
                wf.selected_integration_base_ref = Some(default_ref);
                wf.selected_branch_to_work_on = None;
                // For claude-cli sessions, derive new_branch_name from cs.name so
                // validate_workflow_branch_intent / setup_worktree can proceed without waiting for
                // tddy-coder to produce a branch_suggestion.
                if wf.new_branch_name.is_none() {
                    wf.new_branch_name = derived_claude_cli_branch
                        .clone()
                        .or_else(|| derived_cursor_cli_branch.clone());
                }
            }
        } else {
            let global_idx = list_offset
                .checked_add(branch_idx)
                .and_then(|n| n.checked_sub(1))
                .ok_or_else(|| anyhow::anyhow!("invalid branch index"))?;
            let picked = list_recent_remote_branches_skip(repo_path, &remote, global_idx, 1)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let chain = picked
                .first()
                .ok_or_else(|| anyhow::anyhow!("invalid branch index (list may have changed)"))?;
            validate_chain_pr_integration_base_ref(chain).map_err(|e| anyhow::anyhow!(e))?;
            if intent == Some(BranchWorktreeIntent::WorkOnSelectedBranch) {
                cs.workflow
                    .get_or_insert_with(Default::default)
                    .selected_branch_to_work_on = Some(chain.clone());
            } else {
                cs.worktree_integration_base_ref = Some(chain.clone());
            }
            if intent == Some(BranchWorktreeIntent::NewBranchFromBase) {
                let wf = cs.workflow.get_or_insert_with(Default::default);
                wf.selected_integration_base_ref = Some(chain.clone());
                wf.selected_branch_to_work_on = None;
                // Same: derive new_branch_name for claude-cli when the user picked a base branch.
                if wf.new_branch_name.is_none() {
                    wf.new_branch_name = derived_claude_cli_branch
                        .clone()
                        .or_else(|| derived_cursor_cli_branch.clone());
                }
            }
        }
        // pre_snap was already captured above (before cs modifications).
        write_changeset(&session_dir, &cs).map_err(|e| anyhow::anyhow!("write changeset: {e}"))?;
        // Re-apply raw routing fields that write_changeset doesn't know about.
        if let Some(ref st) = pre_snap.session_type {
            self.persist_changeset_session_type_marker(&session_dir, st)?;
        }
        if let Some(ref m) = pre_snap.model {
            self.persist_changeset_model(&session_dir, m)?;
        }

        // Ask before the model picker when another session already owns the branch this pick just
        // derived, instead of silently creating `<branch>-1` at spawn time. Telegram never goes
        // through the `StartSession` guard, so the same check lives here. Only the claude-cli
        // derivation (`feature/<slug(name)>`) can collide — `cursor-cli/<short-id>` is derived from
        // the session uuid. Nothing is claimed here: no branch, no worktree.
        // Per `docs/ft/daemon/session-branch-conflict.md` § Telegram.
        if is_claude_cli && intent == Some(BranchWorktreeIntent::NewBranchFromBase) {
            let derived_branch = cs.workflow.as_ref().and_then(|w| w.new_branch_name.clone());
            if let Some(branch) = derived_branch {
                if let Some(owner) = self.session_owning_branch(&branch).await? {
                    return self
                        .send_branch_conflict_keyboard(
                            chat_id,
                            proj_idx,
                            session_id,
                            &branch,
                            &owner.session_id,
                            repo_path,
                        )
                        .await;
                }
            }
        }

        // Route to the claude-cli model picker when the session was started via /start-claude.
        if pre_snap.session_type.as_deref() == Some("claude-cli") {
            return self
                .send_claude_model_pick_keyboard(chat_id, proj_idx, session_id)
                .await;
        }
        if pre_snap.session_type.as_deref() == Some("cursor-cli") {
            return self
                .send_cursor_model_pick_keyboard(chat_id, proj_idx, session_id)
                .await;
        }

        let allowed = deps.config.allowed_agents();
        if allowed.is_empty() {
            self.spawn_telegram_workflow(chat_id, session_id, &project.project_id, None)
                .await?;
            return Ok(());
        }
        let mut rows: InlineKeyboardRows = Vec::new();
        for (i, a) in allowed.iter().enumerate() {
            let label = a
                .label
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| a.id.clone());
            let data = format!("{CB_TELEGRAM_AGENT}{i}|p:{proj_idx}|s:{session_id}");
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        let intro = format!(
            "Branch saved for `{}`. Choose an agent:",
            project.project_id
        );
        self.sender
            .send_message_with_keyboard(chat_id, &intro, rows)
            .await?;
        Ok(())
    }

    /// Send the Claude model pick keyboard (`tcm:` callbacks).
    async fn send_claude_model_pick_keyboard(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        let mut rows: InlineKeyboardRows = Vec::new();
        for (i, model) in CLAUDE_CLI_MODELS.iter().enumerate() {
            let data = format!("{CB_TELEGRAM_CLAUDE_MODEL}{i}|p:{proj_idx}|s:{session_id}");
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(model.label.clone(), data)]);
        }
        self.sender
            .send_message_with_keyboard(chat_id, "Choose a model for the Claude CLI session:", rows)
            .await?;
        Ok(())
    }

    /// The session that already owns `branch`, by the one rule every surface shares
    /// ([`tddy_session_lifecycle::branch_owner::find_session_owning_branch`]). The scan reads
    /// changesets from disk, so it runs off the reactor.
    async fn session_owning_branch(
        &self,
        branch: &str,
    ) -> anyhow::Result<Option<tddy_session_lifecycle::branch_owner::SessionClaim>> {
        let sessions_base = self.sessions_base.clone();
        let branch = branch.to_string();
        tokio::task::spawn_blocking(move || {
            tddy_session_lifecycle::branch_owner::find_session_owning_branch(
                &tddy_session_lifecycle::session_reader::DaemonSessionListing,
                &sessions_base,
                &branch,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("branch owner scan join: {e}"))?
    }

    /// Offer the three branch-conflict choices instead of the model picker: switch to the owning
    /// session, add a second agent to the owned branch, or take the suggested suffixed name.
    ///
    /// Claims nothing — the pending session keeps no branch and no worktree while the operator
    /// decides. Per `docs/ft/daemon/session-branch-conflict.md` § Telegram.
    async fn send_branch_conflict_keyboard(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
        branch: &str,
        owner_session_id: &str,
        repo_root: &Path,
    ) -> anyhow::Result<()> {
        let suggested = first_free_suffixed_branch_name_off_reactor(repo_root, branch).await?;
        let owner_label = telegram_label_for_session_id(owner_session_id);
        let mut rows: InlineKeyboardRows = Vec::new();
        for (label, choice) in [
            (
                format!("Switch to {owner_label}"),
                TelegramBranchConflictChoice::SwitchToOwner,
            ),
            (
                format!("New agent on {branch}"),
                TelegramBranchConflictChoice::NewAgentOnOwnedBranch,
            ),
            (
                format!("Use {suggested}"),
                TelegramBranchConflictChoice::UseSuggestedName,
            ),
        ] {
            let data = format!(
                "{CB_TELEGRAM_BRANCH_CONFLICT}{}:{proj_idx}:{session_id}",
                choice.callback_code()
            );
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        let intro = format!(
            "Branch `{branch}` is already used by session {owner_label}. Choose how to continue:"
        );
        self.sender
            .send_message_with_keyboard(chat_id, &intro, rows)
            .await?;
        Ok(())
    }

    /// Apply the operator's branch-conflict choice (`tbc:<choice>:<proj_idx>:<session_id>`).
    ///
    /// The contested branch is re-derived from the pending session's changeset, because it cannot
    /// fit in a 64-byte `callback_data`. Per `docs/ft/daemon/session-branch-conflict.md` § Telegram.
    pub async fn handle_telegram_branch_conflict_callback(
        &self,
        chat_id: i64,
        choice: TelegramBranchConflictChoice,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_telegram_branch_conflict_callback: chat_id={} choice={:?} proj_idx={} session_id={}",
            chat_id,
            choice,
            proj_idx,
            session_id
        );
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            anyhow::bail!("project main repo path does not exist");
        }
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let mut cs =
            read_changeset(&session_dir).map_err(|e| anyhow::anyhow!("read changeset: {e}"))?;
        let owned_branch = cs
            .workflow
            .as_ref()
            .and_then(|w| w.new_branch_name.clone())
            .ok_or_else(|| anyhow::anyhow!("no branch name on the pending session"))?;

        match choice {
            TelegramBranchConflictChoice::SwitchToOwner => {
                // No spawn: the pending session is left half-configured, exactly as abandoning any
                // picker mid-flow does today.
                let owner = self
                    .session_owning_branch(&owned_branch)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("no session owns branch {owned_branch}"))?;
                self.handle_enter_session(chat_id, &owner.session_id)
                    .await?;
                return Ok(());
            }
            TelegramBranchConflictChoice::NewAgentOnOwnedBranch => {
                let wf = cs.workflow.get_or_insert_with(Default::default);
                wf.branch_worktree_intent = Some(BranchWorktreeIntent::WorkOnSelectedBranch);
                wf.selected_branch_to_work_on = Some(owned_branch);
                wf.new_branch_name = None;
            }
            TelegramBranchConflictChoice::UseSuggestedName => {
                let suggested =
                    first_free_suffixed_branch_name_off_reactor(repo_path, &owned_branch).await?;
                let wf = cs.workflow.get_or_insert_with(Default::default);
                wf.branch_worktree_intent = Some(BranchWorktreeIntent::NewBranchFromBase);
                wf.new_branch_name = Some(suggested);
            }
        }
        // `write_changeset` does not know the raw routing markers, so they are re-read first and
        // re-applied after — same as the branch callback.
        let pre_snap = read_changeset_routing_snapshot(&session_dir).unwrap_or_default();
        write_changeset(&session_dir, &cs).map_err(|e| anyhow::anyhow!("write changeset: {e}"))?;
        if let Some(ref st) = pre_snap.session_type {
            self.persist_changeset_session_type_marker(&session_dir, st)?;
        }
        if let Some(ref m) = pre_snap.model {
            self.persist_changeset_model(&session_dir, m)?;
        }

        self.send_claude_model_pick_keyboard(chat_id, proj_idx, session_id)
            .await
    }

    async fn send_cursor_model_pick_keyboard(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        let mut rows: InlineKeyboardRows = Vec::new();
        for (i, model) in CURSOR_CLI_MODELS.iter().enumerate() {
            let data = format!("{CB_TELEGRAM_CURSOR_MODEL}{i}|p:{proj_idx}|s:{session_id}");
            rows.push(vec![(model.label.clone(), data)]);
        }
        self.sender
            .send_message_with_keyboard(chat_id, "Choose a model for the Cursor CLI session:", rows)
            .await?;
        Ok(())
    }

    pub async fn handle_telegram_agent_callback(
        &self,
        chat_id: i64,
        agent_idx: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index"))?;
        let allowed = deps.config.allowed_agents();
        let agent = allowed
            .get(agent_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid agent index"))?;
        self.spawn_telegram_workflow(
            chat_id,
            session_id,
            &project.project_id,
            Some(agent.id.as_str()),
        )
        .await
    }

    async fn spawn_telegram_workflow(
        &self,
        chat_id: i64,
        session_id: &str,
        project_id: &str,
        agent: Option<&str>,
    ) -> anyhow::Result<()> {
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?
            .clone();
        let presenter_hooks = deps.telegram_hooks.clone();
        let child_grpc_registry = deps.child_grpc_by_session.clone();
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let recipe = read_recipe_from_changeset(&session_dir)?;
        let projects_dir = projects_dir_for_telegram_workflow_spawn(&deps)?;
        let project = project_storage::find_project(&projects_dir, project_id)?
            .ok_or_else(|| anyhow::anyhow!("project not found"))?;
        let repo_path = Path::new(&project.main_repo_path);

        if let Ok(meta) = read_session_metadata(&session_dir) {
            if let Some(ref parent_id) = meta.previous_session_id {
                let stored_explicit = read_changeset(&session_dir)
                    .ok()
                    .and_then(|cs| cs.worktree_integration_base_ref);
                let explicit_owned: Option<String> = stored_explicit
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                log::info!(
                    target: "tddy_daemon::telegram_session_control",
                    "spawn_telegram_workflow: chain child parent_session_id={} explicit_worktree_integration_base_ref={:?}",
                    parent_id,
                    explicit_owned.as_deref()
                );
                let sessions_base = self.sessions_base.clone();
                let parent_id = parent_id.clone();
                let session_dir = session_dir.clone();
                let repo_path = repo_path.to_path_buf();
                tokio::task::spawn_blocking(move || {
                    merge_chain_integration_base_with_explicit_operator_overrides(
                        &sessions_base,
                        parent_id.as_str(),
                        &session_dir,
                        &repo_path,
                        explicit_owned.as_deref(),
                    )
                })
                .await
                .map_err(|e| anyhow::anyhow!("chain merge spawn_blocking join: {e}"))??;
            }
        }

        let result = deps
            .spawn_session(project_id, agent, recipe.as_deref(), session_id)
            .await?;
        {
            let mut g = child_grpc_registry
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            g.insert(result.session_id.clone(), result.grpc_port);
        }
        if let Some(ref hooks) = presenter_hooks {
            // TODO(session-notifications): publish this session's presenter events onto the
            // notification bus too, so a Telegram-started workflow session raises the same drawer
            // indicator a web-started one does. It needs the bus (and the sessions base its label
            // is read from) on `TelegramWorkflowSpawn`, which every inbound-control harness
            // constructs by hand; adding it is a change of its own. The Telegram surface for these
            // sessions is unaffected — it never went through the bus.
            tddy_session_lifecycle::presenter_observer_task::spawn_presenter_observer_task(
                Some(Arc::clone(hooks) as SharedPresenterEventSink),
                None,
                &result.session_id,
                result.grpc_port,
            );
        }
        let sid_short = {
            let s = result.session_id.as_str();
            &s[..8.min(s.len())]
        };
        let done = format!(
            "Workflow started (session {sid_short}…). gRPC {} LiveKit room `{}`.",
            result.grpc_port, result.livekit_room
        );
        self.sender.send_message(chat_id, &done).await?;
        Ok(())
    }
}
