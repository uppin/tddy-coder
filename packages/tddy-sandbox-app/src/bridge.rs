//! Host-side `SessionChannel` driver with local terminal PTY proxy.
//!
//! Wraps the shared [`tddy_sandbox_runner::run_host_relay`] with an interactive front-end: real
//! stdin/stdout in raw mode and Ctrl-C shutdown. Tool execution runs via [`tool_engine`].

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use tddy_daemon::tool_engine;
use tddy_sandbox_runner::{run_host_relay, ExecuteToolResponse, HostRelayConfig, HostToolHandler};
use tddy_task::TaskRegistry;
use tokio::sync::mpsc;

/// Runs MCP tool calls in the host worktree via [`tool_engine`].
struct AppToolHandler {
    worktree: PathBuf,
    task_registry: TaskRegistry,
}

#[async_trait::async_trait]
impl HostToolHandler for AppToolHandler {
    async fn execute(
        &self,
        session_id: &str,
        tool_name: &str,
        args_json: &str,
    ) -> ExecuteToolResponse {
        let outcome = tool_engine::execute_tool(
            &self.worktree,
            tool_name,
            args_json,
            &self.task_registry,
            session_id,
        )
        .await;
        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }
}

/// Connect to the in-jail sandbox gRPC server and relay stdin/stdout until disconnect.
pub async fn run_terminal_bridge(
    ready_marker: &Path,
    session_id: &str,
    worktree_path: &Path,
    task_registry: TaskRegistry,
) -> Result<()> {
    eprintln!("connecting SessionChannel on loopback…");
    let client = tddy_sandbox_darwin::connect_sandbox_client(ready_marker)
        .await
        .context("connect sandbox gRPC")?;

    let (rows, cols) = terminal_size_or_default();
    let (terminal_tx, mut terminal_rx) = mpsc::unbounded_channel::<Bytes>();
    let stdout_task = tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        while let Some(chunk) = terminal_rx.recv().await {
            let _ = stdout.write_all(&chunk);
            let _ = stdout.flush();
        }
    });

    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
    let relay = run_host_relay(
        client,
        AppToolHandler {
            worktree: worktree_path.to_path_buf(),
            task_registry,
        },
        HostRelayConfig {
            session_id: session_id.to_string(),
            terminal_sink: terminal_tx,
            initial_cols: cols as u32,
            initial_rows: rows as u32,
        },
        stdin_rx,
    )
    .await
    .map_err(|e| anyhow::anyhow!("run host relay: {e}"))?;

    log::info!(
        target: "tddy_sandbox_app::bridge",
        "SessionChannel open session_id={session_id} terminal={cols}x{rows}"
    );
    eprintln!("terminal bridge active (Ctrl-C or Ctrl-D to disconnect)");

    let _raw = RawMode::enable();
    let shutdown = Arc::new(AtomicBool::new(false));

    // Real stdin → the jail PTY. A blocking read thread feeds the relay's stdin channel. Clone the
    // sender first: the resize-polling loop below also needs to push in-band OSC resize frames
    // down the same channel, but the thread closure takes ownership of its own sender.
    let resize_tx = stdin_tx.clone();
    let shutdown_stdin = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        let mut stdin = std::io::stdin();
        loop {
            if shutdown_stdin.load(Ordering::Relaxed) {
                break;
            }
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => {
                    shutdown_stdin.store(true, Ordering::Relaxed);
                    break;
                }
                Ok(n) => {
                    if n == 1 && buf[0] == 0x03 {
                        log::info!(target: "tddy_sandbox_app::bridge", "Ctrl-C — disconnecting");
                        shutdown_stdin.store(true, Ordering::Relaxed);
                        break;
                    }
                    if stdin_tx.send(Bytes::from(buf[..n].to_vec())).is_err() {
                        shutdown_stdin.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
    });

    // Block until stdin EOF/Ctrl-C, the relay ends, or an OS Ctrl-C arrives. Also polls the local
    // terminal size on the same cadence, so a live window resize reaches the jail PTY via the same
    // in-band OSC convention ordinary keystrokes already use (no dedicated proto message).
    let mut last_sent_size = (rows, cols);
    while !shutdown.load(Ordering::Relaxed) {
        if relay.is_finished() {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                let current = terminal_size_or_default();
                if let Some(frame) = resize_frame_if_changed(current, last_sent_size) {
                    if resize_tx.send(frame).is_err() {
                        log::warn!(target: "tddy_sandbox_app::bridge", "resize frame: stdin channel closed");
                    } else {
                        last_sent_size = current;
                    }
                }
            }
            res = tokio::signal::ctrl_c() => {
                match res {
                    Ok(()) => {
                        log::info!(target: "tddy_sandbox_app::bridge", "Ctrl-C — shutting down");
                        shutdown.store(true, Ordering::Relaxed);
                    }
                    Err(e) => {
                        log::warn!(target: "tddy_sandbox_app::bridge", "ctrl_c listener: {e}");
                    }
                }
                break;
            }
        }
    }
    shutdown.store(true, Ordering::Relaxed);
    relay.abort();
    stdout_task.abort();
    Ok(())
}

