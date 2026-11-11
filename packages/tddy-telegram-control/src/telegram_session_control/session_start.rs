//! `/start-workflow`, `/start-claude`, `/start-cursor` and `/start-agent`: session creation up to
//! the agent, project, branch and model picks. (The keyboards are in `pickers`; the model-pick
//! callback and the generic start it ends in are in `cli_spawns`.)

use super::*;

/// The agent `/start-claude` picks.
const CLAUDE_AGENT_ID: &str = "claude";
/// The agent `/start-cursor` picks.
const CURSOR_AGENT_ID: &str = "cursor";

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

    /// `/start-claude <prompt>`: start a Claude Code CLI session seeded with `prompt` — a shortcut
    /// for `/start-agent` that picks the `claude` agent itself.
    ///
    /// Skips the recipe keyboard (recipes are tddy-coder-specific). Writes the agent's session type
    /// and `initial_prompt` into `changeset.yaml`, then immediately shows the **project** keyboard.
    /// The project → branch → model callbacks complete the flow and end in the daemon's generic
    /// start ([`Self::handle_telegram_model_pick_callback`]).
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
        let session_id = self
            .begin_pending_cli_start(cmd.chat_id, cmd.user_id, &cmd.prompt, Some(CLAUDE_AGENT_ID))
            .await?;
        // `messages` is empty: the keyboard went out through `self.sender`; tests read it with
        // `collect_outbound_messages`.
        Ok(StartClaudeOutcome {
            session_id,
            messages: vec![],
        })
    }

    /// `/start-agent <prompt>`: start any CLI agent the host's coding-agent registry offers.
    ///
    /// Writes the pending session's changeset (`initial_prompt`, a name from the prompt, a
    /// new-branch intent) and sends the **agent** keyboard ([`TelegramAgentChoices`]); the agent
    /// pick then continues as `/start-claude` does, from the project keyboard. A host without
    /// coding agents is refused, and nothing is created.
    pub async fn handle_start_agent(
        &mut self,
        cmd: StartAgentCommand,
    ) -> anyhow::Result<StartAgentOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_agent: chat_id={} user_id={} prompt_len={}",
            cmd.chat_id,
            cmd.user_id,
            cmd.prompt.len()
        );
        let session_id = self
            .begin_pending_cli_start(cmd.chat_id, cmd.user_id, &cmd.prompt, None)
            .await?;
        Ok(StartAgentOutcome {
            session_id,
            messages: vec![],
        })
    }

    /// `/start-cursor <prompt>`: start a Cursor Agent CLI session seeded with `prompt` — a shortcut
    /// for `/start-agent` that picks the `cursor` agent itself.
    pub async fn handle_start_cursor(
        &mut self,
        cmd: StartCursorCommand,
    ) -> anyhow::Result<StartCursorOutcome> {
        let session_id = self
            .begin_pending_cli_start(cmd.chat_id, cmd.user_id, &cmd.prompt, Some(CURSOR_AGENT_ID))
            .await?;
        Ok(StartCursorOutcome {
            session_id,
            messages: vec![],
        })
    }

    /// Create the pending (draft) session of a CLI start: its changeset carries the prompt, a name
    /// derived from it and a new-branch intent. With `shortcut` (an agent id) the agent is already
    /// picked and the **project** keyboard follows; without, the **agent** keyboard does.
    ///
    /// Refused before anything is created when the chat is not authorized, its Telegram account is
    /// not linked to GitHub, the host has no coding agents, or the shortcut's agent is not a CLI
    /// agent of the host.
    async fn begin_pending_cli_start(
        &self,
        chat_id: i64,
        user_id: u64,
        prompt: &str,
        shortcut: Option<&str>,
    ) -> anyhow::Result<String> {
        self.ensure_authorized(chat_id)?;

        if let Some(ref map_path) = self.telegram_github_mapping_path {
            let store = TelegramGithubMappingStore::open(map_path)?;
            if store.get_github_login(user_id).is_none() {
                anyhow::bail!(
                    "Telegram account is not linked to GitHub. Use the bot's /link-github flow (or web OAuth) to connect your GitHub identity before starting a session."
                );
            }
        }

        let offered = TelegramAgentChoices::of(&self.coding_agents);
        if offered.choices().is_empty() {
            return Err(TelegramPickRefusal::NoCodingAgents {
                session_type: shortcut.unwrap_or("CLI agent").to_string(),
            }
            .into());
        }
        // The shortcut names an agent; the session type is the one its spec declares, and it must
        // be one of the offered agents'.
        if let Some(agent_id) = shortcut {
            let offered_session_type = offered
                .choices()
                .iter()
                .any(|choice| choice.agent_id == agent_id);
            if !offered_session_type {
                return Err(TelegramPickRefusal::UnknownSessionType {
                    session_type: agent_id.to_string(),
                }
                .into());
            }
        }

        let session_type = shortcut.and_then(|agent_id| {
            offered
                .choices()
                .iter()
                .find(|choice| choice.agent_id == agent_id)
                .map(|choice| choice.session_type.clone())
        });

        let session_id = Uuid::new_v4().to_string();
        let session_dir = unified_session_dir_path(&self.sessions_base, &session_id);
        std::fs::create_dir_all(&session_dir)?;

        // Write initial_prompt, a derived name and the branch intent into the changeset.
        // - name: the first six words of the prompt; the branch an agent that requires one names is
        //   derived from it when the branch is picked.
        // - branch_worktree_intent: NewBranchFromBase is the sensible default, since a Telegram
        //   start is always fresh; the branch callback reads and preserves it.
        {
            let trimmed = prompt.trim().to_string();
            let name = if !trimmed.is_empty() {
                trimmed
                    .split_whitespace()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                format!(
                    "{}-{}",
                    shortcut.unwrap_or("agent"),
                    &session_id[..8.min(session_id.len())]
                )
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
        if let Some(session_type) = &session_type {
            self.persist_changeset_session_type_marker(&session_dir, session_type)?;
        }

        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "begin_pending_cli_start: created session_dir={}",
            session_dir.display()
        );

        if let Ok(mut g) = self.telegram_tracked.lock() {
            g.bind_chat_to_session_for_telegram_tracking(chat_id, &session_id);
        }

        match session_type {
            Some(_) => {
                self.send_project_pick_keyboard(chat_id, &session_id)
                    .await?
            }
            None => self.send_agent_pick_keyboard(chat_id, &session_id).await?,
        }
        Ok(session_id)
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
}
