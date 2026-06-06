//! Claude Code CLI session manager — spawns `claude` CLI in a subprocess, plumbs I/O via
//! tokio channels. Designed for raw-terminal forwarding via `StreamSessionTerminalIO` gRPC.
//!
//! The acceptance tests use `/bin/cat` as the stub binary; for production, `claude` is used.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc, RwLock};

/// Capacity for the broadcast output channel (bytes chunks from process stdout to all subscribers).
const OUTPUT_BROADCAST_CAPACITY: usize = 256;

/// Handle to a running claude CLI process.
pub struct PtyHandle {
    pub worktree_path: PathBuf,
    pub model: String,
    /// Send bytes to the child process stdin.
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    /// Subscribe to bytes from the child process stdout/stderr.
    pub stdout_tx: broadcast::Sender<Bytes>,
    /// PID of the spawned process.
    pub pid: u32,
}

/// Manages a registry of active Claude CLI sessions (session_id → PtyHandle).
pub struct ClaudeCliSessionManager {
    registry: Arc<RwLock<HashMap<String, Arc<PtyHandle>>>>,
}

impl ClaudeCliSessionManager {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Spawn a new claude CLI process for `session_id` in `worktree_path`.
    ///
    /// Returns an `Arc<PtyHandle>` on success. The child process is monitored in a background
    /// task; when it exits the session is removed from the registry.
    pub async fn start(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
        let (stdout_tx, _stdout_rx) = broadcast::channel::<Bytes>(OUTPUT_BROADCAST_CAPACITY);

        let mut child = self
            .spawn_child(binary_path, model, session_id, &worktree_path)
            .await?;
        let pid = child.id().ok_or_else(|| anyhow::anyhow!("child has no pid"))?;

        // Plumb stdin from the channel to the child's stdin pipe.
        if let Some(child_stdin) = child.stdin.take() {
            Self::spawn_stdin_forwarder(child_stdin, stdin_rx);
        }

        // Plumb stdout+stderr from the child to the broadcast channel.
        let stdout_tx_clone = stdout_tx.clone();
        if let Some(child_stdout) = child.stdout.take() {
            Self::spawn_stdout_reader(child_stdout, stdout_tx_clone.clone());
        }
        if let Some(child_stderr) = child.stderr.take() {
            Self::spawn_stdout_reader(child_stderr, stdout_tx_clone);
        }

        let handle = Arc::new(PtyHandle {
            worktree_path,
            model: model.to_string(),
            stdin_tx,
            stdout_tx,
            pid,
        });

        let reg = Arc::clone(&self.registry);
        let sid = session_id.to_string();
        let handle_clone = Arc::clone(&handle);
        tokio::spawn(async move {
            // Monitor the child process; remove from registry when it exits.
            if let Err(e) = child.wait().await {
                log::debug!(
                    target: "tddy_daemon::claude_cli_session",
                    "child wait error for session {}: {}",
                    sid,
                    e
                );
            }
            log::info!(
                target: "tddy_daemon::claude_cli_session",
                "claude-cli process exited for session {}",
                sid
            );
            drop(handle_clone); // release Arc so registry holds the only strong ref
            reg.write().await.remove(&sid);
        });

        self.registry
            .write()
            .await
            .insert(session_id.to_string(), Arc::clone(&handle));

        Ok(handle)
    }

    /// Resume (relaunch) an existing session by spawning a new process in the same worktree.
    pub async fn resume(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        // Start a fresh process in the same worktree.
        self.start(session_id, worktree_path, model, binary_path)
            .await
    }

    /// Look up an active session by id.
    pub async fn get(&self, session_id: &str) -> Option<Arc<PtyHandle>> {
        self.registry.read().await.get(session_id).cloned()
    }

    // --- private helpers ---

    async fn spawn_child(
        &self,
        binary_path: &str,
        model: &str,
        session_id: &str,
        worktree_path: &PathBuf,
    ) -> anyhow::Result<tokio::process::Child> {
        let mut cmd = tokio::process::Command::new(binary_path);
        if !model.is_empty() {
            cmd.arg("--model").arg(model);
        }
        cmd.arg("--session-id").arg(session_id);
        cmd.current_dir(worktree_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!(
                "failed to spawn claude-cli binary {:?}: {}",
                binary_path,
                e
            )
        })?;
        Ok(child)
    }

    fn spawn_stdin_forwarder(
        mut child_stdin: tokio::process::ChildStdin,
        mut rx: mpsc::UnboundedReceiver<Bytes>,
    ) {
        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                if child_stdin.write_all(&data).await.is_err() {
                    break;
                }
            }
        });
    }

    fn spawn_stdout_reader<R: AsyncReadExt + Unpin + Send + 'static>(
        mut reader: R,
        tx: broadcast::Sender<Bytes>,
    ) {
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = Bytes::copy_from_slice(&buf[..n]);
                        // Ignore send errors (no active subscribers).
                        let _ = tx.send(chunk);
                    }
                    Err(_) => break,
                }
            }
        });
    }
}
