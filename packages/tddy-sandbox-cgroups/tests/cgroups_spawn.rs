//! Red: Linux cgroup v2 limit writing and the fail-fast unsupported-userns contract.
#![cfg(target_os = "linux")]

use tddy_sandbox::SandboxError;
use tddy_sandbox_cgroups::{
    enable_controllers, move_pid_into_scope, relocate_self_into_leaf, userns_unsupported_error,
    write_cgroup_limits, CgroupLimits,
};

/// **writes_cgroup_v2_resource_limits_to_the_scope_directory**: each `Some` limit lands in its
/// `*.max` file under the delegated scope.
#[test]
fn writes_cgroup_v2_resource_limits_to_the_scope_directory() {
    // Given
    let scope = tempfile::tempdir().unwrap();
    let limits = CgroupLimits {
        memory_max: Some(536_870_912), // 512 MiB
        cpu_max: Some("50000 100000".to_string()),
        pids_max: Some(128),
    };

    // When
    write_cgroup_limits(scope.path(), &limits).expect("write cgroup limits");

    // Then
    let read = |name: &str| std::fs::read_to_string(scope.path().join(name)).unwrap();
    assert_eq!(read("memory.max").trim(), "536870912");
    assert_eq!(read("cpu.max").trim(), "50000 100000");
    assert_eq!(read("pids.max").trim(), "128");
}

/// **unsupported_error_names_unprivileged_user_namespaces**: the fail-fast error is `Unsupported`
/// for the linux platform and its message points at user namespaces.
#[test]
fn unsupported_error_names_unprivileged_user_namespaces() {
    // When
    let err = userns_unsupported_error();

    // Then
    let SandboxError::Unsupported { platform, message } = err else {
        panic!("expected SandboxError::Unsupported, got {err:?}");
    };
    assert_eq!(platform, "linux");
    assert!(
        message.to_lowercase().contains("user namespace"),
        "remediation message must mention user namespaces, got: {message}"
    );
}

/// **relocates_the_daemon_process_into_the_supervisor_leaf**: preparing the delegated base creates
/// the supervisor leaf and moves the daemon's own pid into it, so the base itself holds no processes
/// and can enable controllers (cgroup v2 no-internal-processes rule).
#[test]
fn relocates_the_daemon_process_into_the_supervisor_leaf() {
    // Given
    let base = tempfile::tempdir().unwrap();

    // When
    relocate_self_into_leaf(base.path(), 4242, "supervisor").expect("relocate into leaf");

    // Then
    let procs = std::fs::read_to_string(base.path().join("supervisor").join("cgroup.procs"))
        .expect("supervisor cgroup.procs written");
    assert_eq!(procs.trim(), "4242");
}

/// **enables_the_configured_controllers_in_subtree_control**: the controllers are written as a
/// `+`-prefixed enable line into the base's `cgroup.subtree_control`.
#[test]
fn enables_the_configured_controllers_in_subtree_control() {
    // Given
    let base = tempfile::tempdir().unwrap();
    let controllers = vec!["memory".to_string(), "cpu".to_string(), "pids".to_string()];

    // When
    enable_controllers(base.path(), &controllers).expect("enable controllers");

    // Then
    let written = std::fs::read_to_string(base.path().join("cgroup.subtree_control"))
        .expect("subtree_control written");
    assert_eq!(written.trim(), "+memory +cpu +pids");
}

/// **moves_the_child_pid_into_the_session_scope**: the sandboxed child's pid is written into the
/// session scope's `cgroup.procs`.
#[test]
fn moves_the_child_pid_into_the_session_scope() {
    // Given
    let scope = tempfile::tempdir().unwrap();

    // When
    move_pid_into_scope(scope.path(), 9182).expect("move pid into scope");

    // Then
    let procs =
        std::fs::read_to_string(scope.path().join("cgroup.procs")).expect("scope cgroup.procs");
    assert_eq!(procs.trim(), "9182");
}
