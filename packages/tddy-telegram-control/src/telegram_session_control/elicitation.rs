//! Presenter input from Telegram: feature submission, document review, elicitation answers,
//! and the changeset writes and recipe keyboard around them.

use super::*;

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// Deliver feature text to the running child `tddy-coder` via [`PresenterIntent`] (see [`SUBMIT_FEATURE_CMD`]).
    pub async fn handle_submit_feature(
        &self,
        chat_id: i64,
        session_key: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        presenter_intent_client::submit_feature_text_localhost(port, text).await?;
        let short = &session_id[..8.min(session_id.len())];
        self.sender
            .send_message(
                chat_id,
                &format!("Feature text submitted for session {short}…"),
            )
            .await?;
        Ok(())
    }

    /// Forward document-review actions to the child presenter ([`PresenterIntent`]).
    pub async fn handle_document_review_action(
        &self,
        chat_id: i64,
        action: char,
        session_key: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (_session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        match action {
            'a' => presenter_intent_client::approve_session_document_localhost(port).await,
            'r' => presenter_intent_client::refine_session_document_localhost(port).await,
            'v' => presenter_intent_client::view_session_document_localhost(port).await,
            'd' => presenter_intent_client::dismiss_viewer_localhost(port).await,
            'j' => presenter_intent_client::reject_session_document_localhost(port).await,
            _ => anyhow::bail!("unknown document action {action:?}"),
        }?;
        Ok(())
    }

    /// Single-select clarification answer ([`PresenterIntent::AnswerClarificationSelect`]).
    pub async fn handle_elicitation_select(
        &self,
        chat_id: i64,
        session_key: &str,
        option_index: usize,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        if let Ok(mut g) = deps.pending_elicitation_other.lock() {
            g.remove(&chat_id);
        }
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        presenter_intent_client::answer_clarification_select_localhost(port, option_index as u32)
            .await?;
        let confirmation = {
            let guard = deps
                .elicitation_select_options
                .lock()
                .map_err(|e| anyhow::anyhow!("elicitation options cache lock: {e}"))?;
            guard
                .get(&session_id)
                .and_then(|v| v.get(option_index))
                .cloned()
        };
        let text = match confirmation {
            Some(full) => format!("You selected:\n{full}"),
            None => format!("You selected option {}.", option_index + 1),
        };
        self.sender.send_message(chat_id, &text).await?;
        Ok(())
    }

    /// User tapped **Other** on a single-select clarification keyboard — next plain message is the answer.
    pub async fn handle_elicitation_other(
        &self,
        chat_id: i64,
        session_key: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (_session_id, _port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        deps.pending_elicitation_other
            .lock()
            .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?
            .insert(chat_id, session_key.to_string());
        self.sender
            .send_message(
                chat_id,
                "Send your custom answer as your next message in this chat.",
            )
            .await?;
        Ok(())
    }

    /// If this chat is awaiting an "Other" free-text answer, consume `body` and forward to the presenter.
    ///
    /// Returns `Ok(true)` when the message was handled (including presenter errors surfaced to the user).
    pub async fn handle_elicitation_other_followup_plain_message(
        &self,
        chat_id: i64,
        body: &str,
    ) -> anyhow::Result<bool> {
        if !self.is_authorized(chat_id) {
            return Ok(false);
        }
        let Some(ref deps) = self.workflow_spawn else {
            return Ok(false);
        };
        let session_key = {
            let g = deps
                .pending_elicitation_other
                .lock()
                .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?;
            match g.get(&chat_id).cloned() {
                Some(s) => s,
                None => return Ok(false),
            }
        };
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, &session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That follow-up does not match the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }
        let trimmed = body.trim();
        if trimmed.is_empty() {
            self.sender
                .send_message(
                    chat_id,
                    "That message was empty — send your custom answer as text, or pick an option on the question.",
                )
                .await?;
            return Ok(true);
        }
        {
            let mut g = deps
                .pending_elicitation_other
                .lock()
                .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?;
            g.remove(&chat_id);
        }
        if let Err(e) =
            presenter_intent_client::answer_clarification_text_localhost(port, trimmed).await
        {
            deps.pending_elicitation_other
                .lock()
                .map_err(|e| anyhow::anyhow!("pending elicitation other lock: {e}"))?
                .insert(chat_id, session_key);
            return Err(e);
        }
        let text = format!("You selected:\n{trimmed}");
        self.sender.send_message(chat_id, &text).await?;
        Ok(true)
    }

    /// Free-text clarification ([`PresenterIntent::AnswerClarificationText`]).
    pub async fn handle_answer_text_command(
        &self,
        chat_id: i64,
        session_key: &str,
        text: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        if let Ok(mut g) = deps.pending_elicitation_other.lock() {
            g.remove(&chat_id);
        }
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That session is not the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }
        presenter_intent_client::answer_clarification_text_localhost(port, text).await?;
        Ok(())
    }

    /// Multi-select clarification ([`PresenterIntent::AnswerClarificationMultiSelect`]).
    pub async fn handle_answer_multi_command(
        &self,
        chat_id: i64,
        session_key: &str,
        indices: &[usize],
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That session is not the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }
        let u32s: Vec<u32> = indices.iter().map(|&i| i as u32).collect();
        presenter_intent_client::answer_clarification_multi_select_localhost(
            port,
            u32s,
            String::new(),
        )
        .await?;
        Ok(())
    }

    /// Inline **Choose none** / **Choose recommended** (`eli:mn:` / `eli:mr:`).
    pub async fn handle_elicitation_multi_select_shortcut(
        &self,
        chat_id: i64,
        session_key: &str,
        question_index: i32,
        kind: ElicitationMultiSelectShortcutKind,
    ) -> anyhow::Result<()> {
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "handle_elicitation_multi_select_shortcut: chat_id={} kind={:?} question_index={}",
            chat_id,
            kind,
            question_index,
        );
        self.ensure_authorized(chat_id)?;
        let deps = self
            .workflow_spawn
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("workflow spawn not configured"))?;
        if let Ok(mut g) = deps.pending_elicitation_other.lock() {
            g.remove(&chat_id);
        }

        let (session_id, port) = {
            let map = deps
                .child_grpc_by_session
                .lock()
                .map_err(|e| anyhow::anyhow!("grpc registry lock: {e}"))?;
            resolve_child_grpc_port(&map, session_key)?
        };
        if !self.elicitation_callback_permitted(chat_id, &session_id) {
            anyhow::bail!(
                "That session is not the active elicitation for this chat. Finish the current prompt or use the web UI."
            );
        }

        match kind {
            ElicitationMultiSelectShortcutKind::ChooseNone => {
                log::debug!(
                    target: "tddy_daemon::telegram_session_control",
                    "shortcut ChooseNone → presenter empty indices session_id={}",
                    session_id,
                );
                presenter_intent_client::answer_clarification_multi_select_localhost(
                    port,
                    Vec::new(),
                    String::new(),
                )
                .await?;
            }
            ElicitationMultiSelectShortcutKind::ChooseRecommended => {
                let meta = {
                    let guard = deps
                        .elicitation_multi_select_meta
                        .lock()
                        .map_err(|e| anyhow::anyhow!("elicitation_multi_select_meta lock: {e}"))?;
                    guard.get(&session_id).cloned()
                };

                let Some(meta) = meta else {
                    anyhow::bail!(
                        "No shortcut metadata for this session — use /answer-multi or the web UI."
                    );
                };
                if meta.question_index != question_index {
                    log::warn!(
                        target: "tddy_daemon::telegram_session_control",
                        "shortcut question_index mismatch cached={} callback={}",
                        meta.question_index,
                        question_index
                    );
                    anyhow::bail!("That recommendation shortcut is stale for this question.");
                }

                let trimmed = meta.recommended_other.trim();
                if trimmed.is_empty() {
                    anyhow::bail!("No recommended answer is configured for this step.");
                }

                log::debug!(
                    target: "tddy_daemon::telegram_session_control",
                    "shortcut ChooseRecommended → presenter Other len {}",
                    trimmed.len(),
                );

                presenter_intent_client::answer_clarification_multi_select_localhost(
                    port,
                    Vec::new(),
                    trimmed.to_string(),
                )
                .await?;
            }
        }

        let confirm = match kind {
            ElicitationMultiSelectShortcutKind::ChooseNone => {
                "You submitted an empty multi-select (Choose none)."
            }
            ElicitationMultiSelectShortcutKind::ChooseRecommended => {
                "You submitted the recommended answer."
            }
        };
        self.sender.send_message(chat_id, confirm).await?;
        Ok(())
    }

    /// Store `/start-workflow` text as [`Changeset::initial_prompt`] so `tddy-coder` does not block on
    /// [`WorkflowEvent::AwaitingFeatureInput`] when only Telegram drove session creation (no web prompt).
    pub(super) async fn persist_initial_prompt_to_changeset(
        &self,
        session_dir: &Path,
        prompt: &str,
    ) -> anyhow::Result<()> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let mut cs = match read_changeset(session_dir) {
            Ok(c) => c,
            Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
            Err(e) => anyhow::bail!("read changeset: {e}"),
        };
        cs.initial_prompt = Some(trimmed.to_string());
        write_changeset(session_dir, &cs).map_err(|e| anyhow::anyhow!("write changeset: {e}"))
    }

    pub(super) async fn persist_recipe_to_changeset(
        &self,
        session_dir: &Path,
        recipe_name: &str,
        demo_options: Option<serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        if recipe_name.is_empty() {
            anyhow::bail!("empty recipe name");
        }
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

        let stored = normalize_recipe_name_for_tddy_coder_cli(recipe_name);
        map.insert(
            serde_yaml::Value::String("recipe".into()),
            serde_yaml::Value::String(stored),
        );
        if let Some(d) = demo_options {
            map.insert(serde_yaml::Value::String("demo_options".into()), d);
        }

        let out = serde_yaml::to_string(&root)?;
        std::fs::write(&path, out)?;
        Ok(())
    }

    /// Second page of recipe buttons after **More recipes…** (`recipe:more|session:…`).
    pub(super) async fn send_more_recipes_keyboard(
        &self,
        chat_id: i64,
        session_id: &str,
    ) -> anyhow::Result<()> {
        let mut rows: InlineKeyboardRows = Vec::with_capacity(RECIPE_MORE_PAGE.len());
        for (i, name) in RECIPE_MORE_PAGE.iter().enumerate() {
            let label = format!("Recipe: {name}");
            let data = format!("mr:{i}|{session_id}");
            debug_assert!(
                data.len() <= 64,
                "Telegram callback_data exceeds 64 bytes: len={} data={data:?}",
                data.len()
            );
            rows.push(vec![(label, data)]);
        }
        self.sender
            .send_message_with_keyboard(chat_id, "More recipes — choose one:", rows)
            .await?;
        Ok(())
    }
}
