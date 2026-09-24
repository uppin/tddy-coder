use super::strip_resize;

use crate::pty_runtime::DEFAULT_TERM_COLS;

use crate::pty_runtime::DEFAULT_TERM_ROWS;

use portable_pty::PtySize;
use tddy_task::{TaskChannel, TaskId};

use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc, watch};

use std::sync::Arc;

use bytes::Bytes;

use std::path::PathBuf;

/// Handle to a running process in a PTY (the `claude` CLI for the main terminal, or a login shell
/// for started terminals).
///
/// Backed by a [`TaskHandle`] in the shared [`TaskRegistry`]; I/O is plumbed through the task's
/// PTY channel and resize control lives in [`PtyRegistry`].
pub struct PtyHandle {
    /// Stable identifier within the session; [`MAIN_TERMINAL_ID`] for the main `claude` terminal.
    pub terminal_id: String,
    /// Tool kind label: `"claude-cli"` for the main terminal, `"bash"` for started Bash tools.
    pub kind: String,
    pub worktree_path: PathBuf,
    pub model: String,
    /// Send bytes to the child process via PTY master (stdin).
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    /// Subscribe to bytes from the child process via PTY master (stdout+stderr combined).
    pub stdout_tx: broadcast::Sender<Bytes>,
    /// Rolling capture of recent PTY output plus the terminal modes still in effect, for replay
    /// to late subscribers.
    pub capture: Arc<std::sync::Mutex<TerminalCapture>>,
    /// PID of the spawned process.
    pub pid: u32,
    /// PTY master — kept alive for the session's lifetime to avoid SIGHUP; also allows resize.
    pub(crate) master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    /// Becomes true (and sender drops) when the task reaches a terminal status.
    pub pty_done: watch::Receiver<bool>,
    /// Current PTY dimensions, updated by `resize()`.
    pub(crate) current_size: Arc<std::sync::Mutex<PtySize>>,
    /// Owning task in the shared registry.
    pub(crate) task_id: TaskId,
    /// Shared per-terminal I/O channel — the source of stdin/stdout/capture and the input-offset
    /// ACK state. Held here (not just the derived senders) so the ACK state is shared across every
    /// `PtyHandle` rebuilt for this terminal by `resolve_pty_handle`.
    pub(crate) channel: Arc<TaskChannel>,
}

impl PtyHandle {
    /// Resize the PTY to the given dimensions and signal the child with SIGWINCH.
    pub fn resize(&self, rows: u16, cols: u16) {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        if let Ok(m) = self.master.lock() {
            let _ = m.resize(size);
        }
        if let Ok(mut s) = self.current_size.lock() {
            *s = size;
        }
    }

    /// Send a SIGWINCH (window resize) to the child process to force a full-screen redraw.
    ///
    /// Useful when a new `streamSessionTerminalIO` subscriber connects and has missed the
    /// initial render: after subscribing, call this so claude repaints to the live channel.
    pub fn trigger_redraw(&self) {
        if let Ok(m) = self.master.lock() {
            let size = self.current_size.lock().map(|s| *s).unwrap_or(PtySize {
                rows: DEFAULT_TERM_ROWS,
                cols: DEFAULT_TERM_COLS,
                pixel_width: 0,
                pixel_height: 0,
            });
            let _ = m.resize(size);
        }
    }

    /// Forward input data to the PTY stdin, stripping any embedded resize escape sequence, and
    /// acknowledge the client's cumulative `input_offset`.
    ///
    /// When `\x1b]resize;{cols};{rows}\x07` is found, the PTY is resized (SIGWINCH sent)
    /// and the escape bytes are not forwarded to the subprocess. Used by both the bidi and
    /// unary input paths so resize always works regardless of which transport the client uses.
    ///
    /// `input_offset` is the running byte total the client reports for this chunk (0 = unset, for
    /// legacy clients). After the input is handled, the applied offset advances to the maximum of
    /// its current value and `input_offset`; when it advances, the new value is published to
    /// `StreamTerminalOutput` subscribers as an ACK. Resize bytes stripped above are still counted
    /// (the client counted them), so the ACK matches the client's byte accounting exactly.
    pub fn send_input(&self, data: bytes::Bytes, input_offset: u64) {
        let (resize, remaining) = strip_resize(&data);
        if let Some((cols, rows)) = resize {
            self.resize(rows, cols);
        }
        if !remaining.is_empty() {
            let _ = self.stdin_tx.send(remaining);
        }
        self.channel.acknowledge_input(input_offset);
    }

    /// Subscribe to applied-input-offset changes — the ACK source for `StreamTerminalOutput`.
    ///
    /// The receiver's initial value is the current applied offset; each `changed()` observes a new
    /// monotonic maximum published by [`send_input`](Self::send_input).
    pub fn subscribe_acked_offset(&self) -> watch::Receiver<u64> {
        self.channel.subscribe_acked_offset()
    }
}
