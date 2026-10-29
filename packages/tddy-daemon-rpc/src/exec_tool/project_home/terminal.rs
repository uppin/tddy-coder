use crate::exec_tool::project_home::DaemonProjectHome;
use async_trait::async_trait;
use tddy_task::TextGrid;
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tddy_terminal_rpc::session::TerminalSessionStore;
use tddy_terminal_rpc::ControlClaim;
use tddy_tool_engine::project_home_terminal_tools::ProjectHomeTerminalPort;
use tddy_worktree_service::session_target::SessionTarget;

#[async_trait]
impl ProjectHomeTerminalPort for DaemonProjectHome {
    /// The screen of `target`'s main terminal, read under its capture's lock: the active buffer's
    /// rows and the cursor at one moment. The store is the one the terminal RPCs resolve through,
    /// sandbox first. A session with no live terminal, or whose terminal has not reported its size,
    /// is refused with the reason (`#home-inbox` 2/8).
    async fn read_terminal(&self, target: &SessionTarget) -> anyhow::Result<TextGrid> {
        let session = self
            .terminals
            .get_terminal(&target.session_id, MAIN_TERMINAL_ID)
            .await
            .ok_or_else(|| anyhow::anyhow!("session {} has no live terminal", target.session_id))?;
        let capture = session.capture();
        let grid = capture
            .lock()
            .map_err(|_| {
                anyhow::anyhow!(
                    "session {}'s terminal capture is poisoned",
                    target.session_id
                )
            })?
            .text_grid();
        grid.ok_or_else(|| {
            anyhow::anyhow!(
                "session {}'s terminal size is not known yet",
                target.session_id
            )
        })
    }

    /// Take `target`'s control lease for `screen_id` the way a browser's claim does, stealing it, and
    /// remember the issued token under this caller and the target. The token is kept here and never
    /// returned (`#home-inbox` 5/8).
    async fn claim_terminal(&self, target: &SessionTarget, screen_id: &str) -> Result<(), String> {
        match self
            .terminal_control
            .claim(&target.session_id, screen_id, true)
            .await
        {
            ControlClaim::Granted { control_token } => {
                self.leases
                    .remember(self.claim_key(&target.session_id), control_token);
                Ok(())
            }
            ControlClaim::Denied { holder_screen_id } => Err(format!(
                "control of session {} is held by `{holder_screen_id}`, and was not taken",
                target.session_id
            )),
        }
    }

    /// Write `bytes` to `target`'s main terminal, provided this caller's remembered token is still the
    /// session's lease and `screen_id` holds it. The lease is verified here explicitly: a jailed
    /// terminal does not check it on input, so this is the only check it gets. Input is written with
    /// no input offset, which the terminal reads as unset and does not acknowledge (`#home-inbox` 5/8).
    async fn send_keys(
        &self,
        target: &SessionTarget,
        screen_id: &str,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let session_id = target.session_id.as_str();
        let holder = self.terminal_control.holder_screen_id(session_id).await;
        let token = match self.leases.token_of(&self.claim_key(session_id)) {
            Some(token) if holder.as_deref() == Some(screen_id) => token,
            _ => return Err(control_refusal(session_id, holder.as_deref())),
        };
        if !self.terminal_control.verify(session_id, &token).await {
            let holder = self.terminal_control.holder_screen_id(session_id).await;
            return Err(control_refusal(session_id, holder.as_deref()));
        }
        let terminal = self
            .terminals
            .get_terminal(session_id, MAIN_TERMINAL_ID)
            .await
            .ok_or_else(|| format!("session {session_id} has no live terminal"))?;
        terminal.send_input(bytes.into(), 0);
        Ok(())
    }
}

/// Why a send to `session_id` is refused for want of control: another screen holds it, naming that
/// screen, or nothing does. Either way the agent claims it again, which is the one way back.
fn control_refusal(session_id: &str, holder: Option<&str>) -> String {
    match holder {
        Some(holder) => format!(
            "control of session {session_id} is held by `{holder}`; claim it again with ClaimSessionTerminal, then send"
        ),
        None => format!(
            "control of session {session_id} is not held; claim it with ClaimSessionTerminal first"
        ),
    }
}
