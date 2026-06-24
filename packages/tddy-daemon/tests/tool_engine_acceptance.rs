//! Acceptance tests: every execute_tool invocation registers a task in the TaskRegistry.
//!
//! Verifies the "every action is a task" invariant established in the background-tasks changeset.
//! Tests are driven directly through `execute_tool` — no RPC transport, no daemon process.

use tempfile::tempdir;
use tddy_daemon::tool_engine::execute_tool;
use tddy_task::{TaskRegistry, TaskStatus};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Call execute_tool(Read) on `filename` inside a fresh tempdir.
async fn read_file_in_registry(
    registry: &TaskRegistry,
    filename: &str,
    content: &str,
) -> tddy_daemon::tool_engine::ToolOutcome {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(filename), content).unwrap();
    execute_tool(
        dir.path(),
        "Read",
        &serde_json::json!({ "path": filename }).to_string(),
        registry,
        "test-session",
    )
    .await
}

/// Wait until the task handle reaches any terminal status.
async fn wait_terminal(handle: &tddy_task::TaskHandle) {
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn sync_tool_invocation_registers_a_completed_task() {
    // Given — a fresh registry and a readable file
    let registry = TaskRegistry::new();

    // When — Read tool is executed
    let outcome = read_file_in_registry(&registry, "hello.txt", "hello world").await;

    // Then — outcome succeeds and carries a job_id
    assert!(!outcome.is_error, "Read must succeed");
    assert!(!outcome.job_id.is_empty(), "outcome.job_id must be set");

    // And the registry has exactly one task matching that id
    let tasks = registry.list().await;
    assert_eq!(tasks.len(), 1, "Read must register exactly one task");
    let task = &tasks[0];
    assert_eq!(task.id.0, outcome.job_id);
    assert_eq!(task.kind, "execute_tool:Read");
    assert!(
        matches!(task.status(), TaskStatus::Completed { .. }),
        "sync task must be terminal immediately"
    );
}

#[tokio::test]
async fn failed_sync_tool_registers_a_failed_task() {
    // Given — a fresh registry with a worktree containing no matching file
    let dir = tempdir().unwrap();
    let registry = TaskRegistry::new();

    // When — Read on a nonexistent file
    let outcome = execute_tool(
        dir.path(),
        "Read",
        &serde_json::json!({ "path": "missing.txt" }).to_string(),
        &registry,
        "test-session",
    )
    .await;

    // Then — outcome is an error and a task is still registered
    assert!(outcome.is_error, "Read on missing file must return an error");
    assert!(!outcome.job_id.is_empty(), "even error outcomes must have a job_id");

    let tasks = registry.list().await;
    assert_eq!(tasks.len(), 1, "error Read must still register a task");
    let task = &tasks[0];
    assert!(
        matches!(task.status(), TaskStatus::Failed { .. }),
        "error outcome must register a Failed task, got {:?}",
        task.status()
    );
}

#[tokio::test]
async fn shell_background_job_registers_a_task_with_output_channel() {
    // Given — a fresh registry and a temp worktree
    let dir = tempdir().unwrap();
    let registry = TaskRegistry::new();

    // When — Shell with block_until_ms=0 (background detach)
    let outcome = execute_tool(
        dir.path(),
        "Shell",
        &serde_json::json!({ "command": "echo hello", "block_until_ms": 0 }).to_string(),
        &registry,
        "test-session",
    )
    .await;

    // Then — outcome carries job_running=true and a non-empty job_id
    assert!(outcome.job_running, "background Shell must return job_running=true");
    assert!(!outcome.job_id.is_empty(), "background Shell must return a non-empty job_id");

    // And the task is registered with a combined output channel
    let tasks = registry.list().await;
    assert_eq!(tasks.len(), 1, "background Shell must register exactly one task");
    let task = &tasks[0];
    assert_eq!(task.id.0, outcome.job_id);
    assert_eq!(task.kind, "execute_tool:Shell");
    assert_eq!(task.channels().len(), 1, "background Shell task must have one output channel");
}

#[tokio::test]
async fn await_tool_returns_when_background_shell_completes() {
    // Given — a background shell job
    let dir = tempdir().unwrap();
    let registry = TaskRegistry::new();
    let spawn = execute_tool(
        dir.path(),
        "Shell",
        &serde_json::json!({ "command": "echo done", "block_until_ms": 0 }).to_string(),
        &registry,
        "test-session",
    )
    .await;
    let job_id = spawn.job_id.clone();

    // When — Await is called with the job_id and a generous timeout
    let result = execute_tool(
        dir.path(),
        "Await",
        &serde_json::json!({ "job_id": &job_id, "timeout_ms": 5000 }).to_string(),
        &registry,
        "test-session",
    )
    .await;

    // Then — Await completes without timing out
    assert!(!result.is_error, "Await must not return an error; got {:?}", result.error_message);
    let v: serde_json::Value =
        serde_json::from_str(&result.result_json).expect("Await result must be valid JSON");
    assert_eq!(
        v.get("completed").and_then(|t| t.as_bool()),
        Some(true),
        "Await must report completed=true when the job finishes before the timeout"
    );
}

#[tokio::test]
async fn many_sync_invocations_keep_registry_at_or_below_cap() {
    // Given — a fresh registry with a file to read
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "x").unwrap();
    let registry = TaskRegistry::new();

    // When — 210 fast Read invocations exceed the 200-task cap
    invoke_read_n_times(dir.path(), &registry, 210).await;

    // Then — cap eviction keeps the list at or below 200
    let count = registry.list().await.len();
    assert!(
        count <= 200,
        "registry must stay at or below the 200-task cap after eviction; got {count}"
    );
}

/// Invoke Read on `f.txt` inside `dir` exactly `n` times, waiting for each task to reach
/// terminal status before proceeding so the exit-monitor has a chance to evict stale entries.
async fn invoke_read_n_times(dir: &std::path::Path, registry: &TaskRegistry, n: usize) {
    for _ in 0..n {
        let outcome = execute_tool(
            dir,
            "Read",
            &serde_json::json!({ "path": "f.txt" }).to_string(),
            registry,
            "test-session",
        )
        .await;
        // Drain each task to terminal so the exit-monitor's evict_over_cap fires.
        if let Some(handle) = registry.get_by_str(&outcome.job_id).await {
            wait_terminal(&handle).await;
            // Yield to give exit_monitor's tokio::spawn a chance to call evict_over_cap.
            tokio::task::yield_now().await;
        }
    }
}
