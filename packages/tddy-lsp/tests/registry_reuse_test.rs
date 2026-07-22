//! Registry behaviour: reuse across targets, per-root isolation, allow-list gating, idle
//! teardown, and respawn-after-crash. All tests run against the deterministic `fake_lsp`
//! server, never a real language server.

use std::path::PathBuf;
use std::time::Duration;

use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspError, LspKey, LspRegistry};
use tddy_task::{TaskId, TaskRegistry};

fn fake_allow_list() -> LspAllowList {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    allow
}

fn key(root: &str) -> LspKey {
    LspKey {
        root: PathBuf::from(root),
        language: Language::Rust,
    }
}

async fn wait_for_terminal(tasks: &TaskRegistry, id: &TaskId) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match tasks.get(id).await {
                Some(handle) if handle.status().is_terminal() => return,
                None => return,
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .expect("task never reached a terminal state");
}

#[tokio::test]
async fn two_targets_in_one_workspace_reuse_a_single_language_server() {
    // Given a registry over a fake language server and one workspace
    let tasks = TaskRegistry::new();
    let registry = LspRegistry::new(fake_allow_list(), tasks.clone(), Duration::from_secs(60));
    let workspace = key("/workspace");

    // When two targets in that workspace each request a server
    let first = registry
        .get_or_spawn(workspace.clone())
        .await
        .expect("first target's server");
    let second = registry
        .get_or_spawn(workspace.clone())
        .await
        .expect("second target's server");

    // Then both targets share a single running server task
    assert_eq!(first.task_id, second.task_id);
    assert_eq!(tasks.list().await.len(), 1);
}

#[tokio::test]
async fn different_workspace_roots_get_separate_language_servers() {
    // Given a registry over a fake language server
    let tasks = TaskRegistry::new();
    let registry = LspRegistry::new(fake_allow_list(), tasks.clone(), Duration::from_secs(60));

    // When targets in two different workspaces each request a server
    let a = registry
        .get_or_spawn(key("/workspace-a"))
        .await
        .expect("workspace-a server");
    let b = registry
        .get_or_spawn(key("/workspace-b"))
        .await
        .expect("workspace-b server");

    // Then each workspace gets its own server task
    assert_ne!(a.task_id, b.task_id);
    assert_eq!(tasks.list().await.len(), 2);
}

#[tokio::test]
async fn requesting_a_disallowed_language_returns_an_error_and_spawns_no_server() {
    // Given a registry whose allow-list permits no languages
    let tasks = TaskRegistry::new();
    let registry = LspRegistry::new(LspAllowList::new(), tasks.clone(), Duration::from_secs(60));

    // When a Rust server is requested
    let result = registry.get_or_spawn(key("/workspace")).await;

    // Then it is rejected and no task is spawned
    assert!(matches!(result, Err(LspError::LanguageNotAllowed(_))));
    assert_eq!(tasks.list().await.len(), 0);
}

#[tokio::test]
async fn an_idle_language_server_is_torn_down_after_the_timeout() {
    // Given a running server with a short idle timeout
    let tasks = TaskRegistry::new();
    let registry = LspRegistry::new(fake_allow_list(), tasks.clone(), Duration::from_millis(50));
    let workspace = key("/workspace");
    registry
        .get_or_spawn(workspace.clone())
        .await
        .expect("server");

    // When it sits idle past the timeout and the reaper runs
    tokio::time::sleep(Duration::from_millis(120)).await;
    let reaped = registry.reap_idle().await;

    // Then it is reaped and no longer available
    assert_eq!(reaped, vec![workspace.clone()]);
    assert!(registry.get(&workspace).await.is_none());
}

#[tokio::test]
async fn activity_keeps_a_language_server_alive_past_the_idle_timeout() {
    // Given a running server with a short idle timeout
    let tasks = TaskRegistry::new();
    let registry = LspRegistry::new(fake_allow_list(), tasks.clone(), Duration::from_millis(50));
    let workspace = key("/workspace");
    registry
        .get_or_spawn(workspace.clone())
        .await
        .expect("server");

    // When it is used again shortly before the reaper runs (resetting the idle timer)
    tokio::time::sleep(Duration::from_millis(30)).await;
    registry
        .get_or_spawn(workspace.clone())
        .await
        .expect("reused server");
    tokio::time::sleep(Duration::from_millis(30)).await;
    let reaped = registry.reap_idle().await;

    // Then it is not reaped
    assert!(reaped.is_empty());
    assert!(registry.get(&workspace).await.is_some());
}

#[tokio::test]
async fn a_crashed_language_server_is_respawned_on_the_next_request() {
    // Given a running server whose task then dies (reaches a terminal state)
    let tasks = TaskRegistry::new();
    let registry = LspRegistry::new(fake_allow_list(), tasks.clone(), Duration::from_secs(60));
    let workspace = key("/workspace");
    let first = registry
        .get_or_spawn(workspace.clone())
        .await
        .expect("first server");
    tasks.cancel_task(&first.task_id).await;
    wait_for_terminal(&tasks, &first.task_id).await;

    // When a target requests a server again
    let second = registry
        .get_or_spawn(workspace.clone())
        .await
        .expect("respawned server");

    // Then a fresh server task is spawned rather than a dead handle returned
    assert_ne!(first.task_id, second.task_id);
}
