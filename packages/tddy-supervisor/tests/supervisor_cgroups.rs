//! Cgroup v2 scope lifecycle.
//!
//! The supervisor writes into whatever base its config names. Production names
//! `/sys/fs/cgroup/...`; these tests name a temp directory. The code path is identical, so the
//! scope layout, the clamping and the cleanup are all covered on any host, root or not.

mod support;

// Used only by the spawning tests below, which are Linux-only.
#[cfg(target_os = "linux")]
use std::path::Path;

use support::{a_service, a_supervisor, ScopeAssertions};
// Used only by the spawning tests below, which are Linux-only.
#[cfg(target_os = "linux")]
use support::{a_tool_that_records_its_parent, current_username};
use tddy_supervisor::{AppliedLimits, CreateScopeRequest, RequestedLimits};
// Used only by the spawning tests below, which are Linux-only.
#[cfg(target_os = "linux")]
use tddy_supervisor::SpawnSessionRequest;

const MIB: u64 = 1024 * 1024;

fn a_scope(name: &str) -> CreateScopeRequest {
    CreateScopeRequest {
        name: name.to_string(),
        limits: RequestedLimits::default(),
    }
}

fn limits(memory_max: u64, cpu_max: &str, pids_max: u64) -> RequestedLimits {
    RequestedLimits {
        memory_max: Some(memory_max),
        cpu_max: Some(cpu_max.to_string()),
        pids_max: Some(pids_max),
    }
}

// Used only by the spawning tests below, which are Linux-only.
#[cfg(target_os = "linux")]
fn a_session_spawn(os_user: &str, tool_path: &Path, scope: &str) -> SpawnSessionRequest {
    SpawnSessionRequest {
        os_user: os_user.to_string(),
        tool_path: tool_path.to_path_buf(),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        scope: Some(scope.to_string()),
    }
}

#[tokio::test]
async fn creates_a_scope_with_the_requested_limits_when_they_are_under_the_ceiling() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .with_memory_ceiling(512 * MIB)
        .with_cpu_ceiling("400000 100000")
        .with_pids_ceiling(512)
        .start()
        .await;

    // When
    let scope = supervisor
        .client()
        .await
        .create_scope(CreateScopeRequest {
            limits: limits(128 * MIB, "200000 100000", 64),
            ..a_scope("session-alpha")
        })
        .await
        .expect("create a cgroup scope");

    // Then
    scope
        .assert_directory_exists()
        .assert_applied_limits(AppliedLimits {
            memory_max: Some(128 * MIB),
            cpu_max: Some("200000 100000".to_string()),
            pids_max: Some(64),
        })
        .assert_wrote("memory.max", "134217728")
        .assert_wrote("cpu.max", "200000 100000")
        .assert_wrote("pids.max", "64");
}

#[tokio::test]
async fn clamps_requested_limits_down_to_the_policy_ceiling() {
    // Given a caller asking for four times the memory, four times the cpu and eight times the
    // pids the root-owned policy permits.
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .with_memory_ceiling(64 * MIB)
        .with_cpu_ceiling("100000 100000")
        .with_pids_ceiling(64)
        .start()
        .await;

    // When
    let scope = supervisor
        .client()
        .await
        .create_scope(CreateScopeRequest {
            limits: limits(256 * MIB, "400000 100000", 512),
            ..a_scope("session-greedy")
        })
        .await
        .expect("create a cgroup scope with over-ceiling limits");

    // Then the request is honored at the ceiling rather than rejected — a session that asks for
    // too much gets less, it does not fail to start.
    scope
        .assert_applied_limits(AppliedLimits {
            memory_max: Some(64 * MIB),
            cpu_max: Some("100000 100000".to_string()),
            pids_max: Some(64),
        })
        .assert_wrote("memory.max", "67108864")
        .assert_wrote("cpu.max", "100000 100000")
        .assert_wrote("pids.max", "64");
}

/// Makes the supervisor spawn for real, so it is Linux-only — see the note in `support`'s
/// header.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn places_a_spawned_session_into_the_scope_it_asked_for() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .with_memory_ceiling(512 * MIB)
        .start()
        .await;
    let client = supervisor.client().await;
    let scope = client
        .create_scope(a_scope("session-placed"))
        .await
        .expect("create a cgroup scope");

    // When
    let spawned = client
        .spawn_session(a_session_spawn(
            &current_username(),
            tool.path(),
            "session-placed",
        ))
        .await
        .expect("spawn a session into an existing scope");

    // Then
    scope.assert_wrote("cgroup.procs", &spawned.pid.to_string());
}

#[tokio::test]
async fn removes_the_scope_directory_when_the_scope_is_destroyed() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .start()
        .await;
    let client = supervisor.client().await;
    let scope = client
        .create_scope(a_scope("session-transient"))
        .await
        .expect("create a cgroup scope");
    scope.assert_directory_exists();

    // When
    client
        .destroy_scope("session-transient")
        .await
        .expect("destroy the cgroup scope");

    // Then
    assert!(
        !scope.path.exists(),
        "scope directory {} outlived its session",
        scope.path.display()
    );
}
