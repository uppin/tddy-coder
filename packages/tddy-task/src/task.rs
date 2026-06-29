//! Core task types: TaskId, TaskStatus, TaskChannel, TaskHandle, TaskBody, TaskContext.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, watch};
use tokio_util::sync::CancellationToken;

/// Unique identifier for a task, formatted as a UUIDv7 string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

impl TaskId {
    /// Create a new time-ordered unique task ID.
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7().to_string())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Lifecycle status of a task.
///
/// Transitions: `Pending → Running` → one of `Completed | Failed | Cancelled`.
/// A cancel request while `Running` does not flip the status immediately; the body
/// handles its own cleanup, then reports the terminal state.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed { exit_code: Option<i32> },
    Failed { message: String },
    Cancelled,
}

impl TaskStatus {
    /// Returns true for any terminal status.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed { .. } | TaskStatus::Failed { .. } | TaskStatus::Cancelled
        )
    }
}

/// Direction/type of a task channel's output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Stdout,
    Stderr,
    /// Combined stdout+stderr on a single channel (e.g. background-Shell tool).
    Combined,
    /// PTY byte stream label (actual PTY control lives in daemon PtyRegistry).
    Pty,
}

/// Bounded broadcast capacity for task channel output.
const CHANNEL_BROADCAST_CAPACITY: usize = 256;
/// Maximum bytes retained in the replay ring buffer per channel.
const CHANNEL_CAPTURE_LIMIT_BYTES: usize = 64 * 1024;

/// A single named I/O channel on a task.
///
/// Writers push bytes into the channel; multiple observers subscribe via `subscribe()`.
/// A bounded replay buffer allows late subscribers to receive already-emitted output.
/// Optionally accepts stdin bytes via `send_input()`.
pub struct TaskChannel {
    /// Short identifier used in `WatchTask` requests, e.g. `"0"`, `"make"`, `"qemu-img"`.
    pub channel_id: String,
    /// Human-readable label, e.g. `"stdout"`, `"make output"`.
    pub name: String,
    /// Whether this channel carries stdout, stderr, or combined output.
    pub kind: ChannelKind,
    output_tx: broadcast::Sender<Bytes>,
    capture: Arc<Mutex<Vec<u8>>>,
    stdin_tx: Option<mpsc::UnboundedSender<Bytes>>,
}

impl TaskChannel {
    /// Create a new task channel. If `stdin_rx` is provided the caller must drain it.
    pub fn new(
        channel_id: impl Into<String>,
        name: impl Into<String>,
        kind: ChannelKind,
    ) -> (Arc<Self>, Option<mpsc::UnboundedReceiver<Bytes>>) {
        let (output_tx, _) = broadcast::channel(CHANNEL_BROADCAST_CAPACITY);
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
        let channel = Arc::new(Self {
            channel_id: channel_id.into(),
            name: name.into(),
            kind,
            output_tx,
            capture: Arc::new(Mutex::new(Vec::new())),
            stdin_tx: Some(stdin_tx),
        });
        (channel, Some(stdin_rx))
    }

    /// Create a new output-only channel (no stdin).
    pub fn output_only(
        channel_id: impl Into<String>,
        name: impl Into<String>,
        kind: ChannelKind,
    ) -> Arc<Self> {
        let (output_tx, _) = broadcast::channel(CHANNEL_BROADCAST_CAPACITY);
        Arc::new(Self {
            channel_id: channel_id.into(),
            name: name.into(),
            kind,
            output_tx,
            capture: Arc::new(Mutex::new(Vec::new())),
            stdin_tx: None,
        })
    }

    /// PTY channel with stdin + broadcast output (label only; master handle in daemon).
    pub fn pty(
        channel_id: impl Into<String>,
        name: impl Into<String>,
    ) -> (Arc<Self>, Option<mpsc::UnboundedReceiver<Bytes>>) {
        Self::new(channel_id, name, ChannelKind::Pty)
    }

    /// Whether this channel accepts stdin input.
    pub fn accepts_input(&self) -> bool {
        self.stdin_tx.is_some()
    }

    /// Subscribe to live output bytes. The receiver misses bytes sent before subscription;
    /// use `replay_capture()` first to replay the buffer.
    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.output_tx.subscribe()
    }

    /// A snapshot of all bytes emitted so far (bounded to `CHANNEL_CAPTURE_LIMIT_BYTES`).
    pub fn replay_capture(&self) -> Vec<u8> {
        self.capture.lock().unwrap().clone()
    }

    /// Write bytes to the channel, appending to the replay buffer and broadcasting to subscribers.
    pub fn write(&self, data: Bytes) {
        // Append to replay ring buffer (trim when over limit).
        {
            let mut buf = self.capture.lock().unwrap();
            buf.extend_from_slice(&data);
            if buf.len() > CHANNEL_CAPTURE_LIMIT_BYTES {
                let excess = buf.len() - CHANNEL_CAPTURE_LIMIT_BYTES;
                buf.drain(..excess);
            }
        }
        // Broadcast to subscribers — ignore "no receivers" errors (common for fast tasks).
        let _ = self.output_tx.send(data);
    }

    /// Send bytes to the stdin receiver (if this channel accepts input).
    /// Returns `false` if the channel has no stdin or the receiver is closed.
    pub fn send_input(&self, data: Bytes) -> bool {
        match &self.stdin_tx {
            Some(tx) => tx.send(data).is_ok(),
            None => false,
        }
    }

    /// Clone of the stdin sender (for bridging external writers to the PTY body).
    pub fn stdin_sender(&self) -> Option<mpsc::UnboundedSender<Bytes>> {
        self.stdin_tx.clone()
    }

    /// Clone of the broadcast sender (for legacy PTY subscribers).
    pub fn output_broadcast(&self) -> broadcast::Sender<Bytes> {
        self.output_tx.clone()
    }

    /// Shared capture buffer (for replay to late subscribers).
    pub fn capture_arc(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.capture)
    }
}

