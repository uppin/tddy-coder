//! Cancellation integration tests using real child processes.
//!
//! Tests follow the signal_session.rs pattern: spawn a real `sleep` child, then verify
//! that cancellation via TaskRegistry sends SIGINT and the child dies.
//!
//! Uses `serial_test::serial` to avoid racing on child-PID liveness probes.

#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serial_test::serial;
use tddy_task::{
    ChannelKind, TaskBody, TaskChannel, TaskContext, TaskId, TaskRegistry, TaskStatus,
};
use tokio::process::Command;

/// Verify that a given PID is no longer alive using `kill -0`.
/// Returns true if the process is dead.
fn is_dead(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    rc != 0
}

/// Wait up to `max_wait` for a PID to die (polls every 50ms).
async fn wait_dead(pid: u32, max_wait: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        if is_dead(pid) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A task body that spawns `sleep 60`, registers its PID, then awaits cancellation.
/// On cancel: sends SIGINT to the child, waits, returns Cancelled.
struct SleepBody;

#[async_trait]
impl TaskBody for SleepBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        // Given a real child process
        let mut child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id().expect("child has PID");
        ctx.register_child_pid(pid);

        // Wait for cancellation
        ctx.cancel_token().cancelled().await;

        // On cancel: SIGINT the child
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
        ctx.deregister_child_pid(pid);
        // Wait for child to die (up to 2s)
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        TaskStatus::Cancelled
    }
}

/// A task body that spawns a child which ignores SIGINT and SIGTERM (traps both).
/// Used to exercise the registry-level SIGKILL escalation safety net.
///
/// The body tries to be cooperative (sends SIGINT on cancel) but the child ignores it.
/// The body then awaits `child.wait()` — which only resolves once the safety-net SIGKILL
/// kills the child — allowing tokio to reap the zombie so `is_dead()` is reliable.
struct TrapBody;

#[async_trait]
impl TaskBody for TrapBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        // Spawn a shell that traps INT and TERM so graceful cancellation has no effect.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("trap '' INT TERM; sleep 60")
            .spawn()
            .expect("spawn trapped sleep");
        let pid = child.id().expect("child has PID");
        ctx.register_child_pid(pid);

        // Await cancellation signal.
        ctx.cancel_token().cancelled().await;

        // Try SIGINT — the child ignores it (traps both INT and TERM).
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };

        // Wait for the child to die. Since the child ignores SIGINT, only the registry
        // safety-net SIGKILL can unblock this. This await call also allows tokio to
        // reap the zombie when the child eventually dies.
        let _ = child.wait().await;
        ctx.deregister_child_pid(pid);
        TaskStatus::Cancelled
    }
}

/// A task body that spawns two `sleep 60` children and registers both PIDs.
struct TwoChildrenBody;

#[async_trait]
impl TaskBody for TwoChildrenBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let mut child_a = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep a");
        let mut child_b = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep b");
        let pid_a = child_a.id().expect("pid a");
        let pid_b = child_b.id().expect("pid b");
        ctx.register_child_pid(pid_a);
        ctx.register_child_pid(pid_b);

        ctx.cancel_token().cancelled().await;

        // SIGINT both
        unsafe { libc::kill(pid_a as libc::pid_t, libc::SIGINT) };
        unsafe { libc::kill(pid_b as libc::pid_t, libc::SIGINT) };
        ctx.deregister_child_pid(pid_a);
        ctx.deregister_child_pid(pid_b);
        let _ = tokio::time::timeout(Duration::from_secs(2), child_a.wait()).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), child_b.wait()).await;
        TaskStatus::Cancelled
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Acceptance: CancelTask sends SIGINT to the registered child and the child dies.
#[tokio::test]
#[serial]
async fn cancel_sends_sigint_to_registered_child() {
    // Given — a running task that holds a real sleep child
    let registry = TaskRegistry::new();
    let handle = registry.spawn(SleepBody, "test", "session", vec![]).await;

    // Let the body start and register the PID.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let pids: Vec<u32> = handle.pid_slot.lock().unwrap().clone();
    assert!(!pids.is_empty(), "body must have registered a child PID");
    let child_pid = pids[0];

    // Verify child is alive before cancel
    assert!(!is_dead(child_pid), "child must be alive before cancel");

    // When
    registry.cancel_task(&handle.id).await;

    // Then — child must die
    let died = wait_dead(child_pid, Duration::from_secs(5)).await;
    assert!(
        died,
        "child process {child_pid} must die after cancel sends SIGINT"
    );

    // And task status must be Cancelled
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
        "task must report Cancelled after cancel"
    );
}

/// Acceptance: registry escalation sends SIGKILL to a child that traps SIGINT and SIGTERM.
#[tokio::test]
#[serial]
async fn escalation_sigkills_unresponsive_child() {
    // Given — a task that spawns a child trapping INT+TERM and a stalled body
    let registry = TaskRegistry::new();
    let handle = registry.spawn(TrapBody, "test", "session", vec![]).await;

    // Let the body start and register the PID.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let pids: Vec<u32> = handle.pid_slot.lock().unwrap().clone();
    assert!(!pids.is_empty(), "body must have registered a child PID");
    let child_pid = pids[0];

    // Verify child alive
    assert!(
        !is_dead(child_pid),
        "trapped child must be alive before cancel"
    );

    // When — cancel triggers escalation safety net
    registry.cancel_task(&handle.id).await;

    // Then — child must die within the escalation window (~5s grace + ~2s SIGTERM + SIGKILL + buffer)
    // The body awaits child.wait() so tokio reaps the zombie when SIGKILL fires.
    let died = wait_dead(child_pid, Duration::from_secs(15)).await;
    assert!(
        died,
        "escalation must SIGKILL unresponsive child {child_pid}"
    );
}

/// Acceptance: all child PIDs from a multi-child task receive SIGINT on cancel.
#[tokio::test]
#[serial]
async fn multiple_children_all_signaled() {
    // Given — a task with two real sleep children
    let registry = TaskRegistry::new();
    let handle = registry
        .spawn(TwoChildrenBody, "test", "session", vec![])
        .await;

    // Let the body start and register both PIDs.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let pids: Vec<u32> = handle.pid_slot.lock().unwrap().clone();
    assert_eq!(pids.len(), 2, "body must register exactly 2 child PIDs");
    let pid_a = pids[0];
    let pid_b = pids[1];

    assert!(!is_dead(pid_a), "child A must be alive before cancel");
    assert!(!is_dead(pid_b), "child B must be alive before cancel");

    // When
    registry.cancel_task(&handle.id).await;

    // Then — both children die
    let a_dead = wait_dead(pid_a, Duration::from_secs(5)).await;
    let b_dead = wait_dead(pid_b, Duration::from_secs(5)).await;
    assert!(a_dead, "child A ({pid_a}) must die after cancel");
    assert!(b_dead, "child B ({pid_b}) must die after cancel");
}
