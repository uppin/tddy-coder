//! The privilege boundary: who may call, and what they may ask for.
//!
//! Peer authorization ("is this caller one of my services?") and spawn policy ("may that caller
//! have *this*?") are separate gates, and each has its own test so a regression in one cannot
//! hide behind the other.

mod support;

use support::{
    a_service, a_supervisor, a_tool_that_records_its_parent, current_username, process_is_alive,
    DenialAssertions,
};
use tddy_supervisor::SpawnSessionRequest;

fn a_session_spawn(os_user: &str, tool_path: &std::path::Path) -> SpawnSessionRequest {
    SpawnSessionRequest {
        os_user: os_user.to_string(),
        tool_path: tool_path.to_path_buf(),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        scope: None,
    }
}

#[tokio::test]
async fn rejects_a_session_spawn_from_a_peer_that_owns_no_declared_service() {
    // Given a supervisor that manages nothing, so no uid on this host owns a declared service —
    // even though the requested user and tool are both allowlisted.
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    let result = supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn rejects_a_session_spawn_for_an_os_user_outside_the_allowlist() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_session_user("someone-else")
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    let result = supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn rejects_a_session_spawn_for_a_tool_path_outside_the_allowlist() {
    // Given
    let allowlisted = a_tool_that_records_its_parent();
    let unlisted = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(allowlisted.path())
        .start()
        .await;

    // When
    let result = supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(&current_username(), unlisted.path()))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn spawns_an_allowlisted_tool_for_an_allowlisted_user_as_a_child_of_the_supervisor() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    let spawned = supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn an allowlisted tool for an allowlisted user");

    // Then the session belongs to the supervisor, not to whoever asked for it — that is what
    // makes the daemon unable to reach it with signals or ptrace.
    assert!(
        process_is_alive(spawned.pid),
        "spawned session {} is not alive",
        spawned.pid
    );
    assert_eq!(
        tool.await_recorded_parent_pid().await,
        supervisor.pid(),
        "the session was not exec'd by the supervisor"
    );
}
