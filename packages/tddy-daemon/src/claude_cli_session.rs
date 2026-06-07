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

use async_trait::async_trait;
use bytes::Bytes;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use prost::Message as _;
use tddy_livekit::{LiveKitParticipant, RpcResult, RpcService, TokenGenerator};
use tddy_rpc::{BidiStreamOutput, ResponseBody, RpcMessage};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
use tokio::sync::{broadcast, mpsc, watch, RwLock};

/// Default terminal size for spawned claude sessions.
const DEFAULT_TERM_ROWS: u16 = 24;
const DEFAULT_TERM_COLS: u16 = 220;

/// Capacity for the broadcast output channel (bytes chunks from PTY master to all subscribers).
const OUTPUT_BROADCAST_CAPACITY: usize = 256;

/// Rolling capture of PTY output for replay to late-connecting subscribers.
///
/// Broadcast channels only buffer for *current* subscribers — any output emitted before the first
/// `stream_terminal_output` call is silently dropped (send returns `Err` with 0 receivers).
/// The capture buffer accumulates all raw bytes from session start so that when a subscriber
/// later connects it can replay the full screen state from the beginning.
const CAPTURE_LIMIT_BYTES: usize = 65536; // 64 KB

/// Handle to a running claude CLI process in a PTY.
pub struct PtyHandle {
    pub worktree_path: PathBuf,
    pub model: String,
    /// Send bytes to the child process via PTY master (stdin).
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    /// Subscribe to bytes from the child process via PTY master (stdout+stderr combined).
    pub stdout_tx: broadcast::Sender<Bytes>,
    /// Rolling capture of all PTY output since session start, for replay to late subscribers.
    pub capture: Arc<std::sync::Mutex<Vec<u8>>>,
    /// PID of the spawned process.
    pub pid: u32,
    /// PTY master — kept alive for the session's lifetime to avoid SIGHUP; also allows resize.
    master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    /// Becomes true (and sender drops) when the PTY reader thread exits — signals no more output.
    pub pty_done: watch::Receiver<bool>,
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
        let capture = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

        let session_id_owned = session_id.to_string();
        let model_owned = model.to_string();
        let binary_owned = binary_path.to_string();
        let worktree_clone = worktree_path.clone();
        let stdout_tx_clone = stdout_tx.clone();
        let capture_clone = Arc::clone(&capture);
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
                capture_clone,
                reg,
            );
            let _ = result_tx.send(res);
        });

        let (pid, master, pty_done) = result_rx
            .await
            .map_err(|_| anyhow::anyhow!("PTY spawn thread did not respond"))??;

        let handle = Arc::new(PtyHandle {
            worktree_path,
            model: model.to_string(),
            stdin_tx,
            stdout_tx,
            capture,
            pid,
            master,
            pty_done,
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
        capture: Arc<std::sync::Mutex<Vec<u8>>>,
        reg: Arc<RwLock<HashMap<String, Arc<PtyHandle>>>>,
    ) -> anyhow::Result<(u32, Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>, watch::Receiver<bool>)> {
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
        // Ensure the child sees a proper terminal type for TUI rendering.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

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

        // Watch channel: reader thread holds the sender; drops it on exit so receivers know
        // the PTY is done and no more output will arrive.
        let (pty_done_tx, pty_done_rx) = watch::channel(false);

        // Reader thread: PTY master → capture buffer + broadcast channel.
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
                                // Append to the capture ring (trimmed to CAPTURE_LIMIT_BYTES)
                                // so late-connecting subscribers can replay all output so far.
                                if let Ok(mut cap) = capture.lock() {
                                    cap.extend_from_slice(&buf[..n]);
                                    if cap.len() > CAPTURE_LIMIT_BYTES {
                                        let excess = cap.len() - CAPTURE_LIMIT_BYTES;
                                        cap.drain(0..excess);
                                    }
                                }
                                let chunk = Bytes::copy_from_slice(&buf[..n]);
                                let _ = stdout_tx_reader.send(chunk);
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            // Signal that no more output will be produced. Dropping the sender is equivalent
            // to sending `true` — receivers see the channel closed.
            drop(pty_done_tx);
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

        Ok((pid, master, pty_done_rx))
    }
}

