//! [`CliSessionManager::live_agent_sessions`]: which terminals make a session live.

use std::path::Path;
use std::time::Duration;

use tddy_testing_commons::wait::eventually_awaiting;

use super::CliSessionManager;

/// A safety net for a spawned or exiting terminal to be observed.
const OBSERVE: Duration = Duration::from_secs(10);

/// A command that holds its PTY open until it is stopped.
fn a_holding_agent() -> Vec<String> {
    vec!["/bin/cat".to_string()]
}

async fn with_a_main_agent(manager: &CliSessionManager, session_id: &str, worktree: &Path) {
    manager
        .start_with_options(
            session_id,
            worktree.to_path_buf(),
            "claude-opus-4-8",
            a_holding_agent(),
            "claude-cli",
            None,
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
        .expect("main terminal must start");
}

async fn with_an_agent_tab(
    manager: &CliSessionManager,
    session_id: &str,
    worktree: &Path,
) -> String {
    manager
        .start_agent_terminal(
            session_id,
            worktree.to_path_buf(),
            "muse-cli",
            a_holding_agent(),
        )
        .await
        .expect("agent tab must start")
        .terminal_id
        .clone()
}

#[tokio::test]
async fn a_session_with_a_running_main_agent_is_live() {
    // Given
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    with_a_main_agent(&manager, "main-session", worktree.path()).await;

    // When
    let live = manager.live_agent_sessions().await;

    // Then
    assert!(live.contains("main-session"));
    manager.kill_all().await;
}

#[tokio::test]
async fn a_session_with_only_an_agent_tab_is_live() {
    // Given
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    with_an_agent_tab(&manager, "tab-session", worktree.path()).await;

    // When
    let live = manager.live_agent_sessions().await;

    // Then
    assert!(live.contains("tab-session"));
    manager.kill_all().await;
}

#[tokio::test]
async fn a_session_with_only_a_shell_is_not_live() {
    // Given
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    manager
        .start_terminal("shell-session", worktree.path().to_path_buf(), "/bin/sh")
        .await
        .expect("shell must start");

    // When
    let live = manager.live_agent_sessions().await;

    // Then
    assert!(!live.contains("shell-session"));
    manager.kill_all().await;
}

#[tokio::test]
async fn a_session_whose_agent_tab_was_stopped_is_not_live() {
    // Given
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    let tab = with_an_agent_tab(&manager, "stopped-session", worktree.path()).await;

    // When
    manager.stop_terminal("stopped-session", &tab).await;

    // Then
    assert!(!manager
        .live_agent_sessions()
        .await
        .contains("stopped-session"));
}

#[tokio::test]
async fn a_session_whose_agent_tab_exited_on_its_own_stops_being_live() {
    // Given an agent tab that exits by itself
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    manager
        .start_agent_terminal(
            "exited-session",
            worktree.path().to_path_buf(),
            "muse-cli",
            vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "exit 0".to_string(),
            ],
        )
        .await
        .expect("agent tab must start");

    // When / Then it leaves the live set once it has exited
    eventually_awaiting("the exited tab to stop being live", OBSERVE, || async {
        (!manager
            .live_agent_sessions()
            .await
            .contains("exited-session"))
        .then_some(())
        .ok_or_else(|| "still live".to_string())
    })
    .await;
}
