//! Adapters that expose the coder's [`PtyHandle`] as a [`tddy_terminal_rpc::session::TerminalSession`]
//! and the coder's [`TerminalManager`] as a [`TerminalSessionStore`], so the coder's LiveKit
//! terminal RPC arms can delegate to the unified streaming bridge in `tddy-terminal-rpc`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_pty::Bytes;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, watch};

use crate::session_participant::terminal_manager::{PtyHandle, TerminalManager};

/// A live coder shell terminal exposed to the unified bridge.
pub struct CoderTerminalSession {
    handle: Arc<PtyHandle>,
}

impl CoderTerminalSession {
    pub fn new(handle: Arc<PtyHandle>) -> Self {
        CoderTerminalSession { handle }
    }
}

#[async_trait]
impl tddy_terminal_rpc::session::TerminalSession for CoderTerminalSession {
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
        self.handle.resize(rows, cols).await;
    }

    fn send_input(&self, data: Bytes, input_offset: u64) {
        self.handle.send_input(data, input_offset);
    }
}

/// A [`TerminalSessionStore`] backed by the coder's [`TerminalManager`]. The coder is single-
/// session, so `session_id` is ignored and only `terminal_id` is resolved.
#[derive(Clone)]
pub struct CoderTerminalSessionStore {
    manager: Arc<TerminalManager>,
}

impl CoderTerminalSessionStore {
    pub fn new(manager: Arc<TerminalManager>) -> Self {
        CoderTerminalSessionStore { manager }
    }
}

#[async_trait]
impl tddy_terminal_rpc::session::TerminalSessionStore for CoderTerminalSessionStore {
    async fn get_terminal(
        &self,
        _session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<dyn tddy_terminal_rpc::session::TerminalSession>> {
        self.manager.get_terminal(terminal_id).await.map(|handle| {
            Arc::new(CoderTerminalSession::new(handle))
                as Arc<dyn tddy_terminal_rpc::session::TerminalSession>
        })
    }
}
