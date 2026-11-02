//! Acceptance tests: a session tab on a new tmux session — a `tmux-new` terminal creates the named
//! tmux session in the session's worktree (or attaches when it exists) and stopping the tab detaches
//! it while the tmux session keeps running.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tmux-new-session-tab.md (`#session-tabs 7/7`)
//!
//! Each test runs a real tmux server of its own: `TMUX_TMPDIR` points tmux's socket directory at a
//! temp dir, so the daemon's tmux and the test's tmux are one private server, and no developer's
//! tmux is touched. The session's identity maps to the OS user running the test, so the tab runs as
//! that user without a privilege change. Tests mutate the process environment, so they run serially.

mod support;

use support::*;
use tddy_rpc::Code;

// ---------------------------------------------------------------------------
// A tab on a new tmux session
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_creates_the_named_session_in_the_worktree_and_attaches() {
    // Given no tmux session named work
    let tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    // When
    daemon
        .start_new_tmux_tab("work")
        .await
        .expect("a new tmux session tab starts");

    // Then tmux runs work, rooted in the session's worktree, with the tab attached
    until_attached(&tmux, "work", 1).await;
    assert_eq!(tmux.pane_directory("work"), daemon.worktree);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn stopping_a_tmux_new_tab_leaves_its_session_running() {
    // Given a tab on a new session
    let tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;
    let tab = daemon
        .start_new_tmux_tab("work")
        .await
        .expect("a new tmux session tab starts");
    until_attached(&tmux, "work", 1).await;

    // When
    daemon.stop_tab(&tab).await;

    // Then
    until_attached(&tmux, "work", 0).await;
    assert!(tmux.has_session("work"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_for_an_existing_name_attaches_to_it() {
    // Given a session named work that already has two windows
    let tmux = PrivateTmux::started().with_session("work", 2);
    let daemon = a_daemon_with_a_running_session().await;

    // When
    daemon
        .start_new_tmux_tab("work")
        .await
        .expect("the tab attaches");

    // Then the existing session gained a client and was not replaced
    until_attached(&tmux, "work", 1).await;
    let windows = tmux.tmux(&["list-windows", "-t", "work"]);
    assert_eq!(String::from_utf8_lossy(&windows.stdout).lines().count(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_with_an_empty_name_is_refused() {
    let _tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    let refusal = daemon.start_new_tmux_tab("  ").await.unwrap_err();

    // Refused by the name's validation: the whitespace-only name passes the kind mapping.
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("needs a name"), "{refusal:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn a_tmux_new_tab_with_a_colon_in_its_name_is_refused() {
    let tmux = PrivateTmux::started();
    let daemon = a_daemon_with_a_running_session().await;

    let refusal = daemon.start_new_tmux_tab("a:b").await.unwrap_err();

    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("may not contain"), "{refusal:?}");
    assert!(!tmux.has_session("a"));
}