/// Shared, cloneable handle to a registered task.
pub struct TaskHandle {
    /// Globally unique task identifier.
    pub id: TaskId,
    /// Session that owns this task (used for auth scoping). Empty string for daemon-internal tasks.
    pub session_id: String,
    /// Human-readable kind, e.g. `"execute_tool:Read"`, `"vm_build"`, `"shell"`.
    pub kind: String,
    /// Unix millisecond timestamp of task creation.
    pub created_unix_ms: u64,

    status: Arc<Mutex<TaskStatus>>,
    status_tx: watch::Sender<TaskStatus>,
    /// All output/input channels for this task (0-N).
    channels: Vec<Arc<TaskChannel>>,
    /// Cancellation token. Cancelled by `TaskRegistry::cancel_task()`.
    pub cancel: CancellationToken,
    /// PIDs of child processes registered by the task body.
    /// Used for the registry-level SIGINT/SIGKILL escalation safety net.
    pub pid_slot: Arc<Mutex<Vec<u32>>>,
    /// Terminal result payload (e.g. serialised `ToolOutcome.result_json`).
    pub result_json: Arc<Mutex<Option<String>>>,
}

impl TaskHandle {
    pub(crate) fn new(
        id: TaskId,
        session_id: String,
        kind: String,
        channels: Vec<Arc<TaskChannel>>,
    ) -> (Arc<Self>, watch::Receiver<TaskStatus>) {
        let created_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let (status_tx, status_rx) = watch::channel(TaskStatus::Pending);
        let handle = Arc::new(Self {
            id,
            session_id,
            kind,
            created_unix_ms,
            status: Arc::new(Mutex::new(TaskStatus::Pending)),
            status_tx,
            channels,
            cancel: CancellationToken::new(),
            pid_slot: Arc::new(Mutex::new(Vec::new())),
            result_json: Arc::new(Mutex::new(None)),
        });
        (handle, status_rx)
    }

    /// Current status snapshot.
    pub fn status(&self) -> TaskStatus {
        self.status.lock().unwrap().clone()
    }

    /// Returns a receiver that yields the new `TaskStatus` on every transition.
    pub fn status_watch(&self) -> watch::Receiver<TaskStatus> {
        self.status_tx.subscribe()
    }

    /// All channels declared by this task.
    pub fn channels(&self) -> &[Arc<TaskChannel>] {
        &self.channels
    }

    /// Look up a channel by its `channel_id`.
    pub fn channel(&self, channel_id: &str) -> Option<Arc<TaskChannel>> {
        self.channels
            .iter()
            .find(|c| c.channel_id == channel_id)
            .cloned()
    }

    /// Transition to `Running`. Silently ignored if already past `Pending`.
    pub(crate) fn set_running(&self) {
        let mut s = self.status.lock().unwrap();
        if *s == TaskStatus::Pending {
            *s = TaskStatus::Running;
            let _ = self.status_tx.send(TaskStatus::Running);
        }
    }

    /// Transition to a terminal status. Silently ignored if already terminal.
    pub(crate) fn set_terminal(&self, status: TaskStatus) {
        let mut s = self.status.lock().unwrap();
        if !s.is_terminal() {
            *s = status.clone();
            let _ = self.status_tx.send(status);
        }
    }
}

/// Context passed to a task body during execution.
///
/// Exposes the cancel signal, child-PID registration, and channel writers.
pub struct TaskContext {
    handle: Arc<TaskHandle>,
}

impl TaskContext {
    pub(crate) fn new(handle: Arc<TaskHandle>) -> Self {
        Self { handle }
    }

    /// Cancellation token. Await `.cancelled()` in `tokio::select!` branches.
    pub fn cancel_token(&self) -> CancellationToken {
        self.handle.cancel.clone()
    }

    /// Returns `true` if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.handle.cancel.is_cancelled()
    }

    /// Register a child process PID so the safety-net escalation can reach it.
    pub fn register_child_pid(&self, pid: u32) {
        self.handle.pid_slot.lock().unwrap().push(pid);
    }

    /// Deregister a child PID when the child has exited cleanly.
    pub fn deregister_child_pid(&self, pid: u32) {
        self.handle.pid_slot.lock().unwrap().retain(|&p| p != pid);
    }

    /// Shared PID list for cancel escalation (clone before `spawn_blocking`).
    pub fn pid_slot(&self) -> Arc<Mutex<Vec<u32>>> {
        Arc::clone(&self.handle.pid_slot)
    }

    /// Look up a channel writer by `channel_id`.
    pub fn channel(&self, channel_id: &str) -> Option<Arc<TaskChannel>> {
        self.handle.channel(channel_id)
    }

    /// Store the terminal result payload (e.g. JSON-encoded tool output).
    pub fn set_result(&self, json: String) {
        *self.handle.result_json.lock().unwrap() = Some(json);
    }

    /// Task identifier for this execution context.
    pub fn task_id(&self) -> TaskId {
        self.handle.id.clone()
    }
}

/// Trait implemented by task authors.
///
/// The body owns its child processes and is responsible for handling its own cancellation:
/// - Await `ctx.cancel_token().cancelled()` in each `tokio::select!` wait.
/// - On cancel: send `SIGINT` to each registered child PID, await exit, return `Cancelled`.
///
/// The registry provides a safety-net escalation (SIGTERM → SIGKILL) if the body does not
/// terminate within ~5 seconds of the cancel signal.
#[async_trait]
pub trait TaskBody: Send + 'static {
    /// Execute the task. Must return a terminal `TaskStatus`.
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus;
}
