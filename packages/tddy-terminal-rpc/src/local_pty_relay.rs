//! Local PTY relay: spawn a command in a PTY (via [`tddy_pty::PtyRuntime`]) and bridge its I/O to
//! the local stdin/stdout, with raw-mode and SIGWINCH resize handling.
//!
//! This replaces the standalone `portable-pty` usage that previously lived in
//! `tddy-tools/src/pty_relay.rs::run_local_pty`, so every PTY spawn in the repo now goes through
//! the shared `tddy-pty` runtime + `tddy-task` capture ring.

use std::path::PathBuf;

use anyhow::Result;
use bytes::Bytes;
use tddy_pty::{PtyRegistry, PtyRuntime, PtySpawnSpec};
use tddy_task::TaskRegistry;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Default terminal size when the local terminal size cannot be read.
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 220;

/// Read the local terminal size via `TIOCGWINSZ`, falling back to a default.
fn local_terminal_size() -> (u16, u16) {
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
    (DEFAULT_ROWS, DEFAULT_COLS)
}

/// Raw-mode guard: puts the local stdin into raw mode for the lifetime of the returned value so the
/// spawned command receives keystrokes verbatim. Restores the saved termios on drop.
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

/// Spawn `argv` in a PTY inside `cwd` (with extra `env`) and relay its I/O to the local
/// stdin/stdout until the child exits. Resizes the PTY on local `SIGWINCH`.
pub async fn run(argv: Vec<String>, cwd: PathBuf, env: Vec<(String, String)>) -> Result<()> {
    if argv.is_empty() {
        anyhow::bail!("local_pty_relay: empty argv");
    }
    let (rows, cols) = local_terminal_size();

    let registry = TaskRegistry::new();
    let pty_registry = PtyRegistry::new();
    let spec = PtySpawnSpec {
        argv,
        worktree_path: cwd,
        session_id: "local-pty-relay".to_string(),
        terminal_id: "local".to_string(),
        kind: "local".to_string(),
        env,
    };
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let task = PtyRuntime::spawn(&registry, &pty_registry, spec, ready_tx).await;
    let ready = ready_rx
        .await
        .map_err(|e| anyhow::anyhow!("ready signal dropped: {e}"))?
        .map_err(|e| anyhow::anyhow!("pty spawn failed: {e}"))?;

    // Resize the PTY to the local terminal size now that we know it (the runtime spawns at the
    // default 24×220; the local terminal may be larger).
    pty_registry.resize(&task.id, rows, cols).await;

    let channel = task
        .channel("0")
        .ok_or_else(|| anyhow::anyhow!("missing PTY channel"))?;
    let stdin_sender = channel
        .stdin_sender()
        .ok_or_else(|| anyhow::anyhow!("PTY channel has no stdin"))?;
    let mut stdout_rx = channel.subscribe();

    let _raw = RawMode::enable();

    // Output: PTY → local stdout.
    let stdout_pump = tokio::spawn({
        let mut stdout = tokio::io::stdout();
        async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match stdout_rx.recv().await {
                    Ok(bytes) => {
                        if stdout.write_all(&bytes).await.is_err() {
                            break;
                        }
                        let _ = stdout.flush().await;
                    }
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(_)) => continue,
                }
            }
        }
    });

    // Input: local stdin → PTY. Stops when stdin closes so the writer thread (inside the runtime)
    // releases the master fd promptly.
    let stdin_pump = tokio::spawn({
        let stdin_sender = stdin_sender.clone();
        async move {
            let mut stdin = tokio::io::stdin();
            let mut buf = vec![0u8; 4096];
            loop {
                match stdin.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if stdin_sender
                            .send(Bytes::copy_from_slice(&buf[..n]))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Resize: SIGWINCH → pty_registry.resize.
    let task_id = task.id.clone();
    let pty_registry_for_resize = pty_registry.clone();
    let resize_pump = tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sig = match signal(SignalKind::window_change()) {
                Ok(s) => s,
                Err(_) => return,
            };
            loop {
                if sig.recv().await.is_none() {
                    break;
                }
                let (r, c) = local_terminal_size();
                pty_registry_for_resize.resize(&task_id, r, c).await;
            }
        }
        #[cfg(not(unix))]
        {
            std::future::pending::<()>().await;
        }
    });

    // Wait for the child to exit. The runtime's task reaches a terminal status then.
    let mut status_watch = task.status_watch();
    while !status_watch.borrow().is_terminal() {
        if status_watch.changed().await.is_err() {
            break;
        }
    }

    // Tear down: drop the stdin sender so the runtime's writer thread exits, then abort the
    // pumps. The stdin pump is blocked on a blocking stdin read (no input will arrive), so it is
    // aborted rather than awaited; the output pump is aborted once the child is gone. The
    // registry ages the task out via its TTL.
    drop(stdin_sender);
    stdin_pump.abort();
    stdout_pump.abort();
    resize_pump.abort();
    drop(ready);

    Ok(())
}
