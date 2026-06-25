//! Task registry: stores, spawns, and manages the lifecycle of background tasks.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, watch, RwLock};

use crate::task::{TaskBody, TaskContext, TaskHandle, TaskId, TaskStatus};

/// Maximum number of terminal tasks retained before evicting oldest-first.
const TERMINAL_TASK_CAP: usize = 200;
/// TTL for terminal tasks before they are automatically evicted.
const TERMINAL_TASK_TTL: Duration = Duration::from_secs(5 * 60);

/// Event broadcast on task lifecycle changes in the registry.
#[derive(Clone)]
pub enum TaskRegistryEvent {
    Added(Arc<TaskHandle>),
    Updated(Arc<TaskHandle>),
    Removed(TaskId),
}

impl std::fmt::Debug for TaskRegistryEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskRegistryEvent::Added(h) => write!(f, "Added({:?})", h.id),
            TaskRegistryEvent::Updated(h) => write!(f, "Updated({:?})", h.id),
            TaskRegistryEvent::Removed(id) => write!(f, "Removed({:?})", id),
        }
    }
}

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
    task_events: Arc<broadcast::Sender<TaskRegistryEvent>>,
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            task_events: Arc::new(tx),
        }
    }

    /// Register a task handle that was created externally.
    pub async fn register(&self, handle: Arc<TaskHandle>) {
        self.tasks
            .write()
            .await
            .insert(handle.id.clone(), Arc::clone(&handle));
        let _ = self.task_events.send(TaskRegistryEvent::Added(handle));
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
        let _ = self
            .task_events
            .send(TaskRegistryEvent::Removed(id.clone()));
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
        let _ = self
            .task_events
            .send(TaskRegistryEvent::Added(Arc::clone(&handle)));

        // Spawn the body and the exit-monitor.
        let handle_for_body = Arc::clone(&handle);
        let handle_for_monitor = Arc::clone(&handle);
        let registry_for_monitor = self.clone();
        let event_tx = Arc::clone(&self.task_events);

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
            event_tx,
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
        let _ = self
            .task_events
            .send(TaskRegistryEvent::Added(Arc::clone(&handle)));
        tokio::spawn(exit_monitor(
            handle,
            status_rx,
            self.clone(),
            Arc::clone(&self.task_events),
        ));
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
        let evicted = {
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
            let to_evict: Vec<TaskId> = terminal
                .into_iter()
                .take(excess)
                .map(|(_, id)| id)
                .collect();
            for id in &to_evict {
                tasks.remove(id);
            }
            to_evict
        };
        for id in evicted {
            let _ = self.task_events.send(TaskRegistryEvent::Removed(id));
        }
    }

    /// Subscribe to task list events (added / updated / removed).
    pub fn subscribe_list(&self) -> broadcast::Receiver<TaskRegistryEvent> {
        self.task_events.subscribe()
    }

    /// Subscribe to task list events and atomically capture the current snapshot.
    ///
    /// Subscribes BEFORE reading the snapshot so no events are lost for tasks
    /// spawned concurrently.
    pub async fn list_and_subscribe(
        &self,
    ) -> (Vec<Arc<TaskHandle>>, broadcast::Receiver<TaskRegistryEvent>) {
        let rx = self.task_events.subscribe();
        let snapshot = self.list().await;
        (snapshot, rx)
    }
}

