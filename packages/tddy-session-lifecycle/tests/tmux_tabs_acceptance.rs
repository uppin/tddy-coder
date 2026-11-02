//! Acceptance tests: a session tab attached to a running tmux session — `ListTmuxSessions` lists the
//! session user's tmux sessions, a `tmux` terminal attaches a client to one, and stopping the tab
//! detaches it while the tmux session keeps running.
//!
//! Feature: docs/ft/daemon/terminal-sessions.md § tmux terminals (`#session-tabs 3/7`)
//!
//! Each test runs a real tmux server of its own: `TMUX_TMPDIR` points tmux's socket directory at a
//! temp dir, so the daemon's tmux and the test's tmux are one private server, and no developer's
//! tmux is touched. The session's identity maps to the OS user running the test, so the tab runs as
//! that user without a privilege change. Tests mutate the process environment, so they run serially.

mod support;

use support::*;
use tddy_rpc::{Code, Request};
use tddy_terminal_rpc::proto::terminal_session::{
    ListTerminalSessionsRequest, TerminalSessionService as TerminalSessionServiceTrait,
    TmuxSessionInfo,
};
use tddy_testing_commons::wait::eventually;

/// Each listed session as `(name, windows, attached)`.
fn name_windows_attached(sessions: &[TmuxSessionInfo]) -> Vec<(&str, u32, u32)> {
    sessions
        .iter()
        .map(|s| (s.name.as_str(), s.windows, s.attached))
        .collect()
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_tmux_sessions_returns_the_users_sessions_with_window_and_attached_counts() {
    // Given
    let _tmux = PrivateTmux::started()
        .with_session("alpha", 2)
        .with_session("beta", 1);
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let sessions = daemon
        .list_tmux_sessions()
        .await
        .expect("tmux sessions listed");

    // Then
    assert_eq!(
        name_windows_attached(&sessions),
        vec![("alpha", 2, 0), ("beta", 1, 0)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_tmux_sessions_without_a_tmux_server_is_empty() {
    // Given no tmux server on the private socket
    let _tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let sessions = daemon
        .list_tmux_sessions()
        .await
        .expect("an empty listing, not an error");

    // Then
    assert!(sessions.is_empty(), "{sessions:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_tmux_sessions_without_tmux_installed_is_refused_naming_tmux() {
    // Given a tmux binary that does not exist
    let _tmux = PrivateTmux::started().with_tmux_binary("/nonexistent/tmux");
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let refusal = daemon.list_tmux_sessions().await.unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("tmux"), "{}", refusal.message());
}

// ---------------------------------------------------------------------------
// A tmux tab
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_tab_attaches_to_the_named_session_and_carries_input_and_output() {
    // Given
    let tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;

    // When a tab is attached and the operator types into it
    let tab = daemon
        .start_tmux_tab("alpha")
        .await
        .expect("a tmux tab starts");
    eventually("the tab's client to attach", OBSERVE, || {
        (tmux.attached_clients("alpha") == 1)
            .then_some(())
            .ok_or_else(|| "no client attached yet".to_string())
    })
    .await;
    daemon.type_into(&tab, "echo typed-into-the-tab\r").await;

    // Then the keystrokes reach the tmux pane
    eventually("the pane to show the typed command", OBSERVE, || {
        let text = tmux.pane_text("alpha");
        text.contains("typed-into-the-tab")
            .then_some(())
            .ok_or_else(|| format!("pane holds {text:?}"))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn stopping_a_tmux_tab_detaches_and_leaves_the_tmux_session_running() {
    // Given an attached tab
    let tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;
    let tab = daemon
        .start_tmux_tab("alpha")
        .await
        .expect("a tmux tab starts");
    eventually("the tab's client to attach", OBSERVE, || {
        (tmux.attached_clients("alpha") == 1)
            .then_some(())
            .ok_or_else(|| "no client attached yet".to_string())
    })
    .await;

    // When the tab is closed
    daemon.stop_tab(&tab).await;

    // Then its client is gone and the tmux session is still there
    eventually("the tab's client to detach", OBSERVE, || {
        (tmux.attached_clients("alpha") == 0)
            .then_some(())
            .ok_or_else(|| "a client is still attached".to_string())
    })
    .await;
    assert!(tmux.has_session("alpha"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_tab_for_an_unknown_session_is_not_found() {
    // Given
    let _tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;

    // When
    let refusal = daemon.start_tmux_tab("never-started").await.unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn list_terminal_sessions_reports_a_tmux_tab_with_its_tmux_session_name() {
    // Given an attached tab
    let _tmux = PrivateTmux::started().with_session("alpha", 1);
    let daemon = a_daemon_with_a_running_session().await;
    let tab = daemon
        .start_tmux_tab("alpha")
        .await
        .expect("a tmux tab starts");

    // When
    let terminals = daemon
        .service
        .list_terminal_sessions(Request::direct(ListTerminalSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("ListTerminalSessions must succeed")
        .into_inner()
        .terminals;

    // Then
    let listed = terminals
        .iter()
        .find(|t| t.terminal_id == tab)
        .expect("the tmux tab is listed");
    assert_eq!(listed.kind, "tmux");
    assert_eq!(listed.tmux_session, "alpha");
}
