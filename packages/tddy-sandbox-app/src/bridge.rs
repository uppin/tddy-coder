//! Host-side `SessionChannel` driver with local terminal PTY proxy.
//!
//! Wraps the shared [`tddy_sandbox_runner::run_host_relay`] with an interactive front-end: real
//! stdin/stdout in raw mode and Ctrl-C shutdown. Tool execution runs via [`tool_engine`].

#[cfg(target_os = "macos")]
use std::io::{Read, Write};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use anyhow::{Context, Result};
use bytes::Bytes;
#[cfg(target_os = "macos")]
use tddy_daemon::tool_engine;
#[cfg(target_os = "macos")]
use tddy_sandbox_runner::{run_host_relay, ExecuteToolResponse, HostRelayConfig, HostToolHandler};
#[cfg(target_os = "macos")]
use tddy_task::TaskRegistry;
#[cfg(target_os = "macos")]
use tokio::sync::mpsc;

/// Runs MCP tool calls in the host worktree via [`tool_engine`].
#[cfg(target_os = "macos")]
struct AppToolHandler {
    worktree: PathBuf,
    task_registry: TaskRegistry,
}

#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
pub async fn run_terminal_bridge(
    ready_marker: &Path,
    session_id: &str,
    worktree_path: &Path,
    task_registry: TaskRegistry,
    main_process_exited: Arc<AtomicBool>,
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
                Err(_) => {
                    shutdown_stdin.store(true, Ordering::Relaxed);
                    break;
                }
                // Ctrl-C (0x03) is forwarded to the jail PTY like any other keystroke so the
                // in-jail `claude` receives the interrupt; only a genuine EOF disconnects.
                Ok(n) => match classify_stdin_read(n, &buf) {
                    StdinRead::Disconnected => {
                        shutdown_stdin.store(true, Ordering::Relaxed);
                        break;
                    }
                    StdinRead::Forward(bytes) => {
                        if stdin_tx.send(bytes).is_err() {
                            shutdown_stdin.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                },
            }
        }
    });

    // Block until stdin EOF/Ctrl-C, the relay ends, or an OS Ctrl-C arrives. Also polls the local
    // terminal size on the same cadence, so a live window resize reaches the jail PTY via the same
    // in-band OSC convention ordinary keystrokes already use (no dedicated proto message).
    let mut last_sent_size = (rows, cols);
    loop {
        if let Some(reason) = bridge_stop_reason(
            shutdown.load(Ordering::Relaxed),
            relay.is_finished(),
            main_process_exited.load(Ordering::Relaxed),
        ) {
            log::info!(target: "tddy_sandbox_app::bridge", "bridge loop stopping: {reason:?}");
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
pub(crate) fn resize_frame_if_changed(current: (u16, u16), last_sent: (u16, u16)) -> Option<Bytes> {
    if current == last_sent {
        return None;
    }
    let (rows, cols) = current;
    Some(Bytes::from(
        format!("\x1b]resize;{cols};{rows}\x07").into_bytes(),
    ))
}

/// What to do with the result of a blocking read from the host's real stdin.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StdinRead {
    /// Forward these bytes verbatim to the in-jail PTY (Claude's stdin). Ctrl-C (`0x03`) is
    /// included here: in raw mode the terminal delivers it as a byte rather than raising SIGINT,
    /// and it must reach the underlying `claude` process as an interrupt — not be swallowed by the
    /// host as a disconnect the way the earlier PTY proxy did.
    Forward(Bytes),
    /// stdin reached EOF (`n == 0`, e.g. the controlling terminal closed) — the interactive
    /// session is over and the bridge disconnects.
    Disconnected,
}

/// Classify a blocking stdin read of `n` bytes from `buf` (a fixed-size read buffer). Only the
/// first `n` bytes are meaningful — the remainder of `buf` is stale and must not be forwarded.
pub(crate) fn classify_stdin_read(n: usize, buf: &[u8]) -> StdinRead {
    if n == 0 {
        StdinRead::Disconnected
    } else {
        StdinRead::Forward(Bytes::copy_from_slice(&buf[..n]))
    }
}

/// Why the interactive terminal bridge loop stopped.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BridgeStop {
    /// A host-side shutdown was requested (stdin EOF / read error propagated into the loop, or a
    /// host-delivered SIGINT).
    ShutdownRequested,
    /// The gRPC `SessionChannel` relay task ended.
    RelayFinished,
    /// The spawned sandbox / in-jail Claude main process exited — the sandbox must not outlive the
    /// process it exists to proxy.
    MainProcessExited,
}

