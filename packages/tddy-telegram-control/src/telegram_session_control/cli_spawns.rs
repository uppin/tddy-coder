//! The model-pick callback of a CLI start, and the generic start it ends in.

use tddy_daemon_extensions::{OperatorBranchChoice, OperatorSessionStart, OperatorStartOutcome};

use crate::telegram_session_control::*;

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// Handle the model pick of a pending CLI start (`tcm:<model_idx>|p:<proj_idx>|s:<session_id>`)
    /// and start it: the pending session's agent (its session type) and the model at `model_idx` of
    /// that agent's spec go to the daemon's generic start through the session starter, with the
    /// branch the pending session chose and the agent's spec-required branch name. The started
    /// session replaces the pending one; a branch another session holds is answered with the
    /// conflict keyboard, and nothing is started.
    pub async fn handle_telegram_model_pick_callback(
        &self,
        chat_id: i64,
        model_idx: usize,
        proj_idx: usize,
        session_id: &str,
    ) -> anyhow::Result<()> {
        self.ensure_authorized(chat_id)?;
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let spec = self.pending_session_agent(&session_dir)?;
        let model = telegram_model_at(spec, model_idx)?;
        self.start_pending_cli_session(chat_id, proj_idx, session_id, model.model_id)
            .await
    }

    /// Start the pending session `session_id` as `model` (empty: the agent takes none) through the
    /// daemon's generic start.
    ///
    /// On a start, the pending directory is removed — the generic start created the session under
    /// its own id — and the chat's tracking moves to the started session. On a refusal or a branch
    /// conflict the pending session stays, so the operator can choose again.
    pub(super) async fn start_pending_cli_session(
        &self,
        chat_id: i64,
        proj_idx: usize,
        session_id: &str,
        model: String,
    ) -> anyhow::Result<()> {
        let Some(ref deps) = self.workflow_spawn else {
            anyhow::bail!("Telegram workflow spawn is not configured");
        };
        let projects = sorted_projects_for_workflow_spawn(deps)?;
        let project = projects
            .get(proj_idx)
            .ok_or_else(|| anyhow::anyhow!("invalid project index {proj_idx}"))?;
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let session_type = self.pending_session_agent(&session_dir).map(|spec| {
            format!(
                "{}{}",
                spec.id,
                tddy_coding_agent_specs::CLI_SESSION_TYPE_SUFFIX
            )
        })?;
        let cs =
            read_changeset(&session_dir).map_err(|e| anyhow::anyhow!("read changeset: {e}"))?;
        let workflow = cs.workflow.clone().unwrap_or_default();
        let branch = match workflow.branch_worktree_intent {
            Some(BranchWorktreeIntent::WorkOnSelectedBranch) => {
                OperatorBranchChoice::WorkOnBranch {
                    branch: workflow.selected_branch_to_work_on.clone().ok_or_else(|| {
                        anyhow::anyhow!("no branch was chosen to work on for the pending session")
                    })?,
                }
            }
            _ => OperatorBranchChoice::NewBranchFromBase {
                new_branch_name: workflow.new_branch_name.clone().unwrap_or_default(),
                integration_base_ref: workflow
                    .selected_integration_base_ref
                    .clone()
                    .unwrap_or_default(),
            },
        };
        let start = OperatorSessionStart {
            os_user: deps.os_user.clone(),
            project_id: project.project_id.clone(),
            session_type,
            model,
            initial_prompt: cs.initial_prompt.clone().unwrap_or_default(),
            branch,
        };

        // The daemon bounds the start itself (`session_start_timeout_secs`): an overrun comes back
        // as its DEADLINE_EXCEEDED refusal, relayed here like any other.
        let outcome = self
            .session_starter
            .start_for_operator(start)
            .await
            .map_err(|refused| anyhow::anyhow!("{}", refused.message))?;

        match outcome {
            OperatorStartOutcome::BranchConflict(conflict) => {
                self.send_branch_conflict_keyboard(
                    chat_id,
                    proj_idx,
                    session_id,
                    &conflict.branch,
                    &conflict.owner_session_id,
                    &conflict.suggested_branch_name,
                )
                .await
            }
            OperatorStartOutcome::Started {
                session_id: started_id,
            } => {
                self.hand_over_to_started_session(chat_id, session_id, &started_id, project)
                    .await
            }
        }
    }

    /// The pending session was started as `started_id`: drop the draft, follow the started session
    /// in the chat and tell the operator where to attach.
    async fn hand_over_to_started_session(
        &self,
        chat_id: i64,
        pending_id: &str,
        started_id: &str,
        project: &ProjectData,
    ) -> anyhow::Result<()> {
        if started_id != pending_id {
            let pending_dir = unified_session_dir_path(&self.sessions_base, pending_id);
            if let Err(e) = std::fs::remove_dir_all(&pending_dir) {
                log::warn!(
                    target: "tddy_daemon::telegram_session_control",
                    "could not remove the draft session {pending_id} after starting {started_id}: {e}"
                );
            }
            if let Ok(mut tracked) = self.telegram_tracked.lock() {
                tracked.bind_chat_to_session_for_telegram_tracking(chat_id, started_id);
            }
        }
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "start_pending_cli_session: started session {started_id} (draft {pending_id}) project={}",
            project.project_id
        );

        let started_dir = unified_session_dir_path(&self.sessions_base, started_id);
        let livekit_room = read_session_metadata(&started_dir)
            .ok()
            .and_then(|meta| meta.livekit_room)
            .filter(|room| !room.is_empty());
        let sid_short = &started_id[..8.min(started_id.len())];
        let attach_hint = match livekit_room {
            Some(room) => {
                format!("LiveKit room `{room}`. Attach via the web UI or `tddy-tools pty-relay`.")
            }
            None => "Attach via the web UI or `tddy-tools pty-relay`.".to_string(),
        };
        self.sender
            .send_message(
                chat_id,
                &format!("CLI session started ({sid_short}…). {attach_hint}"),
            )
            .await?;
        self.notify_account_identity_unavailable(chat_id, started_id, project)
            .await
    }
}
