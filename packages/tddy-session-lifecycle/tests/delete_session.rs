//! Acceptance tests for DeleteSession RPC (inactive session directory removal).
//!
//! These verify filesystem-safe deletion: inactive sessions, and active sessions after the
//! daemon terminates the recorded PID (SIGTERM then SIGKILL).
//!
//! A tmux-run session's agent is ended by its delete even when it survives its tmux session's end
//! — SIGTERM, then SIGKILL after a grace — and only while its pane's pid still names the process
//! that started when the agent did, as the session's tab roster recorded (`#tab-hardening` 2/4).
//! Those run a real, private tmux (`support::PrivateTmux`) and a stub agent that ignores SIGHUP;
//! serial, since the tmux socket directory is process-wide environment.
//!
//! A delete also kills the tmux sessions of the session's tmux-wrapped tabs, and treats a worktree
//! that is already gone — removed outside the daemon, or by a close — as removed
//! (docs/ft/daemon/session-liveness.md § Closing a session).

mod support;
mod tmux_agent_support;

use std::time::Duration;

use tddy_cli_sessions::process_start::{process_start, ProcessStart};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Request;
use tddy_service::proto::session::{DeleteSessionRequest, SessionService as SessionServiceTrait};
use tddy_session_lifecycle::test_util::{test_service, TEST_TOKEN};
use tddy_testing_commons::builders::a_session_metadata;
use tddy_testing_commons::fs::write_session_yaml;
use tddy_testing_commons::wait::eventually;

use support::OBSERVE;
use tmux_agent_support::{
    a_daemon_whose_agent, a_daemon_with_a_private_tmux, a_process_is_alive, AgentTemperament,
    KilledOnDrop,
};

/// Longer than a delete takes to signal what it signals: had the delete sent the agent SIGTERM, it
/// would have ended by then.
const HELD: Duration = Duration::from_millis(500);

/// Acceptance: DeleteSession removes the on-disk session directory when the session is inactive.
#[tokio::test]
async fn removes_inactive_session_directory() {
    // Given
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "inactive-delete-me");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = a_session_metadata()
        .with_session_id("inactive-delete-me")
        .with_pid(pid)
        .with_repo_path("/tmp")
        .with_tool("test-tool")
        .with_livekit_room("test-room")
        .build();
    write_session_yaml(&session_dir, &metadata);

    let service = test_service(sessions_base);

    // When
    let request = Request::direct(DeleteSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: "inactive-delete-me".to_string(),
    });
    let response = service
        .delete_session(request)
        .await
        .expect("DeleteSession should succeed for an inactive session");

    // Then
    assert!(response.into_inner().ok);
    assert!(
        !session_dir.exists(),
        "session directory should be removed after successful delete"
    );

    // Second delete should fail — session is gone from this daemon
    let second = Request::direct(DeleteSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: "inactive-delete-me".to_string(),
    });
    let repeat = service.delete_session(second).await;
    assert!(
        repeat.is_err(),
        "second delete after removal should fail (ownership / routing)"
    );
    assert_eq!(
        repeat.unwrap_err().code,
        tddy_rpc::Code::FailedPrecondition,
        "missing session on this daemon maps to failed_precondition (multi-host safe)"
    );
}

/// Acceptance: DeleteSession terminates a live PID then removes the session directory.
#[cfg(unix)]
#[tokio::test]
async fn terminates_active_session_then_removes_directory() {
    // Given
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();

    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = unified_session_dir_path(&sessions_base, "active-delete-me");
    std::fs::create_dir_all(&session_dir).unwrap();
    let metadata = a_session_metadata()
        .with_session_id("active-delete-me")
        .with_pid(pid)
        .with_repo_path("/tmp")
        .with_tool("test-tool")
        .with_livekit_room("test-room")
        .build();
    write_session_yaml(&session_dir, &metadata);

    let service = test_service(sessions_base);

    // When
    let request = Request::direct(DeleteSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: "active-delete-me".to_string(),
    });
    let response = service
        .delete_session(request)
        .await
        .expect("DeleteSession should terminate the process and remove the directory");

    // Then
    assert!(response.into_inner().ok);
    assert!(
        !session_dir.exists(),
        "session directory should be removed after delete"
    );

    let wait = child.wait();
    assert!(wait.is_ok(), "child should have been signalled");
}

