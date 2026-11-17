//! A session's agent ending is announced once, with how it ended — and a tab ending is not
//! (`#activity-wake` 1/3).
//!
//! The PTY is real: a stub agent that exits with a chosen code, or sleeps until it is stopped.
//!
//! Feature: docs/ft/daemon/session-notifications.md (*What is notified*)

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_cli_sessions::cli_session_manager::{AgentEnd, CliSessionManager};
use tddy_testing_commons::wait::eventually_awaiting;
use tokio::sync::broadcast;

const SESSION: &str = "019fb0c1-agent-end";

/// A safety net for the stub to exit and its end to be announced, not a prediction of how long it
/// takes.
const END_OBSERVE: Duration = Duration::from_secs(10);

/// How long nothing more is announced for, after the one end that was. The end is announced once,
/// after the terminal is deregistered; a second announcement would follow within this.
const HELD: Duration = Duration::from_millis(300);

/// An executable stub agent running `body`.
fn a_stub_agent(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("stub_agent.sh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Spawn `binary` as `SESSION`'s main terminal.
async fn runs_as_main(manager: &CliSessionManager, dir: &Path, binary: &Path) {
    manager
        .spawn_rendered_main(
            SESSION,
            "agent-cli",
            dir.to_path_buf(),
            "",
            vec![binary.to_string_lossy().into_owned()],
            Vec::new(),
            None,
            None,
        )
        .await
        .expect("the stub agent spawned");
}

/// The next end announced, within [`END_OBSERVE`].
async fn the_next_end(ends: &mut broadcast::Receiver<AgentEnd>) -> AgentEnd {
    tokio::time::timeout(END_OBSERVE, ends.recv())
        .await
        .expect("no agent end was announced")
        .expect("the announcement channel closed")
}

/// Assert nothing is announced for [`HELD`].
async fn assert_nothing_more(ends: &mut broadcast::Receiver<AgentEnd>) {
    let more = tokio::time::timeout(HELD, ends.recv()).await;
    assert!(more.is_err(), "a further end was announced: {more:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_main_whose_agent_exits_announces_its_end_once_with_the_exit_code() {
    // Given a stub agent that exits with code 3, run as a session's main
    let dir = tempfile::tempdir().unwrap();
    let agent = a_stub_agent(dir.path(), "exit 3");
    let manager = CliSessionManager::new();
    let mut ends = manager.agent_ends();

    // When it runs and exits
    runs_as_main(&manager, dir.path(), &agent).await;

    // Then its end is announced, with the code, exactly once
    assert_eq!(
        the_next_end(&mut ends).await,
        AgentEnd {
            session_id: SESSION.to_string(),
            os_user: None,
            outcome: "exit code 3".to_string(),
            exit_code: Some(3),
        }
    );
    assert_nothing_more(&mut ends).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_main_stopped_by_the_daemon_announces_its_end_as_stopped() {
    // Given a stub agent that runs until it is stopped
    let dir = tempfile::tempdir().unwrap();
    let agent = a_stub_agent(dir.path(), "exec sleep 300");
    let manager = CliSessionManager::new();
    let mut ends = manager.agent_ends();
    runs_as_main(&manager, dir.path(), &agent).await;

    // When the daemon stops the session
    manager.stop_session(SESSION).await;

    // Then its end is announced as a stop, with no exit code
    assert_eq!(
        the_next_end(&mut ends).await,
        AgentEnd {
            session_id: SESSION.to_string(),
            os_user: None,
            outcome: "stopped by the daemon".to_string(),
            exit_code: None,
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tab_ending_announces_nothing() {
    // Given a session tab (not its main) running a shell that exits at once
    let dir = tempfile::tempdir().unwrap();
    let shell = a_stub_agent(dir.path(), "exit 0");
    let manager = CliSessionManager::new();
    let mut ends = manager.agent_ends();
    let tab = manager
        .start_terminal(SESSION, dir.path().to_path_buf(), &shell.to_string_lossy())
        .await
        .expect("the tab started");

    // When it has exited and been deregistered
    eventually_awaiting("the tab to be deregistered", END_OBSERVE, || async {
        match manager.get_terminal(SESSION, &tab.terminal_id).await {
            None => Ok(()),
            Some(_) => Err("the tab is still registered".to_string()),
        }
    })
    .await;

    // Then no agent end is announced: a tab is a client of nothing the session keeps
    assert_nothing_more(&mut ends).await;
}
