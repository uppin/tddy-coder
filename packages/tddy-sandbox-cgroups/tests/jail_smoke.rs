//! Verifies the rootless jail mechanics: the spawned process runs as root inside a user namespace
//! and sees an isolated network namespace (only `lo`, no host interfaces → no direct egress).
//!
//! Requires a host that permits unprivileged user namespaces; on hosts that don't (e.g. Ubuntu with
//! AppArmor's userns restriction), the jail can't be created and the test self-skips. Run it in a
//! userns-capable environment (root, or a privileged container) to exercise the jail.
#![cfg(target_os = "linux")]

use std::collections::BTreeMap;

use tddy_sandbox::SandboxSpec;
use tddy_sandbox_cgroups::{spawn, unprivileged_userns_available};

#[test]
fn runs_as_root_in_an_isolated_user_and_network_namespace() {
    if !unprivileged_userns_available() {
        eprintln!("SKIP: host forbids unprivileged user namespaces (cannot create the jail here)");
        return;
    }

    // Given
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let proof = project_root.join("jail-proof.txt");

    let mut env = BTreeMap::new();
    env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());

    // `/proc/net/dev` reflects the *current* network namespace (unlike `/sys/class/net`, which
    // mirrors the netns its sysfs mount was created in).
    let script = format!(
        "echo uid=$(id -u) > {p}/jail-proof.txt; \
         echo ifaces=$(cut -d: -f1 /proc/net/dev | tail -n +3 | tr -d ' ' | tr '\\n' ',') >> {p}/jail-proof.txt",
        p = project_root.display()
    );
    let spec = SandboxSpec {
        project_root: project_root.clone(),
        scratch_dir: tmp.path().join("scratch"),
        egress_dir: tmp.path().join("egress"),
        allow_read_paths: vec![],
        command: vec!["/bin/sh".to_string(), "-c".to_string(), script],
        env,
        profile_path: tmp.path().join("profile"),
        loopback_allow_ports: vec![],
        ipc_socket: None,
    };

    // When
    let mut child = spawn(spec).expect("spawn rootless jail").into_child();
    let status = child.wait().expect("await jailed process");

    // Then
    assert!(status.success(), "jailed process exited: {status:?}");
    let text = std::fs::read_to_string(&proof).expect("jail proof written to shared project root");
    assert!(
        text.contains("uid=0"),
        "the uid/gid map should make the process root inside the userns, got:\n{text}"
    );
    assert!(
        text.contains("ifaces=lo,"),
        "the network namespace should expose only loopback (no host interfaces → no direct egress), got:\n{text}"
    );
}
