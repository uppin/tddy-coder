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

/// Spawn `argv` in a PTY inside `cwd` (with extra `env`) and relay its I/O to the local
/// stdin/stdout until the child exits. Resizes the PTY on local `SIGWINCH`.
pub async fn run(argv: Vec<String>, cwd: PathBuf, env: Vec<(String, String)>) -> Result<()> {
    if argv.is_empty() {
        anyhow::bail!("local_pty_relay: empty argv");
    }
    let (rows, cols) = crate::local_terminal::terminal_size();

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

    let _raw = crate::local_terminal::RawMode::enable();

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

    // Input: local stdin → PTY. Stops when stdin closes or the child has exited, so a blocked stdin
    // read cannot keep a sender clone alive after the command finishes.
    let mut stdin_status = task.status_watch();
    let mut stdin_pump = tokio::spawn({
        let stdin_sender = stdin_sender.clone();
        async move {
            let mut stdin = tokio::io::stdin();
            let mut buf = vec![0u8; 4096];
            loop {
                if stdin_status.borrow().is_terminal() {
                    break;
                }
                tokio::select! {
                    changed = stdin_status.changed() => {
                        if changed.is_err() || stdin_status.borrow().is_terminal() {
                            break;
                        }
                    }
                    res = stdin.read(&mut buf) => {
                        match res {
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
                let (r, c) = crate::local_terminal::terminal_size();
                pty_registry_for_resize.resize(&task_id, r, c).await;
            }
        }
        #[cfg(not(unix))]
        {
            std::future::pending::<()>().await;
        }
    });

    let mut status_watch = task.status_watch();
    let wait_for_child = async {
        while !status_watch.borrow().is_terminal() {
            if status_watch.changed().await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        _ = wait_for_child => {}
        _ = &mut stdin_pump => {}
    }

    drop(stdin_sender);
    stdin_pump.abort();
    stdout_pump.abort();
    resize_pump.abort();
    drop(ready);

    Ok(())
}