/// Decide whether the interactive bridge loop should stop, given the current signals; `None` keeps
/// it looping. A host-requested shutdown takes precedence, then a finished relay, then a
/// main-process exit, so the reported reason is deterministic when more than one is true at once.
pub(crate) fn bridge_stop_reason(
    shutdown_requested: bool,
    relay_finished: bool,
    main_process_exited: bool,
) -> Option<BridgeStop> {
    if shutdown_requested {
        Some(BridgeStop::ShutdownRequested)
    } else if relay_finished {
        Some(BridgeStop::RelayFinished)
    } else if main_process_exited {
        Some(BridgeStop::MainProcessExited)
    } else {
        None
    }
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

pub(crate) struct RawMode {
    #[cfg(unix)]
    saved: libc::termios,
}

impl RawMode {
    pub(crate) fn enable() -> Self {
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
        assert_eq!(frame, Some(Bytes::from_static(b"\x1b]resize;100;30\x07")));
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

    // -----------------------------------------------------------------------
    // classify_stdin_read — decides what to do with the result of a blocking read from the host's
    // real stdin. The interactive front-end must FORWARD Ctrl-C (0x03) to the in-jail `claude` so
    // it reaches Claude as an interrupt, rather than intercepting it as a host-side disconnect the
    // way the earlier PTY proxy did. Only a genuine EOF (n == 0) ends the session.
    // -----------------------------------------------------------------------

    /// Ctrl-C is a keystroke Claude must see, not a host disconnect signal: a lone 0x03 byte is
    /// forwarded verbatim to the jail PTY so the underlying `claude` process is interrupted.
    #[test]
    fn forwards_ctrl_c_to_the_jail_pty_instead_of_disconnecting() {
        // Given
        let buf = [0x03u8];

        // When
        let decision = classify_stdin_read(1, &buf);

        // Then
        assert_eq!(decision, StdinRead::Forward(Bytes::from_static(&[0x03])));
    }

    /// Ordinary typed input is forwarded unchanged to Claude's PTY, exactly the length that was
    /// read (trailing bytes of the fixed-size read buffer are not leaked).
    #[test]
    fn forwards_ordinary_keystrokes_to_the_jail_pty() {
        // Given
        let mut buf = [0u8; 256];
        buf[..3].copy_from_slice(b"ls\r");

        // When
        let decision = classify_stdin_read(3, &buf);

        // Then
        assert_eq!(decision, StdinRead::Forward(Bytes::from_static(b"ls\r")));
    }

    /// Ctrl-C bundled inside a larger chunk is still forwarded whole — the old code special-cased
    /// only a lone `n == 1` 0x03, so this locks in that 0x03 is never treated specially at all.
    #[test]
    fn forwards_ctrl_c_when_bundled_with_other_bytes() {
        // Given
        let mut buf = [0u8; 256];
        buf[..3].copy_from_slice(&[b'a', 0x03, b'b']);

        // When
        let decision = classify_stdin_read(3, &buf);

        // Then
        assert_eq!(
            decision,
            StdinRead::Forward(Bytes::from_static(&[b'a', 0x03, b'b']))
        );
    }

    /// A zero-byte read is stdin EOF (the terminal closed / Ctrl-D): the interactive session is
    /// over and the bridge disconnects.
    #[test]
    fn disconnects_when_stdin_reaches_eof() {
        // Given
        let buf = [0u8; 256];

        // When
        let decision = classify_stdin_read(0, &buf);

        // Then
        assert_eq!(decision, StdinRead::Disconnected);
    }

    // -----------------------------------------------------------------------
    // bridge_stop_reason — decides whether the interactive bridge loop should keep running or
    // stop, given the current signals. The new requirement: the sandbox must not outlive the main
    // in-jail process, so `main_process_exited` is a stop condition in its own right (previously
    // the loop only watched shutdown + relay).
    // -----------------------------------------------------------------------

    /// While nothing has ended, the loop keeps running (returns no stop reason).
    #[test]
    fn keeps_running_while_no_stop_signal_is_present() {
        // Given / When
        let reason = bridge_stop_reason(false, false, false);

        // Then
        assert_eq!(reason, None);
    }

    /// The core new behavior: when the main process (Claude) has exited, the sandbox bridge stops
    /// so it does not linger after the process it exists to proxy is gone.
    #[test]
    fn stops_when_the_main_process_has_exited() {
        // Given / When
        let reason = bridge_stop_reason(false, false, true);

        // Then
        assert_eq!(reason, Some(BridgeStop::MainProcessExited));
    }

    /// The gRPC SessionChannel relay ending is a stop condition.
    #[test]
    fn stops_when_the_relay_finishes() {
        // Given / When
        let reason = bridge_stop_reason(false, true, false);

        // Then
        assert_eq!(reason, Some(BridgeStop::RelayFinished));
    }

    /// A host-side shutdown request (e.g. stdin EOF propagated into the loop) stops the loop.
    #[test]
    fn stops_when_shutdown_is_requested() {
        // Given / When
        let reason = bridge_stop_reason(true, false, false);

        // Then
        assert_eq!(reason, Some(BridgeStop::ShutdownRequested));
    }
}
