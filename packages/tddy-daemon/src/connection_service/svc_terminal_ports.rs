//! What this daemon hands `tddy-terminal-rpc` so that crate can serve
//! `terminal_session.TerminalSessionService` itself.
//!
//! The nine terminal methods are `tddy-terminal-rpc`'s; what stays here is the four answers only a
//! daemon has — which terminal a `(session_id, terminal_id)` names, who holds the control lease,
//! how a login shell becomes a task, and which identity a session token belongs to. Each is a port
//! implemented once here rather than a branch repeated per handler, which is what the four terminal
//! RPCs in [`super::rpc_service`] used to carry.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::Status;
use tddy_terminal_rpc::service::{
    ControlChange, ControlClaim, TerminalControl, TerminalDescriptor, TerminalRoster,
    TerminalSessionPorts,
};
use tokio::sync::broadcast;

use super::ConnectionServiceImpl;
use crate::cli_session_manager::CliSessionManager;
use crate::terminal_session_adapter::DaemonTerminalSessionStore;

impl ConnectionServiceImpl {
    /// Every terminal this daemon serves, behind one
    /// [`TerminalSessionStore`](tddy_terminal_rpc::session::TerminalSessionStore).
    ///
    /// Composite over both owners so a *sandboxed* session's PTY is an ordinary terminal to the
    /// bridge. Before `#unbundle` node 6 each terminal RPC branched on the sandbox registry itself,
    /// and `StreamTerminalOutput`'s branch carried its own transcription of the bridge's
    /// replay/offset loop — two implementations of the same offsets, in a surface the repo has
    /// already been bitten twice by disagreeing about.
    pub(crate) fn terminal_store(&self) -> DaemonTerminalSessionStore {
        DaemonTerminalSessionStore::new(
            Arc::clone(&self.claude_cli_manager),
            Arc::clone(&self.sandbox_manager),
        )
    }

    /// The `terminal_session.TerminalSessionService` entry this daemon registers.
    ///
    /// Built from the *same* managers `connection.ConnectionService` serves its terminal RPCs from,
    /// so the two coordinates address one set of PTYs and one control lease while both are
    /// mounted — a second `CliSessionManager` here would mean a terminal started on one coordinate
    /// was invisible on the other.
    pub(crate) fn terminal_session_entry(&self) -> tddy_rpc::ServiceEntry {
        let config = self.config.clone();
        tddy_terminal_rpc::build_terminal_session_entry(TerminalSessionPorts {
            github_users: self.user_resolver.clone(),
            os_users: Arc::new(move |github_user: &str| {
                config.os_user_for_github(github_user).map(str::to_owned)
            }),
            terminals: Arc::new(self.terminal_store()),
            control: Arc::new(CliManagerTerminalControl::new(Arc::clone(
                &self.claude_cli_manager,
            ))),
            roster: Arc::new(CliManagerTerminalRoster::new(Arc::clone(
                &self.claude_cli_manager,
            ))),
            initial_frame_bytes: tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
        })
    }
}

/// The control lease, held by [`CliSessionManager`].
///
/// The lease is the manager's rather than this adapter's because a session's *deletion* clears it,
/// and deletion runs nowhere near an RPC handler.
pub struct CliManagerTerminalControl {
    manager: Arc<CliSessionManager>,
}

impl CliManagerTerminalControl {
    pub fn new(manager: Arc<CliSessionManager>) -> Self {
        CliManagerTerminalControl { manager }
    }
}

#[async_trait]
impl TerminalControl for CliManagerTerminalControl {
    async fn claim(&self, session_id: &str, screen_id: &str, steal: bool) -> ControlClaim {
        self.manager
            .claim_control(session_id, screen_id, steal)
            .await
    }

    async fn verify(&self, session_id: &str, control_token: &str) -> bool {
        self.manager.verify_control(session_id, control_token).await
    }

    async fn holder_screen_id(&self, session_id: &str) -> Option<String> {
        self.manager
            .current_control(session_id)
            .await
            .map(|lease| lease.holder_screen_id)
    }

    fn subscribe(&self) -> broadcast::Receiver<ControlChange> {
        self.manager.subscribe_control()
    }
}

/// The terminals of a session, spawned as tasks by [`CliSessionManager`].
pub struct CliManagerTerminalRoster {
    manager: Arc<CliSessionManager>,
}

impl CliManagerTerminalRoster {
    pub fn new(manager: Arc<CliSessionManager>) -> Self {
        CliManagerTerminalRoster { manager }
    }
}

#[async_trait]
impl TerminalRoster for CliManagerTerminalRoster {
    /// A started shell runs in the same worktree the session's agent does, which is why the main
    /// terminal has to exist first: it is where that path is recorded.
    async fn start(&self, session_id: &str, shell_path: &str) -> Result<String, Status> {
        let main = self
            .manager
            .get(session_id)
            .await
            .ok_or_else(|| Status::failed_precondition("session has no running terminal"))?;
        let worktree = main.worktree_path.clone();
        let handle = self
            .manager
            .start_terminal(session_id, worktree, shell_path)
            .await
            .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))?;
        Ok(handle.terminal_id.clone())
    }

    async fn stop(&self, session_id: &str, terminal_id: &str) -> bool {
        self.manager.stop_terminal(session_id, terminal_id).await
    }

    async fn list(&self, session_id: &str) -> Vec<TerminalDescriptor> {
        self.manager
            .list_terminals(session_id)
            .await
            .iter()
            .map(|handle| TerminalDescriptor {
                terminal_id: handle.terminal_id.clone(),
                kind: handle.kind.clone(),
                pid: handle.pid,
            })
            .collect()
    }
}
