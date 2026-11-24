//! A target's terminal as [`type_into`](crate::terminal_keys::type_into) types into it for
//! `SendSessionKeys`: read and written through the home session's lease.

use async_trait::async_trait;
use tddy_worktree_service::session_target::SessionTarget;

use super::ProjectHomeTerminalPort;
use crate::terminal_keys::TypingTerminal;

/// A target's terminal typed into under a home session's lease: its output read through
/// [`ProjectHomeTerminalPort::read_terminal`], each write made through
/// [`ProjectHomeTerminalPort::send_keys`], which checks the lease.
pub(super) struct LeasedTerminal<'a> {
    pub(super) port: &'a dyn ProjectHomeTerminalPort,
    pub(super) target: &'a SessionTarget,
    pub(super) screen_id: &'a str,
}

#[async_trait]
impl TypingTerminal for LeasedTerminal<'_> {
    async fn output_offset(&self) -> Result<u64, String> {
        self.port
            .read_terminal(self.target)
            .await
            .map(|grid| grid.offset)
            .map_err(|e| format!("cannot read the terminal: {e:#}"))
    }

    async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.port.send_keys(self.target, self.screen_id, bytes).await
    }
}
