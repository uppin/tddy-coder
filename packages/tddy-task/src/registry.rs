//! Task registry: stores, spawns, and manages the lifecycle of background tasks.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, RwLock};

use crate::task::{TaskBody, TaskContext, TaskHandle, TaskId, TaskStatus};

/// Maximum number of terminal tasks retained before evicting oldest-first.
const TERMINAL_TASK_CAP: usize = 200;
/// TTL for terminal tasks before they are automatically evicted.
const TERMINAL_TASK_TTL: Duration = Duration::from_secs(5 * 60);

/// Thread-safe registry of background tasks.
///
/// Unlike `ShellJobRegistry`, the `TaskRegistry` supports:
/// - `list()` — enumerate all tasks.
/// - Per-task PID tracking for cancellation.
/// - `CancellationToken`-based cooperative cancellation.
/// - Retention/cap policy for terminal tasks.
#[derive(Clone)]
pub struct TaskRegistry {
    tasks: Arc<RwLock<HashMap<TaskId, Arc<TaskHandle>>>>,
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a task handle that was created externally.
    pub async fn register(&self, handle: Arc<TaskHandle>) {
        self.tasks.write().await.insert(handle.id.clone(), handle);
    }

    /// Look up a task by ID.
    pub async fn get(&self, id: &TaskId) -> Option<Arc<TaskHandle>> {
        self.tasks.read().await.get(id).cloned()
    }

    /// Look up a task by its string ID.
    pub async fn get_by_str(&self, id: &str) -> Option<Arc<TaskHandle>> {
        self.get(&TaskId(id.to_string())).await
    }

    /// Remove a task from the registry.
    pub async fn remove(&self, id: &TaskId) {
        self.tasks.write().await.remove(id);
    }

    /// Return a snapshot of all currently registered tasks.
    pub async fn list(&self) -> Vec<Arc<TaskHandle>> {
        self.tasks.read().await.values().cloned().collect()
    }

    /// Spawn a task body and register the resulting `TaskHandle`.
    ///
    /// The handle is inserted **before** the body starts (so a racing `WatchTask` never
    /// misses the registration — mirroring the `ClaudeCliSessionManager` pattern at
    /// `claude_cli_session.rs:223`).
    ///
    /// Returns the `Arc<TaskHandle>` so callers can embed the `task_id` in their response.
    pub async fn spawn<B: TaskBody>(
        &self,
        body: B,
        kind: impl Into<String>,
        session_id: impl Into<String>,
        channels: Vec<Arc<crate::task::TaskChannel>>,
    ) -> Arc<TaskHandle> {
        let id = TaskId::new();
        let (handle, status_rx) = TaskHandle::new(id, session_id.into(), kind.into(), channels);
        self.tasks
            .write()
            .await
            .insert(handle.id.clone(), Arc::clone(&handle));

        // Spawn the body and the exit-monitor.
        let handle_for_body = Arc::clone(&handle);
        let handle_for_monitor = Arc::clone(&handle);
        let registry_for_monitor = self.clone();

        tokio::spawn(async move {
            handle_for_body.set_running();
            let ctx = TaskContext::new(Arc::clone(&handle_for_body));
            let terminal = Box::new(body).run(ctx).await;
            handle_for_body.set_terminal(terminal);
        });

        tokio::spawn(exit_monitor(
            handle_for_monitor,
            status_rx,
            registry_for_monitor,
        ));

        handle
    }

    /// Register a task that has already reached a terminal state (e.g., a synchronous tool call
    /// run inline and wrapped as a task for observability).
    ///
    /// Like `spawn`, inserts the handle and launches an `exit_monitor` so the retention/cap
    /// policy (5-min TTL + 200-task cap) still applies.
    pub(crate) async fn register_terminal(
        &self,
        handle: Arc<TaskHandle>,
        status_rx: watch::Receiver<TaskStatus>,
    ) {
        self.tasks
            .write()
            .await
            .insert(handle.id.clone(), Arc::clone(&handle));
        tokio::spawn(exit_monitor(handle, status_rx, self.clone()));
    }

