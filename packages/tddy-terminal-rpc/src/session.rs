//! Transport-agnostic terminal-session abstraction shared by the daemon and coder bridge
//! functions in [`crate::bridge`].
//!
//! [`TerminalSession`] mirrors the live-handle shape both backends already expose (a broadcast of
//! stdout bytes, a rolling [`TerminalCapture`] ring, a `pty_done` watch, an async resize, an input
//! sink, and an applied-input-offset watch). [`TerminalSessionStore`] resolves a `(session_id,
//! terminal_id)` to a live [`TerminalSession`]; the daemon impl applies OS-user impersonation and
//! session-token auth, the coder impl does neither.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, watch};

/// A live terminal a streaming RPC attaches to: the broadcast fan-out of stdout bytes, the rolling
/// replay ring, the process-exit watch, and the resize / input / ACK controls.
///
/// Every method returns a fresh handle (a new broadcast/watch receiver, a cloned capture arc) so a
/// bridge can subscribe without taking ownership of the session.
#[async_trait]
pub trait TerminalSession: Send + Sync {
    /// The rolling replay ring. The bridge locks this to read the mode prologue and the
    /// last-frame / history chunks.
    fn capture(&self) -> Arc<Mutex<TerminalCapture>>;

    /// Subscribe to live stdout bytes. Subscribe BEFORE snapshotting the capture ring so bytes
    /// produced between the snapshot and the first recv are still delivered.
    fn subscribe_stdout(&self) -> broadcast::Receiver<Bytes>;

    /// Watch for child-process exit; the bridge ends the output stream when this fires.
    fn subscribe_pty_done(&self) -> watch::Receiver<bool>;

    /// Watch the applied-input-offset for ACK frames interleaved on the output stream.
    fn subscribe_acked_offset(&self) -> watch::Receiver<u64>;

    /// Resize the PTY (SIGWINCH) to the given dimensions.
    async fn resize(&self, rows: u16, cols: u16);

    /// Forward input to the PTY stdin, intercepting an embedded OSC resize escape, and advance the
    /// applied-input offset.
    fn send_input(&self, data: Bytes, input_offset: u64);

    /// Nudge the child to redraw at the current PTY size. The default is a no-op; the daemon
    /// overrides it to issue a second SIGWINCH so TUIs that don't redraw on resize still produce a
    /// fresh post-resize frame for the bridge to forward.
    fn trigger_redraw(&self) {}
}

/// Resolves a `(session_id, terminal_id)` to a live [`TerminalSession`]. The daemon impl applies
/// session-token auth and OS-user mapping; the coder impl resolves from its in-memory terminal
/// manager. Returns `None` when the session or terminal is unknown or no longer running.
#[async_trait]
pub trait TerminalSessionStore: Send + Sync {
    async fn get_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<dyn TerminalSession>>;
}
