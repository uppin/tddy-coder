//! Acceptance: `TerminateSession` does what the web UI's Terminate does — SIGTERM to the session's
//! process — and leaves its record, worktree and branch in place, on its own host or another
//! (`#start-parity` 7/9).
//!
//! A session's process is stood in for by the main agent the daemon holds for it, which is what
//! `SignalSession` reaches.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-terminate.md

mod support;

use std::sync::Arc;

use support::{
    a_host, a_workspace_in, git_stdout, Host, InProcessPeers, HOST_A, HOST_B, OTHER_PROJECT,
    THIS_PROJECT,
};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    on_b: String,
    foreign_on_a: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    let foreign_on_a = a_workspace_in(&a.daemon, OTHER_PROJECT).await;
    World {
        a,
        b,
        home,
        on_a,
        on_b,
        foreign_on_a,
    }
}

/// The main agent the daemon holds for a session, a stub that records the signal it receives.
struct StandIn {
    main: tddy_session_lifecycle::test_util::HeldMain,
    host_daemon: Arc<tddy_daemon_rpc::test_util::TestDaemon>,
    session_id: String,
}

impl StandIn {
    /// The signal the agent received, waiting a few seconds for it: `SIGTERM` for the `TERM` it
    /// records, `None` when it received none.
    async fn exit_signal(&self) -> Option<i32> {
        match self.main.signal_received().await.as_deref() {
            Some("TERM") => Some(libc::SIGTERM),
            _ => None,
        }
    }

    /// Whether the daemon still holds the agent.
    async fn is_held(&self) -> bool {
        self.host_daemon.serves_a_main_for(&self.session_id).await
    }
}

/// `session_id`'s main agent, held by `host`'s daemon: the session is running.
async fn a_running_process_for(host: &Host, session_id: &str) -> StandIn {
    let main = host.daemon.holds_a_main_agent(session_id).await;
    StandIn {
        main,
        host_daemon: Arc::clone(&host.daemon),
        session_id: session_id.to_string(),
    }
}

impl World {
    async fn home_calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
            .into_inner()
    }
}

#[tokio::test]
async fn terminate_sends_sigterm_and_keeps_the_record_worktree_and_branch() {
    // Given a session of the project with a running process
    let world = a_world().await;
    let process = a_running_process_for(&world.a, &world.on_a).await;
    let worktree = world.a.worktree_of(&world.on_a);
    let branch = git_stdout(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);

    // When the home session terminates it
    let terminated = world
        .home_calls(
            "TerminateSession",
            serde_json::json!({ "session_id": world.on_a }),
        )
        .await;

    // Then its process got SIGTERM, and its record, worktree and branch are all still there
    assert!(!terminated.is_error, "{}", terminated.error_message);
    assert_eq!(process.exit_signal().await, Some(libc::SIGTERM));
    assert!(tddy_core::session_lifecycle::unified_session_dir_path(
        world.a.sessions.path(),
        &world.on_a
    )
    .is_dir());
    assert!(worktree.is_dir());
    assert_eq!(
        git_stdout(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]),
        branch
    );
}

#[tokio::test]
async fn terminating_a_session_whose_process_has_ended_is_an_error_saying_so() {
    // Given a session whose process has already ended
    let world = a_world().await;
    let process = a_running_process_for(&world.a, &world.on_a).await;
    world.a.daemon.stops_holding(&world.on_a).await;
    drop(process);

    // When
    let refused = world
        .home_calls(
            "TerminateSession",
            serde_json::json!({ "session_id": world.on_a }),
        )
        .await;

    // Then
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("no running agent"),
        "{}",
        refused.error_message
    );
}

#[tokio::test]
async fn a_session_of_another_project_is_refused_and_left_running() {
    // Given a running session of another project
    let world = a_world().await;
    let process = a_running_process_for(&world.a, &world.foreign_on_a).await;

    // When
    let refused = world
        .home_calls(
            "TerminateSession",
            serde_json::json!({ "session_id": world.foreign_on_a }),
        )
        .await;

    // Then
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("another project"),
        "{}",
        refused.error_message
    );
    assert!(process.is_held().await, "the process still runs");
}

#[tokio::test]
async fn terminate_stops_a_session_on_another_host() {
    // Given a running session of the project on host-b
    let world = a_world().await;
    let process = a_running_process_for(&world.b, &world.on_b).await;

    // When the home session terminates it, naming host-b
    let terminated = world
        .home_calls(
            "TerminateSession",
            serde_json::json!({ "session_id": world.on_b, "daemon_instance_id": HOST_B }),
        )
        .await;

    // Then host-b's process got SIGTERM
    assert!(!terminated.is_error, "{}", terminated.error_message);
    assert_eq!(process.exit_signal().await, Some(libc::SIGTERM));
}