    /// Create and register a terminal task for an already-completed synchronous operation.
    ///
    /// Returns the new `TaskId`. Callers (e.g. `execute_tool`) use this to record every tool
    /// invocation in the registry without spawning a separate tokio task.
    pub async fn create_terminal_task(
        &self,
        kind: impl Into<String>,
        session_id: impl Into<String>,
        result_json: Option<String>,
        terminal: TaskStatus,
    ) -> TaskId {
        let task_id = TaskId::new();
        let (handle, status_rx) =
            TaskHandle::new(task_id.clone(), session_id.into(), kind.into(), vec![]);
        handle.set_running();
        if let Some(json) = result_json {
            *handle.result_json.lock().unwrap() = Some(json);
        }
        handle.set_terminal(terminal);
        self.register_terminal(handle, status_rx).await;
        task_id
    }

    /// Request cooperative cancellation of a task.
    ///
    /// Signals the task's `CancellationToken`. If the body does not terminate within the
    /// grace period, the safety-net escalation sends SIGTERM then SIGKILL to all registered
    /// child PIDs.
    pub async fn cancel_task(&self, id: &TaskId) -> bool {
        let handle = match self.get(id).await {
            Some(h) => h,
            None => return false,
        };
        handle.cancel.cancel();
        // Spawn the escalation safety net.
        let handle_clone = Arc::clone(&handle);
        tokio::spawn(async move {
            escalation_safety_net(handle_clone).await;
        });
        true
    }

    /// Evict the oldest terminal tasks beyond the cap.
    async fn evict_over_cap(&self) {
        let mut tasks = self.tasks.write().await;
        // Collect terminal task IDs with their creation timestamps.
        let mut terminal: Vec<(u64, TaskId)> = tasks
            .values()
            .filter(|h| h.status().is_terminal())
            .map(|h| (h.created_unix_ms, h.id.clone()))
            .collect();
        if terminal.len() <= TERMINAL_TASK_CAP {
            return;
        }
        // Sort oldest-first and remove the excess.
        terminal.sort_by_key(|(ts, _)| *ts);
        let excess = terminal.len() - TERMINAL_TASK_CAP;
        for (_, id) in terminal.into_iter().take(excess) {
            tasks.remove(&id);
        }
    }
}

/// Monitors a task's status watch and, once terminal:
/// 1. Applies the TTL retention timer.
/// 2. Evicts the task (and over-cap siblings) after the TTL expires.
async fn exit_monitor(
    handle: Arc<TaskHandle>,
    mut status_rx: watch::Receiver<TaskStatus>,
    registry: TaskRegistry,
) {
    // Wait until terminal.
    loop {
        if status_rx.borrow().is_terminal() {
            break;
        }
        if status_rx.changed().await.is_err() {
            break;
        }
    }

    // Trigger cap eviction immediately on completion (a new terminal task may push us over).
    registry.evict_over_cap().await;

    // Wait the TTL, then remove.
    tokio::time::sleep(TERMINAL_TASK_TTL).await;
    registry.remove(&handle.id).await;
}