/// Encode a live terminal resize as the `\x1b]resize;{cols};{rows}\x07` OSC sequence understood by
/// `tddy_sandbox_runner`'s `strip_resize_escape`, when `current` differs from `last_sent`.
///
/// Both tuples are `(rows, cols)` (matching [`terminal_size_or_default`]'s return order); the OSC
/// payload itself is `cols;rows`, matching the wire format `tddy_daemon::claude_cli_session::
/// strip_resize` and `tddy_tools::pty_relay::encode_resize_osc` already use.
fn resize_frame_if_changed(current: (u16, u16), last_sent: (u16, u16)) -> Option<Bytes> {
    if current == last_sent {
        return None;
    }
    let (rows, cols) = current;
    Some(Bytes::from(
        format!("\x1b]resize;{cols};{rows}\x07").into_bytes(),
    ))
}

/// The host's own controlling terminal size, `(rows, cols)`. Read both here (to size the PTY
/// live-resize polling loop) and from `spawn::spawn_claude_sandbox` (to open the jail's PTY at
/// the right size from the start, via `--initial-cols`/`--initial-rows`) — same ioctl, same
/// controlling terminal, just called at two different points before/after attach.
pub(crate) fn terminal_size_or_default() -> (u16, u16) {
    #[cfg(unix)]
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0
            && ws.ws_row > 0
            && ws.ws_col > 0
        {
            return (ws.ws_row, ws.ws_col);
        }
    }
    (24, 220)
}

struct RawMode {
    #[cfg(unix)]
    saved: libc::termios,
}

impl RawMode {
    fn enable() -> Self {
        #[cfg(unix)]
        unsafe {
            let mut saved: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut saved) == 0 {
                let mut raw = saved;
                libc::cfmakeraw(&mut raw);
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
                return Self { saved };
            }
        }
        Self {
            #[cfg(unix)]
            saved: unsafe { std::mem::zeroed() },
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // resize_frame_if_changed — decides whether a polled host terminal size (as returned by
    // `terminal_size_or_default`, `(rows, cols)`) differs from the size last sent to the jail,
    // and if so encodes the `\x1b]resize;{cols};{rows}\x07` OSC sequence (the same wire format
    // `tddy_daemon::claude_cli_session::strip_resize` and `tddy_tools::pty_relay::encode_resize_osc`
    // already use) so the sandboxed PTY can be resized live instead of only once at attach time.
    // -----------------------------------------------------------------------

    /// No terminal resize occurred since the last frame was sent — nothing should go out over
    /// the stdin channel to avoid spamming the jail with no-op resize escapes.
    #[test]
    fn returns_none_when_terminal_size_is_unchanged() {
        // Given
        let last_sent = (57, 170);
        let current = (57, 170);

        // When
        let frame = resize_frame_if_changed(current, last_sent);

        // Then
        assert_eq!(frame, None);
    }

    /// A genuine terminal resize is encoded as the OSC escape sequence the sandbox-runner's
    /// `strip_resize_escape` understands, with cols before rows in the payload.
    #[test]
    fn returns_the_encoded_osc_resize_sequence_when_size_changes() {
        // Given
        let last_sent = (24, 80);
        let current = (30, 100);

        // When
        let frame = resize_frame_if_changed(current, last_sent);

        // Then
        assert_eq!(
            frame,
            Some(Bytes::from_static(b"\x1b]resize;100;30\x07"))
        );
    }

    /// A change in rows alone (columns unchanged) still counts as a resize — a naive
    /// implementation that only compares columns would miss this.
    #[test]
    fn detects_a_size_change_when_only_rows_differ() {
        // Given
        let last_sent = (24, 80);
        let current = (40, 80);

        // When
        let frame = resize_frame_if_changed(current, last_sent);

        // Then
        assert_eq!(frame, Some(Bytes::from_static(b"\x1b]resize;80;40\x07")));
    }
}
