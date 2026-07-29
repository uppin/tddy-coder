//! Adapters that expose the daemon's claude-cli [`PtyHandle`] as a [`tddy_terminal_rpc::session::TerminalSession`]
//! and a [`CliSessionManager`] as a [`TerminalSessionStore`], so the daemon's terminal RPCs can delegate
//! to the unified streaming bridge in `tddy-terminal-rpc`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, watch};

use crate::cli_session_manager::{CliSessionManager, PtyHandle};

/// A live claude-cli terminal exposed to the unified bridge.
pub struct DaemonTerminalSession {
    handle: Arc<PtyHandle>,
}

impl DaemonTerminalSession {
    pub fn new(handle: Arc<PtyHandle>) -> Self {
        DaemonTerminalSession { handle }
    }

    /// The underlying handle, for daemon handlers that need direct access (e.g. control-token checks).
    pub fn handle(&self) -> &Arc<PtyHandle> {
        &self.handle
    }
}

#[async_trait]
impl tddy_terminal_rpc::session::TerminalSession for DaemonTerminalSession {
    fn capture(&self) -> Arc<Mutex<TerminalCapture>> {
        Arc::clone(&self.handle.capture)
    }

    fn subscribe_stdout(&self) -> broadcast::Receiver<Bytes> {
        self.handle.stdout_tx.subscribe()
    }

    fn subscribe_pty_done(&self) -> watch::Receiver<bool> {
        self.handle.pty_done.clone()
    }

    fn subscribe_acked_offset(&self) -> watch::Receiver<u64> {
        self.handle.subscribe_acked_offset()
    }

    async fn resize(&self, rows: u16, cols: u16) {
        self.handle.resize(rows, cols);
    }

    fn send_input(&self, data: Bytes, input_offset: u64) {
        self.handle.send_input(data, input_offset);
    }

    fn trigger_redraw(&self) {
        self.handle.trigger_redraw();
    }
}

/// A [`TerminalSessionStore`] backed by the daemon's [`CliSessionManager`]. Auth (session-token
/// resolution + OS-user mapping) and control-token checks remain the caller's responsibility — this
/// store only resolves the live terminal handle.
#[derive(Clone)]
pub struct DaemonTerminalSessionStore {
    manager: Arc<CliSessionManager>,
}

impl DaemonTerminalSessionStore {
    pub fn new(manager: Arc<CliSessionManager>) -> Self {
        DaemonTerminalSessionStore { manager }
    }
}

#[async_trait]
impl tddy_terminal_rpc::session::TerminalSessionStore for DaemonTerminalSessionStore {
    async fn get_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<dyn tddy_terminal_rpc::session::TerminalSession>> {
        self.manager
            .get_terminal(session_id, terminal_id)
            .await
            .map(|handle| {
                Arc::new(DaemonTerminalSession::new(handle))
                    as Arc<dyn tddy_terminal_rpc::session::TerminalSession>
            })
    }
}