/// Acceptance: deleting a tmux-run session ends its agent even when the agent ignores the SIGHUP a
/// tmux kill sends and never reads its pane — the kill alone would leave it running.
#[tokio::test]
#[serial_test::serial]
async fn deleting_a_tmux_run_session_whose_agent_ignores_sighup_leaves_no_agent() {
    // Given a running tmux-run session whose agent survives its tmux session's end
    let daemon = a_daemon_whose_agent(AgentTemperament::IgnoresHangupAndNeverReadsItsPane).await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    let agent = daemon.agent_pid_of(&session_id);

    // When it is deleted
    daemon.deletes(&session_id).await;

    // Then neither the agent nor its tmux session is left
    eventually(
        "the agent and its tmux session to be gone",
        OBSERVE,
        || match (a_process_is_alive(agent), daemon.tmux.has_session(&name)) {
            (false, false) => Ok(()),
            (agent, tmux) => Err(format!("agent alive: {agent}, tmux session alive: {tmux}")),
        },
    )
    .await;
}

/// Acceptance: a delete never signals a pane pid whose process did not start when the session's
/// agent did — a pid the kernel may have handed to another process.
#[tokio::test]
#[serial_test::serial]
async fn a_pane_pid_whose_start_time_changed_is_never_signalled() {
    // Given a running tmux-run session whose roster records a start its pane process does not have
    let daemon = a_daemon_whose_agent(AgentTemperament::IgnoresHangupAndNeverReadsItsPane).await;
    let (session_id, name) = daemon.a_running_tmux_session().await;
    let pane_process = daemon.agent_pid_of(&session_id);
    let _cleanup = KilledOnDrop(pane_process);
    daemon.records_the_agent_start(&session_id, ProcessStart::from_epoch_secs(1));

    // When it is deleted
    daemon.deletes(&session_id).await;
    tokio::time::sleep(HELD).await;

    // Then its tmux session is gone, and the process the pid names was left alone
    assert_eq!(
        (
            daemon.tmux.has_session(&name),
            a_process_is_alive(pane_process)
        ),
        (false, true)
    );
}

/// Acceptance: a tmux-run session's roster records when its agent started — the start a delete
/// checks its pane pid against.
#[tokio::test]
#[serial_test::serial]
async fn a_tmux_run_session_records_when_its_agent_started() {
    // Given
    let daemon = a_daemon_with_a_private_tmux().await;

    // When
    let (session_id, _name) = daemon.a_running_tmux_session().await;

    // Then
    let agent = daemon.agent_pid_of(&session_id);
    assert_eq!(
        daemon.recorded_agent_start(&session_id),
        process_start(agent)
    );
}

/// Deleting a session ends its tmux-wrapped agent tabs too, not only the tmux session
/// its main runs in — a tab's tmux session would otherwise keep its agent running with no session
/// left to reclaim it.
#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_kills_the_tmux_sessions_of_its_tabs() {
    // Given a running tmux-run session with a tmux-wrapped agent tab open
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    let (_tab, tab_tmux) = daemon.opens_a_tmux_wrapped_agent_tab(&session_id).await;

    // When it is deleted
    daemon.deletes(&session_id).await;

    // Then the tab's tmux session is gone with it
    eventually("the tab's tmux session to be gone", OBSERVE, || {
        (!daemon.tmux.has_session(&tab_tmux))
            .then_some(())
            .ok_or_else(|| format!("tmux session {tab_tmux:?} is still alive"))
    })
    .await;
}

/// A session whose worktree is already gone is deleted all the same.
#[tokio::test]
#[serial_test::serial]
async fn deleting_a_session_whose_worktree_was_already_removed_succeeds() {
    // Given a running session whose worktree was removed outside the daemon
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.worktree_is_removed_outside_the_daemon(&session_id);

    // When
    let deleted = daemon.delete_of(&session_id).await;

    // Then
    assert_eq!(
        (
            deleted.map_err(|s| s.message().to_string()),
            daemon.session_dir(&session_id).exists()
        ),
        (Ok(()), false)
    );
}

/// A session that was closed — its tabs ended and its worktree removed — is deleted.
#[tokio::test]
#[serial_test::serial]
async fn deleting_a_closed_session_succeeds() {
    // Given a closed session
    let daemon = a_daemon_with_a_private_tmux().await;
    let (session_id, _name) = daemon.a_running_tmux_session().await;
    daemon.closes(&session_id).await.outcome();

    // When
    let deleted = daemon.delete_of(&session_id).await;

    // Then
    assert_eq!(
        (
            deleted.map_err(|s| s.message().to_string()),
            daemon.session_dir(&session_id).exists()
        ),
        (Ok(()), false)
    );
}
