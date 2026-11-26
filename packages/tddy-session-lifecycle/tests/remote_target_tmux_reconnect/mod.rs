//! Acceptance: a target tmux session stays live when its transport drops and re-attaches on
//! view (`#target-reconnect` 2/3) — the client exit is classified by probing the tmux session
//! **on the target**, a detach watcher holds the session while it lives, and the first stream
//! attaches a new transport client (`attach-session`, never a relaunch).
//!
//! A module of `remote_target_session_acceptance`: the real daemon, and the stand-in ssh target
//! whose tmux is this host's binary on the target's own socket directory.

use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::read_session_metadata;
use tddy_rpc::Request;
use tddy_service::proto::session::{ListSessionsRequest, SessionService as SessionServiceTrait};
use tddy_terminal_rpc::proto::terminal_session::StreamReplayMode;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamTerminalOutputRequest, TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

use super::cli_harness_support::{current_os_user, VALID_TOKEN};
use super::{
    a_daemon_with_a_reachable_target, a_tmux_start_on_the_target, tmux_session_of,
    ADaemonWithATarget, OBSERVE,
};

const MAIN: &str = "main";
/// Longer than two watcher intervals of a default manager (10 s), for a gone tmux to be seen.
const WATCH: Duration = Duration::from_secs(45);

