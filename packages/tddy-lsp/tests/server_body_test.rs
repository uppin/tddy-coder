//! The long-running server task body: it stays alive until cancelled, and an unresponsive
//! server is force-killed via the registry's escalation safety net.

use std::time::Duration;

use tddy_lsp::{LaunchSpec, LspServerBody};
use tddy_task::{ChannelKind, TaskChannel, TaskId, TaskRegistry, TaskStatus};
use tokio::sync::oneshot;

fn fake_spec(args: &[&str]) -> LaunchSpec {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"));
    spec.args = args.iter().map(|a| a.to_string()).collect();
    spec
}

async fn wait_for_status(
    tasks: &TaskRegistry,
    id: &TaskId,
    expected: TaskStatus,
    timeout: Duration,
) {
    tokio::time::timeout(timeout, async {
        loop {
            if let Some(handle) = tasks.get(id).await {
                if handle.status() == expected {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("task never reached {expected:?}"));
}

#[tokio::test]
async fn a_language_server_task_stays_running_until_it_is_cancelled() {
    // Given a language-server task hosting the fake server
    let tasks = TaskRegistry::new();
    let (client_tx, _client_rx) = oneshot::channel();
    let (channel, _stdin_rx) = TaskChannel::new("0", "lsp", ChannelKind::Combined);
    let body = LspServerBody {
        spec: fake_spec(&[]),
        root_dir: std::env::temp_dir(),
        client_tx,
    };
    let handle = tasks.spawn(body, "lsp:rust", "", vec![channel]).await;

    // When it has had a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Then it is still running — a language server does not exit on its own
    assert_eq!(handle.status(), TaskStatus::Running);

    // And when it is cancelled it reaches a terminal Cancelled state
    tasks.cancel_task(&handle.id).await;
    wait_for_status(
        &tasks,
        &handle.id,
        TaskStatus::Cancelled,
        Duration::from_secs(3),
    )
    .await;
}

#[tokio::test]
async fn an_unresponsive_language_server_is_killed_after_the_grace_period() {
    // Given a language-server task whose server ignores shutdown/exit
    let tasks = TaskRegistry::new();
    let (client_tx, _client_rx) = oneshot::channel();
    let (channel, _stdin_rx) = TaskChannel::new("0", "lsp", ChannelKind::Combined);
    let body = LspServerBody {
        spec: fake_spec(&["--hang"]),
        root_dir: std::env::temp_dir(),
        client_tx,
    };
    let handle = tasks.spawn(body, "lsp:rust", "", vec![channel]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // When it is cancelled and does not shut down gracefully
    tasks.cancel_task(&handle.id).await;

    // Then the registry's escalation safety net force-kills it (SIGTERM→SIGKILL).
    // 10s timeout: the registry grace is 5s + a 2s SIGTERM wait before SIGKILL.
    wait_for_status(
        &tasks,
        &handle.id,
        TaskStatus::Cancelled,
        Duration::from_secs(10),
    )
    .await;
}
