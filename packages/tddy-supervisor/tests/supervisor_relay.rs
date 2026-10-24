//! A coding-agent harness run as another OS user with its stdio relayed over a socket the caller owns.
//!
//! The policy and peer gates are asserted on every host: they refuse before anything is forked.
//! What needs a live child — the bytes actually flowing, the child's exit and stop — makes the
//! supervisor spawn, so it is Linux-only like every other spawning test (see `support`'s header).
//! None of it needs root: the target user is whoever runs the tests, so no privilege is dropped;
//! that the drop to a *different* user happens before exec is the pre-exec plan's own, separately
//! tested contract, and is what cannot be shown without root here.

mod support;

use std::path::Path;

use support::{
    a_service, a_supervisor, a_tool_that_echoes_its_stdin, current_username, DenialAssertions,
};
use tddy_supervisor::{SpawnRelayedRequest, SpawnSessionRequest};
use tokio::net::UnixListener;

/// How long the live-child test waits for the supervisor's connection and for the echo.
#[cfg(target_os = "linux")]
const RELAY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

fn a_relayed_spawn(tool: &Path, socket: &Path) -> SpawnRelayedRequest {
    SpawnRelayedRequest {
        os_user: current_username(),
        tool_path: tool.to_path_buf(),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        relay_socket: socket.to_path_buf(),
    }
}

fn a_session_spawn(tool: &Path) -> SpawnSessionRequest {
    SpawnSessionRequest {
        os_user: current_username(),
        tool_path: tool.to_path_buf(),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        scope: None,
    }
}

fn a_listening_socket() -> (tempfile::TempDir, std::path::PathBuf, UnixListener) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("relay.sock");
    let listener = UnixListener::bind(&path).expect("bind relay socket");
    (dir, path, listener)
}

#[tokio::test]
async fn relaying_is_off_unless_the_operator_allowlisted_the_harness_for_it() {
    // Given a tool that SpawnSession may run, and no relay allowlist at all
    let tool = a_tool_that_echoes_its_stdin();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;
    let (_dir, socket, _listener) = a_listening_socket();

    // When
    let result = supervisor
        .client()
        .await
        .spawn_relayed(a_relayed_spawn(tool.path(), &socket))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn a_harness_allowlisted_only_for_relaying_cannot_be_spawned_as_a_session() {
    // Given a harness on the relay allowlist and on no session allowlist
    let tool = a_tool_that_echoes_its_stdin();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_relay_tool(tool.path())
        .start()
        .await;

    // When it is asked for through SpawnSession
    let result = supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(tool.path()))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn a_relayed_spawn_for_a_user_outside_the_allowlist_is_denied() {
    // Given a relay-allowlisted harness, and a session allowlist without the requesting user
    let tool = a_tool_that_echoes_its_stdin();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_session_user("someone-else")
        .allowing_relay_tool(tool.path())
        .start()
        .await;
    let (_dir, socket, _listener) = a_listening_socket();

    // When
    let result = supervisor
        .client()
        .await
        .spawn_relayed(a_relayed_spawn(tool.path(), &socket))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn a_relayed_spawn_from_a_peer_that_owns_no_declared_service_is_denied() {
    // Given a supervisor that manages no service, so no caller is one of its services
    let tool = a_tool_that_echoes_its_stdin();
    let supervisor = a_supervisor()
        .allowing_the_current_user()
        .allowing_relay_tool(tool.path())
        .start()
        .await;
    let (_dir, socket, _listener) = a_listening_socket();

    // When
    let result = supervisor
        .client()
        .await
        .spawn_relayed(a_relayed_spawn(tool.path(), &socket))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn a_relay_path_that_is_not_the_callers_listening_socket_is_denied() {
    // Given a relay path naming a plain file rather than a listening socket
    let tool = a_tool_that_echoes_its_stdin();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_relay_tool(tool.path())
        .start()
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let not_a_socket = dir.path().join("plain-file");
    std::fs::write(&not_a_socket, b"x").expect("write");

    // When
    let result = supervisor
        .client()
        .await
        .spawn_relayed(a_relayed_spawn(tool.path(), &not_a_socket))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

/// Makes the supervisor spawn for real, so it is Linux-only — see this file's header.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn an_allowlisted_harness_has_its_stdio_relayed_and_its_exit_reported() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Given
    let tool = a_tool_that_echoes_its_stdin();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_relay_tool(tool.path())
        .start()
        .await;
    let client = supervisor.client().await;
    let (_dir, socket, listener) = a_listening_socket();

    // When
    let spawned = client
        .spawn_relayed(a_relayed_spawn(tool.path(), &socket))
        .await
        .expect("spawn relayed");
    let (mut agent, _) = tokio::time::timeout(RELAY_DEADLINE, listener.accept())
        .await
        .expect("the supervisor connects within the deadline")
        .expect("accept the supervisor's connection");
    agent.write_all(b"ping\n").await.expect("write");
    let mut echoed = [0u8; 5];
    tokio::time::timeout(RELAY_DEADLINE, agent.read_exact(&mut echoed))
        .await
        .expect("the echo arrives within the deadline")
        .expect("read the echo");

    // Then the bytes crossed both ways, and hanging up ends the child, whose exit is reported.
    assert_eq!(&echoed, b"ping\n");
    drop(agent);
    let status = supervisor.await_session_exit(&client, spawned.pid).await;
    assert_eq!(status.pid, spawned.pid);
}