/// Safety-net escalation: if the task body does not report terminal within the grace period
/// after cancellation, send SIGTERM then SIGKILL to all registered child PIDs.
async fn escalation_safety_net(handle: Arc<TaskHandle>) {
    const GRACE: Duration = Duration::from_secs(5);

    // Wait GRACE for the body to report terminal on its own.
    let grace_end = tokio::time::Instant::now() + GRACE;
    loop {
        if handle.status().is_terminal() {
            return; // Body handled it cleanly.
        }
        if tokio::time::Instant::now() >= grace_end {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Body did not terminate — escalate.
    let pids: Vec<u32> = handle.pid_slot.lock().unwrap().clone();
    if pids.is_empty() {
        return;
    }

    #[cfg(unix)]
    {
        // SIGTERM
        for &pid in &pids {
            unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        }

        // Wait another 2s for SIGTERM to take effect.
        tokio::time::sleep(Duration::from_secs(2)).await;

        // SIGKILL if still running.
        if !handle.status().is_terminal() {
            for &pid in &pids {
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{ChannelKind, TaskChannel, TaskStatus};
    use async_trait::async_trait;

    struct ImmediateBody {
        result: TaskStatus,
    }

    #[async_trait]
    impl TaskBody for ImmediateBody {
        async fn run(self: Box<Self>, _ctx: TaskContext) -> TaskStatus {
            self.result
        }
    }

    struct NeverCancelBody;

    #[async_trait]
    impl TaskBody for NeverCancelBody {
        async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
            ctx.cancel_token().cancelled().await;
            TaskStatus::Cancelled
        }
    }

    #[tokio::test]
    async fn register_then_list_shows_task() {
        // Given
        let registry = TaskRegistry::new();
        let id = TaskId::new();
        let (handle, _rx) = TaskHandle::new(
            id.clone(),
            "test-session".to_string(),
            "test".to_string(),
            vec![],
        );

        // When
        registry.register(Arc::clone(&handle)).await;
        let listed = registry.list().await;

        // Then
        assert_eq!(listed.len(), 1, "list must show the registered task");
        assert_eq!(listed[0].id, id);
    }

    #[tokio::test]
    async fn status_transitions_pending_running_completed() {
        // Given
        let registry = TaskRegistry::new();
        let body = ImmediateBody {
            result: TaskStatus::Completed { exit_code: Some(0) },
        };

        // When
        let handle = registry.spawn(body, "test", "session", vec![]).await;

        // Then — wait for terminal
        let mut rx = handle.status_watch();
        loop {
            if rx.borrow().is_terminal() {
                break;
            }
            if rx.changed().await.is_err() {
                break;
            }
        }
        assert!(
            matches!(
                handle.status(),
                TaskStatus::Completed { exit_code: Some(0) }
            ),
            "must complete with exit_code 0"
        );
    }

    #[tokio::test]
    async fn channel_replay_then_live() {
        // Given — a channel that has already received some output
        let ch = TaskChannel::output_only("0", "combined", ChannelKind::Combined);
        ch.write(bytes::Bytes::from("hello"));

        // When — subscribe *after* the write
        let replay = ch.replay_capture();
        let mut rx = ch.subscribe();
        ch.write(bytes::Bytes::from(" world"));

        // Then — replay contains the earlier bytes, rx gets the later bytes
        assert_eq!(
            &replay, b"hello",
            "replay must contain pre-subscription bytes"
        );
        let live = rx.recv().await.expect("live recv");
        assert_eq!(
            &live[..],
            b" world",
            "live stream must deliver post-subscription bytes"
        );
    }

    #[tokio::test]
    async fn cancel_signals_body_and_reaches_cancelled() {
        // Given
        let registry = TaskRegistry::new();
        let handle = registry
            .spawn(NeverCancelBody, "test", "session", vec![])
            .await;

        // When
        registry.cancel_task(&handle.id).await;

        // Then
        let mut rx = handle.status_watch();
        loop {
            if rx.borrow().is_terminal() {
                break;
            }
            if rx.changed().await.is_err() {
                break;
            }
        }
        assert_eq!(
            handle.status(),
            TaskStatus::Cancelled,
            "body must report Cancelled after cancel signal"
        );
    }

    #[tokio::test]
    async fn retention_caps_terminal_count() {
        // Given — spawn more than TERMINAL_TASK_CAP instant tasks
        let registry = TaskRegistry::new();
        let over = TERMINAL_TASK_CAP + 10;
        for _ in 0..over {
            let body = ImmediateBody {
                result: TaskStatus::Completed { exit_code: Some(0) },
            };
            let handle = registry.spawn(body, "test", "s", vec![]).await;
            // Wait for terminal before spawning next to allow cap eviction.
            let mut rx = handle.status_watch();
            loop {
                if rx.borrow().is_terminal() {
                    break;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
            registry.evict_over_cap().await;
        }

        // Then
        let count = registry.list().await.len();
        assert!(
            count <= TERMINAL_TASK_CAP,
            "registry must not exceed cap of {TERMINAL_TASK_CAP}; got {count}"
        );
    }
}
