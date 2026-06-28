//! Red: Linux cgroup v2 limit writing and the fail-fast unsupported-userns contract.
#![cfg(target_os = "linux")]

use tddy_sandbox::SandboxError;
use tddy_sandbox_cgroups::{userns_unsupported_error, write_cgroup_limits, CgroupLimits};

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
