//! `SendSessionMessage` and `ResumeSession` answered on this host (`#orchestrators` 4/11): a turn is
//! delivered by the `turn_input` the target's agent spec declares, without changing who holds its
//! terminal's control lease, and a resume is the web's Resume plus an optional first message.

use std::time::Duration;

use crate::exec_tool::project_home::DaemonProjectHome;
use async_trait::async_trait;
use tddy_coding_agent_specs::TurnInputSlot;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_service::proto::session::ResumeSessionRequest;
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tddy_terminal_rpc::session::TerminalSessionStore;
use tddy_tool_engine::project_home_turn_tools::{ProjectHomeTurnPort, ResumeOutcome, TurnDelivery};
use tddy_tool_engine::terminal_keys::encode_keys;
use tddy_worktree_service::session_target::SessionTarget;

/// How long a resumed agent's terminal is given to draw its first screen before its first message
/// is given up on.
const RESUMED_TERMINAL_READY_TIMEOUT: Duration = Duration::from_secs(60);
const RESUMED_TERMINAL_POLL: Duration = Duration::from_millis(50);

#[async_trait]
impl ProjectHomeTurnPort for DaemonProjectHome {
    /// Refuse a session that cannot take a turn, then deliver `text` by the `turn_input` its agent's
    /// spec declares: no slot (naming the agent), a stopped session (naming `ResumeSession`) and a
    /// pending question or permission (naming the answer tool) are each refused with nothing typed.
    async fn send_session_message(
        &self,
        target: &SessionTarget,
        text: &str,
    ) -> Result<TurnDelivery, String> {
        let slot = self.turn_input_of(target)?;
        if !target.is_active {
            return Err(format!(
                "session {} is stopped: resume it with ResumeSession (optionally with this message)",
                target.session_id
            ));
        }
        self.refuse_while_waiting_on_an_answer(target)?;
        match slot {
            TurnInputSlot::TerminalPaste { submit_key } => {
                self.paste_and_submit(&target.session_id, text, &submit_key)
                    .await?;
                Ok(TurnDelivery::TerminalPaste)
            }
            // TODO(acp-turn-input): no ACP agent declares `acp_prompt` yet, so the daemon holds no
            // route to a session's ACP connection; the delivery is covered by the tool-engine unit
            // test with a fake port only (changeset decision, 2026-11-12).
            TurnInputSlot::AcpPrompt => Err(format!(
                "session {}'s agent declares turn_input acp_prompt, which this host cannot deliver yet",
                target.session_id
            )),
        }
    }

    /// Resume `target` as the web's Resume does, then give `message`, if any, to the resumed agent as
    /// its first turn once its terminal has drawn. A running session is left alone.
    async fn resume_session(
        &self,
        target: &SessionTarget,
        message: Option<&str>,
    ) -> Result<ResumeOutcome, String> {
        if target.is_active {
            return Ok(ResumeOutcome::Running);
        }
        // Refused before anything is resumed: a message the agent cannot take must not leave the
        // session resumed with the message dropped.
        let slot = match message {
            Some(_) => Some(self.turn_input_of(target)?),
            None => None,
        };
        self.launch
            .resume_session_at_session_coordinate(tddy_rpc::Request::direct(ResumeSessionRequest {
                session_token: self.session_token.clone(),
                session_id: target.session_id.clone(),
            }))
            .await
            .map_err(|status| format!("cannot resume: {}", status.message()))?;
        if let (Some(message), Some(slot)) = (message, slot) {
            match slot {
                TurnInputSlot::TerminalPaste { submit_key } => {
                    self.until_the_terminal_has_drawn(&target.session_id).await?;
                    self.paste_and_submit(&target.session_id, message, &submit_key)
                        .await
                        .map_err(|e| format!("resumed, but the message was not delivered: {e}"))?;
                }
                TurnInputSlot::AcpPrompt => {
                    return Err(format!(
                        "resumed, but session {}'s agent declares turn_input acp_prompt, which this host cannot deliver yet",
                        target.session_id
                    ))
                }
            }
        }
        Ok(ResumeOutcome::Resumed)
    }
}

