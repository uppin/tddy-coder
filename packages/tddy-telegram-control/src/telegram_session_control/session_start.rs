//! `/start-workflow`, `/start-claude` and `/start-cursor`: session creation, the model-pick
//! callbacks, and the Claude and Cursor CLI spawns. (The model-pick keyboards are in `pickers`.)

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
}
