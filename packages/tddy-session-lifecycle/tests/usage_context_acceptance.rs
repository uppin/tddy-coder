//! Acceptance: every launch of a coding-agent session records the usage context the daemon later
//! evaluates the agent's `usage:` binding against — which spec, where the agent's HOME is as a host
//! path, its conversation id and its worktree (`#usage-binding 2/5`). Without it a terminal session
//! has no usage at all, and a jailed one is read from the wrong HOME.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-usage-binding-daemon-stream.md
//!
//! The daemon is `agent_registry_support`'s: the real `DaemonSessionHost` over a copy of the
//! built-in `claude.yaml` dropped in as `acme`, its binary a stub. The jailed start runs a real
//! Seatbelt jail, so it is macOS-only.

mod agent_registry_support;
mod support;

use agent_registry_support::{
    a_registry_daemon, builtin_yaml_as, current_os_user, ARegistryDaemon,
};
use pretty_assertions::assert_eq;
use tddy_service::proto::session::StartSessionRequest;
use tddy_usage::read_usage_context;

const COPY_ID: &str = "acme";
const COPY_SESSION_TYPE: &str = "acme-cli";

fn a_daemon_with_a_claude_copy() -> ARegistryDaemon {
    a_registry_daemon()
        .with_tool("acme.yaml", &builtin_yaml_as("claude", COPY_ID))
        .build()
}

fn on_a_named_branch(start: StartSessionRequest) -> StartSessionRequest {
    StartSessionRequest {
        new_branch_name: "feature/usage-context".to_string(),
        ..start
    }
}

/// `(spec_id, agent_home, conversation_id, worktree)` of the session's recorded context.
fn the_recorded_context(
    daemon: &ARegistryDaemon,
    session_id: &str,
) -> (
    String,
    Option<std::path::PathBuf>,
    Option<String>,
    Option<std::path::PathBuf>,
) {
    let context = read_usage_context(&daemon.session_dir(session_id))
        .unwrap_or_else(|| panic!("session {session_id} recorded no usage context"));
    (
        context.spec_id,
        context.agent_home,
        context.conversation_id,
        context.worktree,
    )
}

#[tokio::test]
#[serial_test::serial]
async fn a_host_terminal_session_records_the_os_users_home() {
    // Given
    let daemon = a_daemon_with_a_claude_copy();

    // When
    let (session_id, _host) = daemon
        .start(on_a_named_branch(daemon.a_start(COPY_SESSION_TYPE, false)))
        .await
        .expect("a host acme-cli start succeeds");

    // Then — the agent runs with the OS user's home, and its conversation is the session id
    assert_eq!(
        the_recorded_context(&daemon, &session_id),
        (
            COPY_ID.to_string(),
            tddy_daemon_kernel::user_paths::home_dir_for_user(&current_os_user()),
            Some(session_id.clone()),
            Some(daemon.worktree_of(&session_id)),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_resumed_session_keeps_its_usage_context() {
    // Given
    let daemon = a_daemon_with_a_claude_copy();
    let (session_id, _host) = daemon
        .start(on_a_named_branch(daemon.a_start(COPY_SESSION_TYPE, false)))
        .await
        .expect("a host acme-cli start succeeds");
    daemon.stop(&session_id).await;
    std::fs::remove_file(
        daemon
            .session_dir(&session_id)
            .join(tddy_usage::USAGE_CONTEXT_FILE),
    )
    .expect("the start recorded a usage context");

    // When
    daemon
        .resume(&session_id)
        .await
        .expect("the stopped acme-cli session resumes");

    // Then — the resume recorded it again
    assert_eq!(
        the_recorded_context(&daemon, &session_id).2,
        Some(session_id.clone())
    );
}

#[cfg(target_os = "macos")]
mod jailed {
    use pretty_assertions::assert_eq;

    use super::{
        a_daemon_with_a_claude_copy, on_a_named_branch, the_recorded_context, COPY_SESSION_TYPE,
    };

    #[tokio::test]
    #[serial_test::serial]
    async fn a_jailed_session_records_its_jail_home_as_the_agent_home() {
        // Given — the copy keeps claude.yaml's `home: { kind: persistent, key: claude }`
        let daemon = a_daemon_with_a_claude_copy();

        // When
        let (session_id, _jail) = daemon
            .start(on_a_named_branch(daemon.a_start(COPY_SESSION_TYPE, true)))
            .await
            .expect("a jailed acme-cli start succeeds");

        // Then
        let home = daemon
            .tddy_home
            .join("homes")
            .join("claude")
            .canonicalize()
            .expect("the jail's persistent home exists");
        assert_eq!(the_recorded_context(&daemon, &session_id).1, Some(home));
    }
}
