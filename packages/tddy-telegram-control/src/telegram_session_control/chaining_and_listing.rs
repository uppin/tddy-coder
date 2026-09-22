//! `/chain-workflow` and its parent picker, recipe and plan-review callbacks, and the session
//! list with its enter and delete actions.

use super::*;

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// `/chain-workflow`: first step is parent session selection, then the same project → base →
    /// agent flow as [`Self::handle_start_workflow`] (PRD: session chaining).
    pub async fn handle_chain_workflow(
        &mut self,
        cmd: ChainWorkflowCommand,
    ) -> anyhow::Result<StartWorkflowOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_chain_workflow: chat_id={} user_id={} prompt_len={}",
            cmd.chat_id,
            cmd.user_id,
            cmd.prompt.len()
        );
        self.ensure_authorized(cmd.chat_id)?;

        if let Some(ref map_path) = self.telegram_github_mapping_path {
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_chain_workflow: github link required; mapping_path={}",
                map_path.display()
            );
            let store = TelegramGithubMappingStore::open(map_path)?;
            if store.get_github_login(cmd.user_id).is_none() {
                log::info!(
                    target: "tddy_daemon::telegram_session_control",
                    "handle_chain_workflow: rejected — telegram user_id={} has no linked GitHub identity",
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
            "handle_chain_workflow: created session_dir={}",
            session_dir.display()
        );

        let parent_page =
            parent_candidates_page_for_chain_picker(&self.sessions_base, &session_id)?;

        let chain_intro = if parent_page.is_empty() {
            format!(
                "Chain workflow (stacked PR). Select a **parent** session to stack from — no prior sessions were found yet. New session prefix {}. Create a session with /start-workflow first, or pick below once sessions exist.",
                &session_id[..8.min(session_id.len())]
            )
        } else {
            format!(
                "Chain workflow (stacked PR). **First:** pick the **parent** session to stack onto (branch chain). New child session prefix {}.",
                &session_id[..8.min(session_id.len())]
            )
        };
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_chain_workflow: parent_picker candidates={}",
            parent_page.len()
        );

        let mut parent_kb: InlineKeyboardRows = Vec::new();
        for se in parent_page.iter() {
            let label = format!("Parent: {}", telegram_label_for_session_id(&se.session_id));
            let parent_tail = session_tail8(&se.session_id);
            let cb = format!("{CB_TELEGRAM_CHAIN_PARENT}p:{parent_tail}|s:{session_id}");
            debug_assert!(
                cb.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={cb:?}",
                cb.len()
            );
            parent_kb.push(vec![(label, cb)]);
        }

        self.sender
            .send_message_with_keyboard(cmd.chat_id, &chain_intro, parent_kb.clone())
            .await?;

        let mut messages = vec![CapturedTelegramMessage {
            chat_id: cmd.chat_id,
            text: chain_intro.clone(),
            inline_keyboard: parent_kb,
        }];

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

        messages.push(CapturedTelegramMessage {
            chat_id: cmd.chat_id,
            text: intro,
            inline_keyboard: keyboard,
        });

        Ok(StartWorkflowOutcome {
            session_id,
            messages,
        })
    }

    /// Handle `tcp:` parent picker callback: resolve the parent session and record
    /// [`tddy_core::SessionMetadata::previous_session_id`] on the child session directory.
    ///
    /// Wired from [`crate::telegram_bot`] during the session-chaining follow-up when the operator
    /// picks a parent row after [`Self::handle_chain_workflow`].
    pub async fn handle_chain_parent_callback(
        &mut self,
        child_session_dir: &Path,
        cb: TelegramCallback,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(cb.chat_id)?;
        let Some((parent_tail, child_id)) = parse_telegram_chain_parent_callback(&cb.callback_data)
        else {
            anyhow::bail!("not a chain parent callback (expected `tcp:p:` prefix)");
        };

        validate_session_id_segment(&child_id).map_err(|e| {
            anyhow::anyhow!("invalid child session id in tcp: callback: {}", e.message())
        })?;

        let parent_page = parent_candidates_page_for_chain_picker(&self.sessions_base, &child_id)?;

        let Some(parent_entry) = parent_page
            .iter()
            .find(|e| e.session_id.ends_with(parent_tail.as_str()))
        else {
            anyhow::bail!(
                "no parent candidate matching tail {parent_tail:?} found (page has {} candidate(s)); pick again",
                parent_page.len()
            );
        };
        let parent_id = parent_entry.session_id.as_str();

        let expected_child_dir = unified_session_dir_path(&self.sessions_base, &child_id);
        if child_session_dir != expected_child_dir.as_path() {
            anyhow::bail!(
                "child session directory mismatch for child_id={child_id}: expected {}",
                expected_child_dir.display()
            );
        }

        let mut meta = match read_session_metadata(child_session_dir) {
            Ok(m) => m,
            Err(_) => {
                write_initial_tool_session_metadata(
                    child_session_dir,
                    InitialToolSessionMetadataOpts {
                        project_id: "pending-project-selection".to_string(),
                        ..Default::default()
                    },
                )
                .map_err(|e| anyhow::anyhow!("{e}"))?;
                read_session_metadata(child_session_dir).map_err(|e| anyhow::anyhow!("{e}"))?
            }
        };

        meta.previous_session_id = Some(parent_id.to_string());
        meta.updated_at = Utc::now().to_rfc3339();
        write_session_metadata(child_session_dir, &meta).map_err(|e| anyhow::anyhow!("{e}"))?;

        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_chain_parent_callback: persisted previous_session_id for child {} -> parent {}",
            child_id,
            parent_id
        );

        Ok(())
    }

    /// After recipe selection: persist `changeset.yaml` and continue workflow.
    pub async fn handle_recipe_callback(
        &mut self,
        session_dir: &Path,
        cb: TelegramCallback,
    ) -> anyhow::Result<()> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_recipe_callback: chat_id={} session_dir={}",
            cb.chat_id,
            session_dir.display()
        );
        self.ensure_authorized(cb.chat_id)?;

        if let Some((idx, _sid)) = parse_recipe_mr_callback(&cb.callback_data) {
            let recipe_name = RECIPE_MORE_PAGE[idx];
            self.persist_recipe_to_changeset(session_dir, recipe_name, None)
                .await?;
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_recipe_callback: mr: idx={} recipe={}",
                idx,
                recipe_name
            );
            return Ok(());
        }

        let mut recipe: Option<String> = None;
        let mut demo_options: Option<serde_yaml::Value> = None;

        for segment in cb.callback_data.split('|') {
            if let Some(r) = segment.strip_prefix("recipe:") {
                recipe = Some(r.to_string());
            } else if let Some(rest) = segment.strip_prefix("demo_options:") {
                demo_options = Some(parse_demo_options_value(rest)?);
            }
        }

        if recipe.as_deref() == Some("more") {
            let Some(session_id) = parse_session_id_from_recipe_callback(&cb.callback_data) else {
                anyhow::bail!("recipe:more callback missing session: segment");
            };
            self.send_more_recipes_keyboard(cb.chat_id, &session_id)
                .await?;
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_recipe_callback: sent more recipes keyboard session_id={}",
                session_id
            );
            return Ok(());
        }

        let Some(recipe_name) = recipe else {
            anyhow::bail!("recipe callback missing recipe: segment");
        };

        self.persist_recipe_to_changeset(session_dir, &recipe_name, demo_options)
            .await?;
        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_recipe_callback: wrote changeset.yaml"
        );
        Ok(())
    }

    /// Deliver full plan text (chunked) and record approval callback handling.
    pub async fn handle_plan_review_phase(
        &mut self,
        _session_id: &str,
        golden_plan_text: &str,
        approval_callback: TelegramCallback,
    ) -> anyhow::Result<(Vec<String>, WorkflowTransitionKind)> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_plan_review_phase: chat_id={} plan_len={} approval_data_len={}",
            approval_callback.chat_id,
            golden_plan_text.len(),
            approval_callback.callback_data.len()
        );
        self.ensure_authorized(approval_callback.chat_id)?;

        // Chunk small enough to force continuation markers for typical plans (integration test).
        const CHUNK_MAX: usize = 24;
        let display_chunks = chunk_telegram_text(golden_plan_text, CHUNK_MAX);
        for chunk in &display_chunks {
            self.sender
                .send_message(approval_callback.chat_id, chunk)
                .await?;
        }

        let logical_chunks: Vec<String> = display_chunks
            .iter()
            .map(|c| {
                c.strip_suffix(TELEGRAM_CONTINUATION)
                    .unwrap_or(c.as_str())
                    .to_string()
            })
            .collect();

        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_plan_review_phase: sent_chunks={} transition=PlanReviewApproved",
            display_chunks.len()
        );

        // FIXME: parse approval_callback.callback_data to branch on approve/reject when wired to live teloxide path
        let _ = approval_callback;
        Ok((logical_chunks, WorkflowTransitionKind::PlanReviewApproved))
    }

    /// List sessions under `sessions_base`, paginated `SESSIONS_PAGE_SIZE` at a time.
    /// Sends session entries with inline keyboards ("Enter" + "Delete" per row) and a "More"
    /// button when additional pages exist.
    pub async fn handle_list_sessions(
        &self,
        chat_id: i64,
        offset: usize,
    ) -> anyhow::Result<SessionListPage> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_list_sessions: chat_id={} offset={}",
            chat_id,
            offset
        );
        self.ensure_authorized(chat_id)?;

        let mut sessions =
            tddy_session_lifecycle::session_reader::list_sessions_in_dir(&self.sessions_base)?;
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total = sessions.len();
        let page_slice: Vec<_> = sessions
            .into_iter()
            .skip(offset)
            .take(SESSIONS_PAGE_SIZE)
            .collect();
        let has_more = offset + page_slice.len() < total;
        let next_offset = offset + page_slice.len();

        let sessions_root = self.sessions_base.join(SESSIONS_SUBDIR);
        let mut entries: Vec<TelegramSessionEntry> = Vec::with_capacity(page_slice.len());

        if page_slice.is_empty() {
            self.sender
                .send_message(chat_id, "No sessions found.")
                .await?;
            return Ok(SessionListPage {
                entries,
                has_more: false,
                next_offset,
            });
        }

        for se in page_slice {
            let session_dir = sessions_root.join(&se.session_id);
            let enrich = session_list_status_or_placeholders(&session_dir);
            let label = telegram_label_for_session_id(&se.session_id);
            let entry = TelegramSessionEntry {
                session_id: se.session_id.clone(),
                label,
                status: se.status.clone(),
                workflow_state: enrich.workflow_state,
                elapsed_display: enrich.elapsed_display,
                is_active: se.is_active,
            };
            let text = format_session_list_entry(&entry);
            let enter_data = format!("{CB_ENTER}{}", se.session_id);
            let delete_data = format!("{CB_DELETE}{}", se.session_id);
            let keyboard: InlineKeyboardRows = vec![vec![
                ("Enter".to_string(), enter_data),
                ("Delete".to_string(), delete_data),
            ]];
            self.sender
                .send_message_with_keyboard(chat_id, &text, keyboard)
                .await?;
            entries.push(entry);
        }

        if has_more {
            let more_data = format!("{CB_MORE}{next_offset}");
            self.sender
                .send_message_with_keyboard(
                    chat_id,
                    "More sessions…",
                    vec![vec![("More".to_string(), more_data)]],
                )
                .await?;
        }

        Ok(SessionListPage {
            entries,
            has_more,
            next_offset,
        })
    }

    /// Delete a session by id. Delegates to `session_deletion::delete_session_directory`.
    /// Sends a confirmation message on success.
    pub async fn handle_delete_session(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<DeleteSessionOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_delete_session: chat_id={} session_id={}",
            chat_id,
            session_id
        );
        self.ensure_authorized(chat_id)?;

        // Before the directory goes, as `delete_session_directory` requires: the room's poll loop
        // measures the checkout on an interval, and this path removes the worktree with it.
        tddy_session_lifecycle::session_deletion::close_session_room(
            &self.session_rooms,
            session_id,
        );
        tddy_session_lifecycle::session_deletion::delete_session_directory(
            &self.sessions_base,
            session_id,
            None,
        )
        .map_err(|s| anyhow::anyhow!("{}", s.message))?;

        if let Ok(mut g) = self.telegram_tracked.lock() {
            if g.tracked_session_for_chat(chat_id).as_deref() == Some(session_id.trim()) {
                g.clear_telegram_tracked_session_for_chat(chat_id);
            }
        }

        let text = format!("Session {} deleted.", session_id);
        self.sender.send_message(chat_id, &text).await?;

        Ok(DeleteSessionOutcome {
            session_id: session_id.to_string(),
            confirmation_message: CapturedTelegramMessage {
                chat_id,
                text: text.clone(),
                inline_keyboard: Vec::new(),
            },
        })
    }

    /// Enter an existing session's workflow. Sends current workflow state and available actions.
    pub async fn handle_enter_session(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<EnterSessionOutcome> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_enter_session: chat_id={} session_id={}",
            chat_id,
            session_id
        );
        self.ensure_authorized(chat_id)?;

        let session_dir = self
            .sessions_base
            .join(SESSIONS_SUBDIR)
            .join(session_id.trim());
        let meta_path = session_dir.join(SESSION_METADATA_FILENAME);
        if !session_dir.is_dir() || !meta_path.is_file() {
            anyhow::bail!("session not found: {}", session_id);
        }

        let metadata = read_session_metadata(&session_dir).map_err(|e| anyhow::anyhow!("{e}"))?;
        if let Ok(mut g) = self.telegram_tracked.lock() {
            g.bind_chat_to_session_for_telegram_tracking(chat_id, session_id);
            let _replay_scheduled =
                g.notify_enter_session_elicitation_replay_skeleton(chat_id, session_id);
        }
        let enrich = session_list_status_or_placeholders(&session_dir);
        let label = telegram_label_for_session_id(session_id);
        let text = format!(
            "Session {} ({}): status {} · workflow {} · elapsed {}",
            session_id, label, metadata.status, enrich.workflow_state, enrich.elapsed_display
        );
        self.sender.send_message(chat_id, &text).await?;

        self.maybe_replay_elicitation_after_enter_session(chat_id, session_id)
            .await?;

        let captured = CapturedTelegramMessage {
            chat_id,
            text: text.clone(),
            inline_keyboard: Vec::new(),
        };

        Ok(EnterSessionOutcome {
            session_id: session_id.to_string(),
            messages: vec![captured],
        })
    }

    /// Unauthorized chat: no session creation and no outbound message.
    ///
    /// The inbound [`crate::telegram_bot`] path does not call this for disallowed chats (silent
    /// ignore for multi-daemon deployments). Kept for tests and any caller that needs the same
    /// contract without sending Telegram noise to unrelated channels.
    pub async fn handle_start_workflow_unauthorized(
        &self,
        cmd: StartWorkflowCommand,
    ) -> anyhow::Result<Option<CapturedTelegramMessage>> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow_unauthorized: chat_id={}",
            cmd.chat_id
        );
        if self.allowed_chat_ids.contains(&cmd.chat_id) {
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "handle_start_workflow_unauthorized: chat is authorized — caller should use handle_start_workflow"
            );
            return Ok(None);
        }

        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "handle_start_workflow_unauthorized: ignoring chat not in allowlist"
        );
        Ok(None)
    }
}
