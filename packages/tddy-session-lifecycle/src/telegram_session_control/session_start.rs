//! `/start-workflow`, `/start-claude` and `/start-cursor`: session creation, the model pickers,
//! and the Claude and Cursor CLI spawns.

use super::*;

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// Authorized chat: create session, emit recipe keyboard.
    pub async fn handle_start_workflow(
        &mut self,
        cmd: StartWorkflowCommand,
    ) -> anyhow::Result<StartWorkflowOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow: chat_id={} user_id={} prompt_len={}",
            cmd.chat_id,
            cmd.user_id,
            cmd.prompt.len()
        );
        self.ensure_authorized(cmd.chat_id)?;

        if let Some(ref map_path) = self.telegram_github_mapping_path {
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_start_workflow: github link required; mapping_path={}",
                map_path.display()
            );
            let store = TelegramGithubMappingStore::open(map_path)?;
            if store.get_github_login(cmd.user_id).is_none() {
                log::info!(
                    target: "tddy_daemon::telegram_session_control",
                    "handle_start_workflow: rejected start-workflow — telegram user_id={} has no linked GitHub identity",
                    cmd.user_id
                );
                anyhow::bail!(
                    "Telegram account is not linked to GitHub. Use the bot's /link-github flow (or web OAuth) to connect your GitHub identity before starting a workflow."
                );
            }
        }

        let session_id = Uuid::new_v4().to_string();
        let session_dir = unified_session_dir_path(&self.sessions_base, &session_id);
        std::fs::create_dir_all(&session_dir)?;
        self.persist_initial_prompt_to_changeset(&session_dir, &cmd.prompt)
            .await?;
        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow: created session_dir={}",
            session_dir.display()
        );

        if let Ok(mut g) = self.telegram_tracked.lock() {
            g.bind_chat_to_session_for_telegram_tracking(cmd.chat_id, &session_id);
        }

        let intro = format!(
            "Workflow started (session {}). Choose a recipe to continue.",
            &session_id[..8.min(session_id.len())]
        );
        let keyboard: InlineKeyboardRows = vec![vec![
            (
                format!("Recipe: {TELEGRAM_DEFAULT_RECIPE_CLI}"),
                format!("recipe:{TELEGRAM_DEFAULT_RECIPE_CLI}|session:{session_id}"),
            ),
            (
                "More recipes…".to_string(),
                format!("recipe:more|session:{session_id}"),
            ),
        ]];
        self.sender
            .send_message_with_keyboard(cmd.chat_id, &intro, keyboard.clone())
            .await?;

        let messages = vec![CapturedTelegramMessage {
            chat_id: cmd.chat_id,
            text: intro,
            inline_keyboard: keyboard,
        }];

        Ok(StartWorkflowOutcome {
            session_id,
            messages,
        })
    }

    /// `/start-claude <prompt>`: start a Claude Code CLI session seeded with `prompt`.
    ///
    /// Skips the recipe keyboard (recipes are tddy-coder-specific). Writes `session_type: claude-cli`
    /// and `initial_prompt` into `changeset.yaml`, then immediately shows the **project** keyboard.
    /// Subsequent project → branch → model callbacks complete the flow and call
    /// [`Self::spawn_telegram_claude_cli`].
    pub async fn handle_start_claude(
        &mut self,
        cmd: StartClaudeCommand,
    ) -> anyhow::Result<StartClaudeOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_claude: chat_id={} user_id={} prompt_len={}",
            cmd.chat_id,
            cmd.user_id,
            cmd.prompt.len()
        );
        self.ensure_authorized(cmd.chat_id)?;

        if let Some(ref map_path) = self.telegram_github_mapping_path {
            let store = TelegramGithubMappingStore::open(map_path)?;
            if store.get_github_login(cmd.user_id).is_none() {
                anyhow::bail!(
                    "Telegram account is not linked to GitHub. Use the bot's /link-github flow (or web OAuth) to connect your GitHub identity before starting a session."
                );
            }
        }

        let session_id = Uuid::new_v4().to_string();
        let session_dir = unified_session_dir_path(&self.sessions_base, &session_id);
        std::fs::create_dir_all(&session_dir)?;

        // Write initial_prompt, derived name, and branch intent into the changeset.
        // - name: needed so setup_worktree_for_session_with_optional_chain_base can derive a
        //   branch name (feature/<slug>) when NewBranchFromBase intent is selected.
        // - branch_worktree_intent: NewBranchFromBase is the sensible default for /start-claude
        //   since we are always starting fresh; the branch callback reads and preserves this.
        {
            let trimmed = cmd.prompt.trim().to_string();
            // Always set name — needed by claude_cli_branch_name_from_changeset to derive
            // wf.new_branch_name (required by validate_workflow_branch_intent).
            // Fall back to the short session id when the user sent /start-claude with no text.
            let name = if !trimmed.is_empty() {
                trimmed
                    .split_whitespace()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                format!("claude-{}", &session_id[..8.min(session_id.len())])
            };
            let initial_prompt = if !trimmed.is_empty() {
                Some(trimmed)
            } else {
                None
            };
            let workflow = Some(tddy_core::ChangesetWorkflow {
                branch_worktree_intent: Some(BranchWorktreeIntent::NewBranchFromBase),
                ..Default::default()
            });
            let cs = Changeset {
                name: Some(name),
                initial_prompt,
                workflow,
                ..Default::default()
            };
            write_changeset(&session_dir, &cs)
                .map_err(|e| anyhow::anyhow!("write changeset: {e}"))?;
        }

        // Write the claude-cli marker so the branch callback routes to claude-cli spawn.
        self.persist_changeset_session_type_marker(&session_dir, "claude-cli")?;

        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_claude: created session_dir={}",
            session_dir.display()
        );

        if let Ok(mut g) = self.telegram_tracked.lock() {
            g.bind_chat_to_session_for_telegram_tracking(cmd.chat_id, &session_id);
        }

        // Send the project pick keyboard — the existing project/branch callbacks are reused.
        self.send_project_pick_keyboard(cmd.chat_id, &session_id)
            .await?;

        // messages is empty here — the project keyboard was sent via self.sender.
        // In tests, callers hold an Arc<InMemoryTelegramSender> and use collect_outbound_messages
        // externally to inspect what was sent; the outcome only carries the session_id.
        Ok(StartClaudeOutcome {
            session_id,
            messages: vec![],
        })
    }

    /// `/start-cursor <prompt>`: start a Cursor Agent CLI session seeded with `prompt`.
    pub async fn handle_start_cursor(
        &mut self,
        cmd: StartCursorCommand,
    ) -> anyhow::Result<StartCursorOutcome> {
        self.ensure_authorized(cmd.chat_id)?;

        if let Some(ref map_path) = self.telegram_github_mapping_path {
            let store = TelegramGithubMappingStore::open(map_path)?;
            if store.get_github_login(cmd.user_id).is_none() {
                anyhow::bail!(
                    "Telegram account is not linked to GitHub. Use the bot's /link-github flow (or web OAuth) to connect your GitHub identity before starting a session."
                );
            }
        }

        let session_id = Uuid::new_v4().to_string();
        let session_dir = unified_session_dir_path(&self.sessions_base, &session_id);
        std::fs::create_dir_all(&session_dir)?;

        {
            let trimmed = cmd.prompt.trim().to_string();
            let name = if !trimmed.is_empty() {
                trimmed
                    .split_whitespace()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                format!("cursor-{}", &session_id[..8.min(session_id.len())])
            };
            let initial_prompt = if !trimmed.is_empty() {
                Some(trimmed)
            } else {
                None
            };
            let workflow = Some(tddy_core::ChangesetWorkflow {
                branch_worktree_intent: Some(BranchWorktreeIntent::NewBranchFromBase),
                ..Default::default()
            });
            let cs = Changeset {
                name: Some(name),
                initial_prompt,
                workflow,
                ..Default::default()
            };
            write_changeset(&session_dir, &cs)
                .map_err(|e| anyhow::anyhow!("write changeset: {e}"))?;
        }

        self.persist_changeset_session_type_marker(&session_dir, "cursor-cli")?;

        if let Ok(mut g) = self.telegram_tracked.lock() {
            g.bind_chat_to_session_for_telegram_tracking(cmd.chat_id, &session_id);
        }

        self.send_project_pick_keyboard(cmd.chat_id, &session_id)
            .await?;

        Ok(StartCursorOutcome {
            session_id,
            messages: vec![],
        })
    }

    /// Write `session_type: <value>` into `changeset.yaml` (raw-YAML merge, preserves other fields).
    pub(super) fn persist_changeset_session_type_marker(
        &self,
        session_dir: &Path,
        session_type: &str,
    ) -> anyhow::Result<()> {
        let path = session_dir.join("changeset.yaml");
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        let mut root: serde_yaml::Value = if raw.trim().is_empty() {
            serde_yaml::Mapping::new().into()
        } else {
            serde_yaml::from_str(&raw)?
        };
        let map = root
            .as_mapping_mut()
            .ok_or_else(|| anyhow::anyhow!("changeset.yaml root must be a mapping"))?;
        map.insert(
            serde_yaml::Value::String("session_type".into()),
            serde_yaml::Value::String(session_type.to_string()),
        );
        let out = serde_yaml::to_string(&root)?;
        std::fs::write(&path, out)?;
        Ok(())
    }

    /// Write `model: <value>` into `changeset.yaml` (raw-YAML merge, preserves other fields).
    pub(super) fn persist_changeset_model(
        &self,
        session_dir: &Path,
        model: &str,
    ) -> anyhow::Result<()> {
        let path = session_dir.join("changeset.yaml");
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        let mut root: serde_yaml::Value = if raw.trim().is_empty() {
            serde_yaml::Mapping::new().into()
        } else {
            serde_yaml::from_str(&raw)?
        };
        let map = root
            .as_mapping_mut()
            .ok_or_else(|| anyhow::anyhow!("changeset.yaml root must be a mapping"))?;
        map.insert(
            serde_yaml::Value::String("model".into()),
            serde_yaml::Value::String(model.to_string()),
        );
        let out = serde_yaml::to_string(&root)?;
        std::fs::write(&path, out)?;
        Ok(())
    }

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
    /// [`crate::cli_session_manager::CliSessionManager`], writes `.session.yaml`, and replies
    /// with the session id and attach instructions.
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
            match crate::cli_session_manager::spawn_livekit_bridge(
                Arc::clone(&handle),
                &lk.url,
                &room_name,
                &lk.api_key,
                &lk.api_secret,
                &server_identity,
                // A Telegram-started session is part of no stack, so its block carries the identity
                // and static fields only — published all the same, so the drawer names it the same
                // way it names every other session (D37).
                Some(
                    tddy_core::session_participant_metadata::SessionParticipantMetadata {
                        agent: "claude".to_string(),
                        model: model.clone(),
                        repo_path: worktree_path.to_string_lossy().to_string(),
                        session_id: session_id.to_string(),
                        ..Default::default()
                    },
                ),
            )
            .await
            {
                // The task running the participant is dropped rather than kept: a Telegram-started
                // session's bridge is announced to a human in the reply below and never asked
                // about again from here.
                Ok(_serving) => (room_name, lk.url.clone(), server_identity),
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::telegram_session_control",
                        "spawn_telegram_claude_cli: LiveKit bridge failed ({e}); gRPC path still works"
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
        Ok(())
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

        let hook_token = crate::cursor_cli_spawn::install_cursor_hooks_in_worktree(
            &deps.config,
            &worktree_path,
            session_id,
            &deps.os_user,
        );

        let binary_path = crate::config::resolve_cursor_binary_path(&deps.config);
        // The Cursor chat this session owns for its whole lifetime: minted here, persisted in
        // `.session.yaml` below, and passed as `--resume <id>` on every later spawn so a resume
        // continues this chat instead of opening a new one.
        let cursor_chat_id =
            crate::cursor_cli_spawn::mint_cursor_chat_id(&binary_path, &worktree_path)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "failed to create the Cursor chat for session {session_id}: {e}"
                    )
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
        Ok(())
    }
}
