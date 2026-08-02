//! `SpawnSandbox` — the supervisor builds the jail so the daemon does not have to be able to.
//!
//! Only the policy gate is asserted here. Building a real jail needs unprivileged user namespaces,
//! which a host with `kernel.apparmor_restrict_unprivileged_userns=1` denies to any binary without a
//! matching AppArmor profile — so a test that jailed for real would pass or fail depending on the
//! machine it ran on. The *ordering* of the jail's steps is pinned instead by the `pre_exec_plan`
//! unit tests in `src/spawn_broker.rs`, which run anywhere; jailing end-to-end is operator smoke.
//!
//! What is asserted here runs identically on every host: a request naming something policy does not
//! permit is refused before any syscall happens.

mod support;

use std::path::Path;

use support::{
    a_service, a_supervisor, a_tool_that_records_its_parent, current_username, DenialAssertions,
};
use tddy_supervisor::{SandboxMount, SpawnSandboxRequest};

fn a_sandbox_spawn(os_user: &str, tool_path: &Path) -> SpawnSandboxRequest {
    SpawnSandboxRequest {
        os_user: os_user.to_string(),
        tool_path: tool_path.to_path_buf(),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        scope: None,
        mounts: Vec::new(),
        isolate_network: true,
    }
}

fn mounting(source: &str, target: &str) -> Vec<SandboxMount> {
    vec![SandboxMount {
        source: std::path::PathBuf::from(source),
        target: std::path::PathBuf::from(target),
        readonly: false,
    }]
}

#[tokio::test]
async fn rejects_a_sandbox_spawn_for_an_os_user_outside_the_allowlist() {
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
        .spawn_sandbox(a_sandbox_spawn(&current_username(), tool.path()))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn rejects_a_sandbox_spawn_for_a_tool_path_outside_the_allowlist() {
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
        .spawn_sandbox(a_sandbox_spawn(&current_username(), unlisted.path()))
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn rejects_a_sandbox_spawn_whose_mount_source_is_outside_every_allowed_root() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .allowing_mount_root(Path::new("/srv/tddy/repos"))
        .start()
        .await;

    // When a caller asks to bind something it was never granted
    let result = supervisor
        .client()
        .await
        .spawn_sandbox(SpawnSandboxRequest {
            mounts: mounting("/etc", "/etc"),
            ..a_sandbox_spawn(&current_username(), tool.path())
        })
        .await;

    // Then — the mount list is the one part of a sandbox request that names host paths, so it is
    // the part a compromised daemon would reach for.
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn rejects_a_sandbox_spawn_whose_mount_source_escapes_an_allowed_root_by_traversal() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .allowing_mount_root(Path::new("/srv/tddy/repos"))
        .start()
        .await;

    // When
    let result = supervisor
        .client()
        .await
        .spawn_sandbox(SpawnSandboxRequest {
            mounts: mounting("/srv/tddy/repos/../../../etc", "/etc"),
            ..a_sandbox_spawn(&current_username(), tool.path())
        })
        .await;

    // Then
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn rejects_a_sandbox_spawn_whose_mount_source_only_shares_a_prefix_with_an_allowed_root() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .allowing_mount_root(Path::new("/srv/tddy/repos"))
        .start()
        .await;

    // When
    let result = supervisor
        .client()
        .await
        .spawn_sandbox(SpawnSandboxRequest {
            mounts: mounting("/srv/tddy/repos-backup", "/workspace"),
            ..a_sandbox_spawn(&current_username(), tool.path())
        })
        .await;

    // Then — containment is by path component, not by string prefix.
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn rejects_a_sandbox_spawn_when_the_policy_grants_no_mount_roots_at_all() {
    // Given a policy that permits sessions and tools but no mounts
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    let result = supervisor
        .client()
        .await
        .spawn_sandbox(SpawnSandboxRequest {
            mounts: mounting("/srv/tddy/repos/alice", "/workspace"),
            ..a_sandbox_spawn(&current_username(), tool.path())
        })
        .await;

    // Then — an operator who wrote no mount policy granted no mounts.
    result.assert_denied_without_disclosure();
}