impl DaemonProjectHome {
    /// The `turn_input` the target's agent declares, or a refusal naming the agent.
    fn turn_input_of(&self, target: &SessionTarget) -> Result<TurnInputSlot, String> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &target.session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir)
            .map_err(|e| format!("cannot read session {}'s record: {e}", target.session_id))?;
        let session_type = metadata.session_type.as_deref().unwrap_or_default();
        let agent = self
            .launch
            .coding_agents
            .agents()
            .for_session_type(session_type)
            .ok_or_else(|| {
                format!(
                    "session {} is of type {session_type:?}, which no coding agent declares, so it takes no message (no turn_input)",
                    target.session_id
                )
            })?;
        agent.turn_input().cloned().ok_or_else(|| {
            format!(
                "agent {} declares no turn_input, so a session of it takes no message",
                agent.id
            )
        })
    }

    /// Refuse while the session's agent waits on a permission prompt or a question, naming the tool
    /// that answers it.
    fn refuse_while_waiting_on_an_answer(&self, target: &SessionTarget) -> Result<(), String> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &target.session_id);
        let pending =
            tddy_core::pending_question::read_pending_question(&session_dir).map_err(|e| {
                format!(
                    "cannot read session {}'s pending question: {e}",
                    target.session_id
                )
            })?;
        match pending {
            None => Ok(()),
            Some(pending) => {
                let answer_tool = if pending.kind == "permission" {
                    "AnswerSessionPermission"
                } else {
                    "AnswerSessionQuestion"
                };
                Err(format!(
                    "session {} is waiting on {} {:?}: answer it with {answer_tool} instead",
                    target.session_id, pending.kind, pending.request_id
                ))
            }
        }
    }

    /// Type `text` into the session's main terminal — as a bracketed paste when the terminal turned
    /// it on — then `submit_key`, in one write. The control lease is neither read nor changed:
    /// the daemon is the writer, and the terminal's holder keeps it.
    async fn paste_and_submit(
        &self,
        session_id: &str,
        text: &str,
        submit_key: &str,
    ) -> Result<(), String> {
        let terminal = self
            .terminals
            .get_terminal(session_id, MAIN_TERMINAL_ID)
            .await
            .ok_or_else(|| format!("session {session_id} has no live terminal"))?;
        let bracketed_paste = terminal
            .capture()
            .lock()
            .map_err(|_| format!("session {session_id}'s terminal capture is poisoned"))?
            .text_grid()
            .ok_or_else(|| format!("session {session_id}'s terminal size is not known yet"))?
            .bracketed_paste;
        let bytes = encode_keys(Some(text), &[submit_key.to_string()], bracketed_paste)?;
        terminal.send_input(bytes.into(), 0);
        Ok(())
    }

    /// Wait for a resumed session's main terminal to exist and show something: the agent has drawn.
    async fn until_the_terminal_has_drawn(&self, session_id: &str) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + RESUMED_TERMINAL_READY_TIMEOUT;
        loop {
            if let Some(terminal) = self
                .terminals
                .get_terminal(session_id, MAIN_TERMINAL_ID)
                .await
            {
                let drawn = terminal
                    .capture()
                    .lock()
                    .ok()
                    .and_then(|capture| capture.text_grid())
                    .is_some_and(|grid| grid.lines.iter().any(|line| !line.trim().is_empty()));
                if drawn {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "resumed, but session {session_id}'s terminal showed nothing within {}s, so the message was not delivered",
                    RESUMED_TERMINAL_READY_TIMEOUT.as_secs()
                ));
            }
            tokio::time::sleep(RESUMED_TERMINAL_POLL).await;
        }
    }
}
