//! Core task types: TaskId, TaskStatus, TaskChannel, TaskHandle, TaskBody, TaskContext.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::terminal_capture::TerminalCapture;

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

/// Monotonic accumulator for the highest input byte offset applied to a channel's stdin.
///
/// Backs the terminal `SendTerminalInput` → `StreamTerminalOutput` acknowledgement: input carries a
/// cumulative byte `input_offset`; once applied, that offset is recorded here. [`record`](Self::record)
/// keeps the running maximum and returns whether the value advanced (so a caller only publishes an
/// ACK when there is something new to confirm); [`get`](Self::get) reads the current maximum.
/// Interior mutability (`AtomicU64`) plus the max semantics make it safe to share across the
/// ephemeral handles rebuilt per RPC call and tolerant of out-of-order input.
#[derive(Debug, Default)]
pub struct AppliedOffset {
    value: std::sync::atomic::AtomicU64,
}

impl AppliedOffset {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `offset`, keeping the maximum. Returns `true` iff the stored value increased.
    pub fn record(&self, offset: u64) -> bool {
        use std::sync::atomic::Ordering;
        let mut cur = self.value.load(Ordering::Relaxed);
        loop {
            if offset <= cur {
                return false;
            }
            match self
                .value
                .compare_exchange_weak(cur, offset, Ordering::AcqRel, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => cur = actual,
            }
        }
    }

    /// The current running maximum (0 before any input is applied).
    pub fn get(&self) -> u64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }
}

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
    capture: Arc<Mutex<TerminalCapture>>,
    stdin_tx: Option<mpsc::UnboundedSender<Bytes>>,
    /// Highest input offset applied to this channel's stdin (monotonic); the ACK source shared by
    /// every `PtyHandle` rebuilt for this terminal.
    applied_offset: Arc<AppliedOffset>,
    /// Publishes the applied offset to `StreamTerminalOutput` subscribers as it advances.
    acked_offset_tx: watch::Sender<u64>,
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
            capture: Arc::new(Mutex::new(TerminalCapture::new())),
            stdin_tx: Some(stdin_tx),
            applied_offset: Arc::new(AppliedOffset::new()),
            acked_offset_tx: watch::channel(0u64).0,
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
            capture: Arc::new(Mutex::new(TerminalCapture::new())),
            stdin_tx: None,
            applied_offset: Arc::new(AppliedOffset::new()),
            acked_offset_tx: watch::channel(0u64).0,
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

    /// The output the process produced, as it produced it (bounded to
    /// [`TerminalCapture::CAPTURE_LIMIT_BYTES`]).
    ///
    /// Callers rendering this into a terminal want [`TerminalCapture::replay`] via
    /// [`Self::capture_arc`] instead: it prefixes the terminal modes still in effect, which a
    /// late subscriber's own VT would otherwise never learn about.
    pub fn replay_capture(&self) -> Vec<u8> {
        self.capture.lock().unwrap().buffered_bytes().to_vec()
    }

    /// Write bytes to the channel, appending to the replay buffer and broadcasting to subscribers.
    pub fn write(&self, data: Bytes) {
        self.capture.lock().unwrap().append(&data);
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
    pub fn capture_arc(&self) -> Arc<Mutex<TerminalCapture>> {
        Arc::clone(&self.capture)
    }

    /// Record that input up to cumulative byte `offset` has been applied to stdin, publishing the
    /// new maximum to `subscribe_acked_offset` observers when it advances. Returns whether it
    /// advanced. Shared across every `PtyHandle` rebuilt for this terminal, so an ACK published by
    /// the input path is seen by an already-open output stream.
    pub fn acknowledge_input(&self, offset: u64) -> bool {
        if self.applied_offset.record(offset) {
            // `send_replace` (not `send`) so the latest applied offset is retained even with no
            // current subscriber — a stream that opens later reads it as its initial watch value.
            self.acked_offset_tx.send_replace(self.applied_offset.get());
            true
        } else {
            false
        }
    }

    /// Subscribe to applied-input-offset changes — the ACK source for `StreamTerminalOutput`.
    /// The receiver's initial value is the current applied offset.
    pub fn subscribe_acked_offset(&self) -> watch::Receiver<u64> {
        self.acked_offset_tx.subscribe()
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

#[cfg(test)]
mod applied_offset_tests {
    use super::*;

    #[test]
    fn records_a_higher_offset_and_reports_that_it_advanced() {
        // Given
        let applied = AppliedOffset::new();

        // When
        let advanced = applied.record(42);

        // Then
        assert!(
            advanced,
            "a first, higher offset must report that it advanced"
        );
        assert_eq!(applied.get(), 42);
    }

    #[test]
    fn keeps_the_maximum_and_reports_no_advance_for_a_lower_offset() {
        // Given — a higher offset already applied
        let applied = AppliedOffset::new();
        applied.record(100);

        // When — a later, smaller offset arrives
        let advanced = applied.record(50);

        // Then — the applied offset does not regress and no ACK should be published
        assert!(!advanced, "a lower offset must not report an advance");
        assert_eq!(
            applied.get(),
            100,
            "applied offset must be the running maximum"
        );
    }

    #[test]
    fn starts_at_zero() {
        assert_eq!(AppliedOffset::new().get(), 0);
    }
}

#[cfg(test)]
mod channel_ack_tests {
    use super::*;

    #[test]
    fn acknowledge_input_publishes_the_applied_offset_to_a_subscriber() {
        // Given — a PTY channel with an output stream already subscribed to acks
        let (channel, _stdin_rx) = TaskChannel::pty("0", "pty");
        let mut acked = channel.subscribe_acked_offset();
        assert_eq!(*acked.borrow_and_update(), 0);

        // When — input up to offset 42 is applied
        let advanced = channel.acknowledge_input(42);

        // Then — the subscriber observes the acknowledged offset
        assert!(advanced);
        assert_eq!(*acked.borrow(), 42);
    }

    #[test]
    fn acknowledge_input_never_lowers_the_published_offset() {
        // Given
        let (channel, _stdin_rx) = TaskChannel::pty("0", "pty");
        channel.acknowledge_input(100);

        // When — a later, smaller offset arrives
        let advanced = channel.acknowledge_input(50);

        // Then — the published offset stays at the maximum
        assert!(!advanced);
        assert_eq!(*channel.subscribe_acked_offset().borrow(), 100);
    }
}