impl ADaemonWithATarget {
    /// Whether `session_id` lists active on this daemon.
    async fn is_listed_active(&self, session_id: &str) -> bool {
        self.service
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                include_peers: false,
            }))
            .await
            .expect("the sessions are listed")
            .into_inner()
            .sessions
            .into_iter()
            .find(|entry| entry.session_id == session_id)
            .is_some_and(|entry| entry.is_active)
    }

    /// The local client of `session_id`'s main ends, as a dropped connection ends it: its
    /// process is killed and nothing is sent to the target.
    async fn the_main_client_drops(&self, session_id: &str) {
        let pid = self
            .manager
            .get(session_id)
            .await
            .expect("a main terminal is registered")
            .pid;
        // SAFETY: a plain kill(2) of the local client this test's daemon spawned.
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        self.the_terminal_has_ended(session_id).await;
    }

    /// A client's first stream of `session_id`'s main terminal.
    async fn first_stream_of_the_main(&self, session_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .terminal_session_service()
            .stream_terminal_output(Request::direct(StreamTerminalOutputRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                terminal_id: MAIN.to_string(),
                initial_cols: 0,
                initial_rows: 0,
                mode: StreamReplayMode::Tail as i32,
                from_offset: 0,
                control_token: String::new(),
            }))
            .await
            .map(|_| ())
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_start_records_its_owner_subject() {
    // Given a tmux run started on the target by the test's caller
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon
        .start(a_tmux_start_on_the_target())
        .await
        .expect("a tmux start on a target succeeds");

    // Then the record names who reached the target, for a caller-less re-attach (D3)
    let metadata = read_session_metadata(&daemon.session_dir(&session_id)).expect("metadata");
    assert_eq!(
        metadata.owner_subject.as_deref(),
        Some(current_os_user().as_str())
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_session_stays_live_when_its_transport_client_drops() {
    // Given a tmux run on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon
        .start(a_tmux_start_on_the_target())
        .await
        .expect("a tmux start on a target succeeds");
    let agent = daemon.the_agent_on_the_target(&session_id).await;

    // When its transport client drops
    daemon.the_main_client_drops(&session_id).await;

    // Then the session still lists active, its agent and tmux session on the target untouched
    eventually_awaiting("the dropped session to list active", OBSERVE, || async {
        daemon
            .is_listed_active(&session_id)
            .await
            .then_some(())
            .ok_or_else(|| "the session no longer lists active".to_string())
    })
    .await;
    assert_eq!(
        (
            daemon.manager.get(&session_id).await.is_none(),
            agent.is_alive(),
            daemon
                .target
                .has_tmux_session(&tmux_session_of(&session_id)),
        ),
        (true, true, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_first_stream_reattaches_a_dropped_target_main_without_relaunching() {
    // Given a tmux run on the target whose transport client dropped
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon
        .start(a_tmux_start_on_the_target())
        .await
        .expect("a tmux start on a target succeeds");
    let agent = daemon.the_agent_on_the_target(&session_id).await;
    daemon.the_main_client_drops(&session_id).await;

    // When the main is first streamed
    let streamed = daemon.first_stream_of_the_main(&session_id).await;

    // Then exactly one new client attaches to the same agent: no relaunch, same pane pid
    eventually_awaiting("the main to be re-attached", OBSERVE, || async {
        daemon
            .manager
            .get(&session_id)
            .await
            .map(|_| ())
            .ok_or_else(|| "no main terminal is registered".to_string())
    })
    .await;
    let runs = daemon.agent_runs();
    assert_eq!(
        (
            streamed.map_err(|status| status.message().to_string()),
            runs.len(),
            runs.first().map(|run| run.pid),
            daemon
                .target
                .has_tmux_session(&tmux_session_of(&session_id)),
        ),
        (Ok(()), 1, Some(agent.pid), true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_dropped_target_agent_tab_is_held_by_a_watcher() {
    // Given a [target, tmux, agent] tab of a session on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon.a_session_on_the_target().await;
    daemon.the_agent_on_the_target(&session_id).await;
    let tab = daemon
        .opens_a_tab(&session_id, super::SESSION_TYPE, "", &["target", "tmux"])
        .await
        .expect("an agent-in-tmux tab starts on the target");
    let name = daemon.recorded_tmux_session_of(&session_id, &tab);
    eventually(
        "the tab's tmux session to live on the target",
        OBSERVE,
        || {
            daemon
                .target
                .has_tmux_session(&name)
                .then_some(())
                .ok_or_else(|| format!("tmux session {name} is not there yet"))
        },
    )
    .await;

    // When the tab's local client is killed, as a dropped connection kills it
    let pid = daemon
        .manager
        .get_terminal(&session_id, &tab)
        .await
        .expect("the tab is registered")
        .pid;
    // SAFETY: a plain kill(2) of the tab's local client this test's daemon spawned.
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    eventually_awaiting("the tab's terminal to be gone", OBSERVE, || async {
        daemon
            .manager
            .get_terminal(&session_id, &tab)
            .await
            .is_none()
            .then_some(())
            .ok_or_else(|| "the tab is still registered".to_string())
    })
    .await;

    // Then its tmux session and agent live on the target, and the session stays live
    assert_eq!(
        (
            daemon.target.has_tmux_session(&name),
            daemon.is_listed_active(&session_id).await,
        ),
        (true, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_reattached_target_main_restarts_its_hook_log_follower() {
    // Given a tmux run on the target whose transport client dropped and was re-attached
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon
        .start(a_tmux_start_on_the_target())
        .await
        .expect("a tmux start on a target succeeds");
    daemon.the_agent_on_the_target(&session_id).await;
    daemon.the_main_client_drops(&session_id).await;
    daemon
        .first_stream_of_the_main(&session_id)
        .await
        .expect("the first stream re-attaches");
    eventually_awaiting("the main to be re-attached", OBSERVE, || async {
        daemon
            .manager
            .get(&session_id)
            .await
            .map(|_| ())
            .ok_or_else(|| "no main terminal is registered".to_string())
    })
    .await;

    // When a hook line is appended on the target after the re-attach — the follower was kept
    // with the tmux session, not with the client, so nothing needs restarting for it to be read
    let metadata = read_session_metadata(&daemon.session_dir(&session_id)).expect("metadata");
    let token = metadata
        .hook_token
        .expect("the start recorded a hook token");
    let hook_log = daemon.target.hook_log_of(&session_id);
    let mut contents = std::fs::read_to_string(&hook_log).unwrap_or_default();
    contents.push_str(&format!("UserPromptSubmit\t{token}\t{{}}\n"));
    std::fs::write(&hook_log, contents).expect("the hook line is appended");

    // Then the follower reads it: the session's activity moves on
    eventually_awaiting("the hook to be read after re-attach", OBSERVE, || async {
        let status = read_session_metadata(&daemon.session_dir(&session_id))
            .ok()
            .and_then(|metadata| metadata.activity_status);
        match status.as_deref() {
            Some("Running") => Ok(()),
            other => Err(format!("the session's activity so far: {other:?}")),
        }
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_session_whose_tmux_ended_while_detached_ends() {
    // Given a tmux run on the target whose transport client dropped
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon
        .start(a_tmux_start_on_the_target())
        .await
        .expect("a tmux start on a target succeeds");
    let _agent = daemon.the_agent_on_the_target(&session_id).await;
    daemon.the_main_client_drops(&session_id).await;
    let mut ends = daemon.manager.agent_ends();

    // When its tmux session ends on the target while detached
    let killed = daemon.target.tmux(&[
        "kill-session",
        "-t",
        &format!("={}", tmux_session_of(&session_id)),
    ]);
    assert!(killed.status.success(), "the tmux session is killed");

    // Then the watcher sees it gone: the session lists inactive and the agent end is announced
    // with no exit code
    eventually_awaiting("the session to list inactive", WATCH, || async {
        (!daemon.is_listed_active(&session_id).await)
            .then_some(())
            .ok_or_else(|| "the session still lists active".to_string())
    })
    .await;
    let end = tokio::time::timeout(WATCH, ends.recv())
        .await
        .expect("the agent end is announced")
        .expect("the channel is open");
    assert_eq!(
        (end.session_id.as_str(), end.exit_code),
        (session_id.as_str(), None)
    );
}