/// Monitors a task's status watch and, once terminal:
/// 1. Applies the TTL retention timer.
/// 2. Evicts the task (and over-cap siblings) after the TTL expires.
async fn exit_monitor(
    handle: Arc<TaskHandle>,
    mut status_rx: watch::Receiver<TaskStatus>,
    registry: TaskRegistry,
    event_tx: Arc<broadcast::Sender<TaskRegistryEvent>>,
) {
    // Wait until terminal, emitting Updated on each status change.
    loop {
        if status_rx.borrow().is_terminal() {
            break;
        }
        if status_rx.changed().await.is_err() {
            break;
        }
        let _ = event_tx.send(TaskRegistryEvent::Updated(Arc::clone(&handle)));
    }

    // Emit a final Updated for the terminal status.
    let _ = event_tx.send(TaskRegistryEvent::Updated(Arc::clone(&handle)));

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

    // ── Test bodies ──────────────────────────────────────────────────────────

    struct CompletingBody;

    #[async_trait]
    impl TaskBody for CompletingBody {
        async fn run(self: Box<Self>, _ctx: TaskContext) -> TaskStatus {
            TaskStatus::Completed { exit_code: Some(0) }
        }
    }

    struct WaitForCancelBody;

    #[async_trait]
    impl TaskBody for WaitForCancelBody {
        async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
            ctx.cancel_token().cancelled().await;
            TaskStatus::Cancelled
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Block until the handle reaches any terminal status.
    async fn wait_terminal(handle: &TaskHandle) {
        let mut rx = handle.status_watch();
        loop {
            if rx.borrow().is_terminal() {
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    /// Spawn `n` instant-completing tasks and drain each to terminal before returning.
    /// Used to exercise the cap eviction path without a loop in the test body.
    async fn spawn_and_drain(registry: &TaskRegistry, n: usize) {
        for _ in 0..n {
            let handle = registry.spawn(CompletingBody, "test", "s", vec![]).await;
            wait_terminal(&handle).await;
            registry.evict_over_cap().await;
        }
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn registered_task_appears_in_list() {
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
    async fn spawned_body_reaches_completed_status() {
        // Given
        let registry = TaskRegistry::new();

        // When
        let handle = registry
            .spawn(CompletingBody, "test", "session", vec![])
            .await;

        // Then
        wait_terminal(&handle).await;
        assert!(
            matches!(
                handle.status(),
                TaskStatus::Completed { exit_code: Some(0) }
            ),
            "must complete with exit_code 0"
        );
    }

    #[tokio::test]
    async fn late_subscriber_receives_buffered_bytes_then_new_bytes_live() {
        // Given — a channel that already has buffered output before subscription
        let ch = TaskChannel::output_only("0", "combined", ChannelKind::Combined);
        ch.write(bytes::Bytes::from("hello"));

        // When — subscribe after the write, then write more
        let replay = ch.replay_capture();
        let mut rx = ch.subscribe();
        ch.write(bytes::Bytes::from(" world"));

        // Then — replay buffer contains the pre-subscription bytes
        assert_eq!(
            &replay, b"hello",
            "replay must contain pre-subscription bytes"
        );
        // And the live stream delivers the post-subscription byte
        let live = rx.recv().await.expect("live recv must succeed");
        assert_eq!(
            &live[..],
            b" world",
            "live stream must deliver post-subscription bytes"
        );
    }

    #[tokio::test]
    async fn cancel_signal_reaches_body_and_task_reports_cancelled() {
        // Given
        let registry = TaskRegistry::new();
        let handle = registry
            .spawn(WaitForCancelBody, "test", "session", vec![])
            .await;

        // When
        registry.cancel_task(&handle.id).await;

        // Then
        wait_terminal(&handle).await;
        assert_eq!(
            handle.status(),
            TaskStatus::Cancelled,
            "body must report Cancelled after cancel signal"
        );
    }

    #[tokio::test]
    async fn registry_evicts_oldest_tasks_when_cap_is_exceeded() {
        // Given — spawn more than TERMINAL_TASK_CAP instant tasks
        let registry = TaskRegistry::new();

        // When — fill beyond the cap, draining each task before the next spawn
        spawn_and_drain(&registry, TERMINAL_TASK_CAP + 10).await;

        // Then
        let count = registry.list().await.len();
        assert!(
            count <= TERMINAL_TASK_CAP,
            "registry must not exceed cap of {TERMINAL_TASK_CAP}; got {count}"
        );
    }

    // ── TaskRegistryEvent broadcast tests ────────────────────────────────────

    #[tokio::test]
    async fn subscribe_list_receives_added_event_on_spawn() {
        // Given
        let registry = TaskRegistry::new();
        let mut rx = registry.subscribe_list();

        // When
        let handle = registry
            .spawn(CompletingBody, "shell", "session", vec![])
            .await;

        // Then — first event must be Added with matching task_id
        let event = rx
            .recv()
            .await
            .expect("must receive Added event after spawn");
        match event {
            TaskRegistryEvent::Added(h) => {
                assert_eq!(
                    h.id, handle.id,
                    "Added event must carry the spawned task's id"
                );
            }
            other => panic!("expected Added, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn subscribe_list_receives_updated_event_on_running_transition() {
        // Given — a task that stays running until cancelled so we can observe Running transition
        let registry = TaskRegistry::new();
        let mut rx = registry.subscribe_list();

        let handle = registry
            .spawn(WaitForCancelBody, "test", "session", vec![])
            .await;

        // Drain Added event
        loop {
            let event = rx.recv().await.expect("must receive event");
            if matches!(&event, TaskRegistryEvent::Added(h) if h.id == handle.id) {
                break;
            }
        }

        // Then — an Updated event arrives with Running status
        let event = rx
            .recv()
            .await
            .expect("must receive Updated event for Running");
        match event {
            TaskRegistryEvent::Updated(h) => {
                assert_eq!(h.id, handle.id);
                assert_eq!(
                    h.status(),
                    TaskStatus::Running,
                    "Updated event after spawn must carry Running status"
                );
            }
            other => panic!("expected Updated(Running), got {:?}", other),
        }

        // Cleanup
        registry.cancel_task(&handle.id).await;
        wait_terminal(&handle).await;
    }

    #[tokio::test]
    async fn subscribe_list_receives_removed_event_on_eviction() {
        // Given — spawn a completing task and wait for it to finish, then force eviction
        let registry = TaskRegistry::new();
        let mut rx = registry.subscribe_list();

        let handle = registry
            .spawn(CompletingBody, "test", "session", vec![])
            .await;
        wait_terminal(&handle).await;

        // Force immediate removal (simulates TTL expiry or cap eviction)
        registry.remove(&handle.id).await;

        // Then — a Removed event arrives with the task_id
        loop {
            let event = rx.recv().await.expect("must receive event");
            if let TaskRegistryEvent::Removed(id) = event {
                assert_eq!(
                    id, handle.id,
                    "Removed event must carry the evicted task's id"
                );
                return;
            }
        }
    }

    #[tokio::test]
    async fn list_and_subscribe_returns_snapshot_then_live_events() {
        // Given — a task already in the registry before subscribe
        let registry = TaskRegistry::new();
        let existing = registry
            .spawn(WaitForCancelBody, "existing", "session", vec![])
            .await;
        // Wait for Running so status is stable
        let mut status_rx = existing.status_watch();
        loop {
            if matches!(*status_rx.borrow(), TaskStatus::Running) {
                break;
            }
            status_rx.changed().await.unwrap();
        }

        // When — call list_and_subscribe
        let (snapshot, mut rx) = registry.list_and_subscribe().await;

        // Then — snapshot contains the existing task
        assert!(
            snapshot.iter().any(|h| h.id == existing.id),
            "snapshot must include task that existed before subscribe"
        );

        // And — a new spawn fires an Added live event
        let new_handle = registry
            .spawn(CompletingBody, "new", "session", vec![])
            .await;

        let event = rx
            .recv()
            .await
            .expect("must receive Added event for new task");
        match event {
            TaskRegistryEvent::Added(h) => {
                assert_eq!(
                    h.id, new_handle.id,
                    "live Added event must carry new task id"
                );
            }
            other => panic!("expected Added, got {:?}", other),
        }

        // Cleanup
        registry.cancel_task(&existing.id).await;
        wait_terminal(&existing).await;
    }

    #[tokio::test]
    async fn list_and_subscribe_no_events_lost_when_spawn_races_subscribe() {
        // Verify that list_and_subscribe() subscribes BEFORE reading the snapshot,
        // so a task spawned concurrently appears in either the snapshot or the event stream.
        let registry = TaskRegistry::new();

        // Spawn a task concurrently with list_and_subscribe
        let registry_clone = registry.clone();
        let spawn_handle = tokio::spawn(async move {
            registry_clone
                .spawn(CompletingBody, "racing", "session", vec![])
                .await
        });

        let (snapshot, mut rx) = registry.list_and_subscribe().await;
        let raced_handle = spawn_handle.await.unwrap();

        // The raced task must appear in snapshot OR in the first few live events
        let in_snapshot = snapshot.iter().any(|h| h.id == raced_handle.id);
        if !in_snapshot {
            // Must appear as a live event — drain up to 10 events looking for it
            let mut found = false;
            for _ in 0..10 {
                match rx.try_recv() {
                    Ok(TaskRegistryEvent::Added(h)) if h.id == raced_handle.id => {
                        found = true;
                        break;
                    }
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
            assert!(
                found,
                "racing task must appear in snapshot or live events — no events lost"
            );
        }
    }
}
