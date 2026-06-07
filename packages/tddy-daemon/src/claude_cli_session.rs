//! Claude Code CLI session manager — spawns `claude` CLI in a PTY, plumbs I/O via
//! tokio channels. Designed for raw-terminal forwarding via `StreamSessionTerminalIO` gRPC.
//!
//! Uses `portable-pty` so that `claude` (a TUI) sees a controlling terminal (TTY), allowing it to
//! run interactively. Without a PTY, `claude` detects no TTY and exits immediately.
//!
//! The acceptance tests use `/bin/cat` as the stub binary; for production, `claude` is used.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::{broadcast, mpsc, RwLock};

/// Default terminal size for spawned claude sessions.
const DEFAULT_TERM_ROWS: u16 = 24;
const DEFAULT_TERM_COLS: u16 = 220;

/// Capacity for the broadcast output channel (bytes chunks from PTY master to all subscribers).
const OUTPUT_BROADCAST_CAPACITY: usize = 256;

/// Handle to a running claude CLI process in a PTY.
pub struct PtyHandle {
    pub worktree_path: PathBuf,
    pub model: String,
    /// Send bytes to the child process via PTY master (stdin).
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    /// Subscribe to bytes from the child process via PTY master (stdout+stderr combined).
    pub stdout_tx: broadcast::Sender<Bytes>,
    /// PID of the spawned process.
    pub pid: u32,
    /// PTY master — kept alive for the session's lifetime to avoid SIGHUP; also allows resize.
    master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
}

impl PtyHandle {
    /// Send a SIGWINCH (window resize) to the child process to force a full-screen redraw.
    ///
    /// Useful when a new `streamSessionTerminalIO` subscriber connects and has missed the
    /// initial render: after subscribing, call this so claude repaints to the live channel.
    pub fn trigger_redraw(&self) {
        if let Ok(m) = self.master.lock() {
            let _ = m.resize(PtySize {
                rows: DEFAULT_TERM_ROWS,
                cols: DEFAULT_TERM_COLS,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }
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
    /// std thread; when it exits the session is removed from the registry.
    pub async fn start(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
        let (stdout_tx, _stdout_rx) = broadcast::channel::<Bytes>(OUTPUT_BROADCAST_CAPACITY);

        let session_id_owned = session_id.to_string();
        let model_owned = model.to_string();
        let binary_owned = binary_path.to_string();
        let worktree_clone = worktree_path.clone();
        let stdout_tx_clone = stdout_tx.clone();
        let reg = Arc::clone(&self.registry);

        // portable-pty I/O is blocking; spawn everything from a dedicated OS thread.
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let res = Self::spawn_in_pty(
                &session_id_owned,
                worktree_clone,
                &model_owned,
                &binary_owned,
                stdin_rx,
                stdout_tx_clone,
                reg,
            );
            let _ = result_tx.send(res);
        });

        let (pid, master) = result_rx
            .await
            .map_err(|_| anyhow::anyhow!("PTY spawn thread did not respond"))??;

        let handle = Arc::new(PtyHandle {
            worktree_path,
            model: model.to_string(),
            stdin_tx,
            stdout_tx,
            pid,
            master,
        });

        // Insert into the registry BEFORE returning so that a racing streamSessionTerminalIO
        // call (browser connects immediately after startSession returns) can find the handle.
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

    /// Spawn the child in a PTY. Runs in a dedicated OS thread (portable-pty I/O is blocking).
    ///
    /// Returns `(pid, Arc<Mutex<MasterPty>>)` on success. Launches background threads for I/O
    /// forwarding and exit monitoring. The registry `Arc` is used for cleanup on exit.
    fn spawn_in_pty(
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        stdin_rx: mpsc::UnboundedReceiver<Bytes>,
        stdout_tx: broadcast::Sender<Bytes>,
        reg: Arc<RwLock<HashMap<String, Arc<PtyHandle>>>>,
    ) -> anyhow::Result<(u32, Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>)> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: DEFAULT_TERM_ROWS,
                cols: DEFAULT_TERM_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow::anyhow!("openpty failed: {}", e))?;

        let mut cmd = CommandBuilder::new(binary_path);
        if !model.is_empty() {
            cmd.arg("--model");
            cmd.arg(model);
        }
        cmd.arg("--session-id");
        cmd.arg(session_id);
        cmd.cwd(&worktree_path);

        // Spawn the child on the slave side. The slave is consumed/closed after spawn so the
        // master sees EOF when the child exits.
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow::anyhow!("failed to spawn claude-cli binary {:?}: {}", binary_path, e))?;
        // Drop slave so master sees EOF on child exit.
        drop(pair.slave);

        let pid = child
            .process_id()
            .ok_or_else(|| anyhow::anyhow!("spawned child has no pid"))?;

        let master = Arc::new(std::sync::Mutex::new(pair.master));

        // Reader thread: PTY master → broadcast channel.
        let master_for_reader = Arc::clone(&master);
        let stdout_tx_reader = stdout_tx.clone();
        std::thread::spawn(move || {
            let reader = {
                let m = master_for_reader.lock().unwrap();
                m.try_clone_reader()
            };
            match reader {
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::claude_cli_session",
                        "PTY reader: try_clone_reader failed: {}",
                        e
                    );
                }
                Ok(mut r) => {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match std::io::Read::read(&mut r, &mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                let chunk = Bytes::copy_from_slice(&buf[..n]);
                                let _ = stdout_tx_reader.send(chunk);
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        // Writer thread: mpsc channel → PTY master.
        let master_for_writer = Arc::clone(&master);
        let mut stdin_rx_thread = stdin_rx;
        std::thread::spawn(move || {
            let writer = {
                let m = master_for_writer.lock().unwrap();
                m.take_writer()
            };
            match writer {
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::claude_cli_session",
                        "PTY writer: take_writer failed: {}",
                        e
                    );
                }
                Ok(mut w) => {
                    // Use a blocking recv loop (we're already on a std thread).
                    let rt = tokio::runtime::Handle::try_current();
                    // If inside a tokio context, use block_in_place; otherwise create a local runtime.
                    if let Ok(handle) = rt {
                        tokio::task::block_in_place(|| {
                            loop {
                                // block_on a single recv at a time
                                let data = handle.block_on(stdin_rx_thread.recv());
                                match data {
                                    None => break,
                                    Some(bytes) => {
                                        if std::io::Write::write_all(&mut w, &bytes).is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        });
                    } else {
                        // No tokio context — use a simple busy approach
                        while let Some(bytes) = stdin_rx_thread.blocking_recv() {
                            if std::io::Write::write_all(&mut w, &bytes).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        // Exit monitor thread: wait for child exit → remove from registry.
        let sid = session_id.to_string();
        let mut child_monitor = child;
        std::thread::spawn(move || {
            if let Err(e) = child_monitor.wait() {
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
            // Remove from registry using a lightweight tokio runtime block.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let reg_clone = Arc::clone(&reg);
                let sid_clone = sid.clone();
                handle.spawn(async move {
                    reg_clone.write().await.remove(&sid_clone);
                });
            }
        });

        Ok((pid, master))
    }
}
