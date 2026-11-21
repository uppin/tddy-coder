//! [`CliSessionManager::start_terminal_under_id`]: a tab comes back under the id it was recorded with.

use super::{CliSessionManager, RecordedTerminal, MAIN_TERMINAL_ID};

const RECORDED_ID: &str = "01a1f5aa-0000-7000-8000-000000000001";

fn an_agent_holding_its_pty() -> RecordedTerminal {
    RecordedTerminal::Agent {
        kind: "muse-cli".to_string(),
        argv: vec!["/bin/cat".to_string()],
    }
}

#[tokio::test]
async fn a_terminal_started_under_a_recorded_id_is_listed_under_that_id() {
    // Given
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();

    // When
    manager
        .start_terminal_under_id(
            "recorded-session",
            RECORDED_ID,
            worktree.path().to_path_buf(),
            an_agent_holding_its_pty(),
        )
        .await
        .expect("the tab starts under its recorded id");

    // Then
    let listed: Vec<String> = manager
        .list_terminals("recorded-session")
        .await
        .iter()
        .map(|t| t.terminal_id.clone())
        .collect();
    assert_eq!(listed, vec![RECORDED_ID.to_string()]);
    manager.kill_all().await;
}

#[tokio::test]
async fn a_shell_started_under_a_recorded_id_is_a_bash_terminal() {
    // Given
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();

    // When
    let shell = manager
        .start_terminal_under_id(
            "recorded-shell",
            RECORDED_ID,
            worktree.path().to_path_buf(),
            RecordedTerminal::Shell {
                shell_path: "/bin/sh".to_string(),
            },
        )
        .await
        .expect("the shell starts under its recorded id");

    // Then
    assert_eq!(shell.kind, "bash");
    manager.kill_all().await;
}

#[tokio::test]
async fn the_main_id_is_refused() {
    // Given
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();

    // When
    let started = manager
        .start_terminal_under_id(
            "recorded-main",
            MAIN_TERMINAL_ID,
            worktree.path().to_path_buf(),
            an_agent_holding_its_pty(),
        )
        .await;

    // Then
    assert!(started.is_err(), "main is restored by its own resume path");
}

#[tokio::test]
async fn an_id_already_live_in_the_session_is_refused() {
    // Given a tab already running under the recorded id
    let manager = CliSessionManager::new();
    let worktree = tempfile::tempdir().unwrap();
    manager
        .start_terminal_under_id(
            "recorded-twice",
            RECORDED_ID,
            worktree.path().to_path_buf(),
            an_agent_holding_its_pty(),
        )
        .await
        .expect("the first start succeeds");

    // When
    let again = manager
        .start_terminal_under_id(
            "recorded-twice",
            RECORDED_ID,
            worktree.path().to_path_buf(),
            an_agent_holding_its_pty(),
        )
        .await;

    // Then
    assert!(again.is_err(), "one live terminal per id");
    manager.kill_all().await;
}
