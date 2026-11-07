//! Acceptance (`#agent-tools` 26/28): on a **server** daemon, a host Cursor or Claude session's
//! hooks report over the agent-tool socket of the OS user the session runs as —
//! `<tddyhome>/run-agent-tools/<os user>/agent.sock` — and that socket is served.
//!
//! Before: a Cursor session's `hooks.json` named the embedded host's digest-named socket under
//! `$TMPDIR`, which a server never binds, so every report was dropped fail-quiet; a Claude
//! session's hooks named no socket at all.
//!
//! The daemon is `agent_registry_support`'s — the real `DaemonSessionHost`, not an embedded one,
//! given the coding-agent registry over its `<tddyhome>/tools` and a stub agent binary. The session
//! runs as the account running the test: serving a second OS user needs root, which only the Linux
//! privileged suites have.
//!
//! Changeset: docs/dev/1-WIP/2026-11-06-agent-tool-socket-per-os-user.md

mod agent_registry_support;
mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_registry_support::{
    a_registry_daemon, builtin_yaml_as, current_os_user, ARegistryDaemon,
};
use pretty_assertions::assert_eq;
use tddy_service::proto::session::StartSessionRequest;

const COPY_ID: &str = "acme";
const COPY_SESSION_TYPE: &str = "acme-cli";
/// claude.yaml declares `branch_name_required`.
const BRANCH: &str = "acme-work";

/// How long the served socket is given to accept before the test calls it unserved.
const THE_SOCKET_ACCEPTS_WITHIN: Duration = Duration::from_secs(10);

/// A server daemon holding `builtin`'s yaml under [`COPY_ID`].
fn a_server_daemon_with_a_copy_of(builtin: &str) -> ARegistryDaemon {
    a_registry_daemon()
        .with_tool("acme.yaml", &builtin_yaml_as(builtin, COPY_ID))
        .build()
}

/// Where `daemon` serves the account running the test — the sessions' OS user.
fn the_session_users_agent_tool_socket(daemon: &ARegistryDaemon) -> PathBuf {
    daemon
        .tddy_home
        .join("run-agent-tools")
        .join(current_os_user())
        .join("agent.sock")
}

/// Every distinct `--daemon-socket` value in a hooks file's commands.
fn daemon_sockets_named_in(hooks_file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(hooks_file)
        .unwrap_or_else(|e| panic!("{} was not written: {e}", hooks_file.display()));
    let mut named: Vec<String> = text
        .split("--daemon-socket")
        .skip(1)
        .filter_map(|rest| rest.split_whitespace().next())
        .map(|value| {
            value
                .trim_matches(|c| matches!(c, '\'' | '"' | '\\' | ','))
                .to_string()
        })
        .collect();
    named.sort();
    named.dedup();
    named
}

/// Whether something accepts on `socket` within [`THE_SOCKET_ACCEPTS_WITHIN`].
async fn is_served(socket: &Path) -> bool {
    let deadline = tokio::time::Instant::now() + THE_SOCKET_ACCEPTS_WITHIN;
    loop {
        if tokio::net::UnixStream::connect(socket).await.is_ok() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_host_cursor_sessions_hooks_report_over_its_users_served_agent_tool_socket() {
    // Given a server daemon with cursor.yaml as acme
    let daemon = a_server_daemon_with_a_copy_of("cursor");

    // When a host session starts
    let (session_id, _host) = daemon
        .start(daemon.a_start(COPY_SESSION_TYPE, false))
        .await
        .expect("a host acme-cli start succeeds");

    // Then its hooks name its user's socket, and that socket is served
    let socket = the_session_users_agent_tool_socket(&daemon);
    let hooks =
        daemon_sockets_named_in(&daemon.worktree_of(&session_id).join(".cursor/hooks.json"));
    assert_eq!(
        (hooks, is_served(&socket).await),
        (vec![socket.to_string_lossy().into_owned()], true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_host_claude_sessions_hooks_report_over_its_users_served_agent_tool_socket() {
    // Given a server daemon with claude.yaml as acme
    let daemon = a_server_daemon_with_a_copy_of("claude");

    // When a host session starts
    let (session_id, _host) = daemon
        .start(StartSessionRequest {
            new_branch_name: BRANCH.to_string(),
            ..daemon.a_start(COPY_SESSION_TYPE, false)
        })
        .await
        .expect("a host acme-cli start succeeds");

    // Then its hooks name its user's socket, and that socket is served
    let socket = the_session_users_agent_tool_socket(&daemon);
    let hooks = daemon_sockets_named_in(
        &daemon
            .worktree_of(&session_id)
            .join(".claude/settings.local.json"),
    );
    assert_eq!(
        (hooks, is_served(&socket).await),
        (vec![socket.to_string_lossy().into_owned()], true)
    );
}