// ---------------------------------------------------------------------------
// LiveKit bridge: expose a PtyHandle as a LiveKit RPC server
// ---------------------------------------------------------------------------

/// LiveKit RPC service that bridges `terminal.TerminalService/StreamTerminalIO` to a PTY handle.
struct PtyLiveKitService {
    handle: Arc<PtyHandle>,
}

#[async_trait]
impl RpcService for PtyLiveKitService {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "terminal.TerminalService" && method == "StreamTerminalIO"
    }

    async fn handle_rpc(&self, _service: &str, _method: &str, _msg: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(tddy_rpc::Status::unimplemented("use bidi stream")))
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, tddy_rpc::Status> {
        if service != "terminal.TerminalService" || method != "StreamTerminalIO" {
            return Err(tddy_rpc::Status::not_found(format!("{}/{}", service, method)));
        }

        let (out_tx, out_rx) = mpsc::channel::<Result<Vec<u8>, tddy_rpc::Status>>(256);

        // Replay capture buffer so the client sees all output since session start.
        if let Ok(cap) = self.handle.capture.lock() {
            if !cap.is_empty() {
                let frame = TerminalOutput { data: cap.clone() }.encode_to_vec();
                let _ = out_tx.try_send(Ok(frame));
            }
        }

        // PTY stdout → bidi output stream. Breaks when:
        // - broadcast sender closed (session dropped)
        // - pty_done watch fires (reader exited; no more output coming)
        // - out_tx_clone send fails (client disconnected)
        let mut stdout_rx = self.handle.stdout_tx.subscribe();
        let mut pty_done = self.handle.pty_done.clone();
        let out_tx_clone = out_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = stdout_rx.recv() => {
                        match result {
                            Ok(bytes) => {
                                let frame = TerminalOutput { data: bytes.to_vec() }.encode_to_vec();
                                if out_tx_clone.send(Ok(frame)).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                log::warn!(
                                    target: "tddy_daemon::claude_cli_session",
                                    "LiveKit bridge: PTY output lagged {} messages",
                                    n
                                );
                            }
                        }
                    }
                    // PTY reader exited — drain any remaining broadcast items then stop.
                    _ = pty_done.changed() => break,
                }
            }
        });

        // Bidi input stream → PTY stdin.
        let stdin_tx = self.handle.stdin_tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = input_rx.recv().await {
                if let Ok(input) = TerminalInput::decode(&msg.payload[..]) {
                    if !input.data.is_empty() {
                        let _ = stdin_tx.send(Bytes::from(input.data));
                    }
                }
            }
        });

        // Trigger a SIGWINCH so the TUI repaints for the new subscriber.
        self.handle.trigger_redraw();

        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(out_rx),
        })
    }
}

/// Spawn a LiveKit participant that bridges a PTY session to the LiveKit room.
///
/// Returns the identity string used (`daemon-<instance_id>-<session_id>` or
/// `daemon-<session_id>`), which the caller should return in `StartSessionResponse`.
pub async fn spawn_livekit_bridge(
    handle: Arc<PtyHandle>,
    livekit_url: &str,
    room_name: &str,
    api_key: &str,
    api_secret: &str,
    server_identity: &str,
) -> anyhow::Result<()> {
    let token = TokenGenerator::new(
        api_key.to_string(),
        api_secret.to_string(),
        room_name.to_string(),
        server_identity.to_string(),
        std::time::Duration::from_secs(86400),
    )
    .generate()
    .map_err(|e| anyhow::anyhow!("token generate: {}", e))?;

    let service = PtyLiveKitService { handle };
    let participant =
        LiveKitParticipant::connect(livekit_url, &token, service, Default::default(), None, None)
            .await
            .map_err(|e| anyhow::anyhow!("LiveKitParticipant::connect: {}", e))?;

    let identity_owned = server_identity.to_string();
    tokio::spawn(async move {
        participant.run().await;
        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "LiveKit bridge participant exited for identity {}",
            identity_owned
        );
    });

    Ok(())
}
