//! Unit tests for `tddy_vm::vm_manifest::VmManifest` — the per-VM manifest persisted
//! as `vm/<name>/manifest.yaml`: run policy, login policy, and prepared-base reference.
//! These already pass once the struct/derive shape below compiles (serde round-trips
//! are not hand-written logic) — the point is to lock in the exact public shape that
//! `VmLibrary`/`VmManager` depend on.

use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};

fn a_vm_manifest() -> VmManifest {
    VmManifest {
        name: "web".to_string(),
        prepared_base: Some("debian-12".to_string()),
        image_path: None,
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: "20G".to_string(),
            ssh_host_port: 2222,
            port_forwards: vec![tddy_vm::PortForward {
                host_port: 8080,
                guest_port: 80,
            }],
        },
        login: LoginPolicy {
            username: "tddy".to_string(),
            ssh_private_key: Some("id_web".to_string()),
            ssh_public_key: Some("id_web.pub".to_string()),
        },
    }
}

#[test]
fn yaml_round_trip_preserves_the_prepared_base_reference_and_run_policy() {
    // Given a manifest referencing a prepared base with a custom run policy
    let manifest = a_vm_manifest();

    // When serialized to YAML and parsed back
    let yaml = serde_yml::to_string(&manifest).unwrap();
    let decoded: VmManifest = serde_yml::from_str(&yaml).unwrap();

    // Then the prepared-base reference and run policy fields survive exactly
    assert_eq!(decoded.prepared_base.as_deref(), Some("debian-12"));
    assert_eq!(decoded.run.memory, "2048M");
    assert_eq!(decoded.run.cpus, 2);
    assert_eq!(decoded.run.disk_size, "20G");
    assert_eq!(decoded.run.ssh_host_port, 2222);
    assert_eq!(decoded.run.port_forwards.len(), 1);
    assert_eq!(decoded.run.port_forwards[0].host_port, 8080);
    assert_eq!(decoded.run.port_forwards[0].guest_port, 80);
}

#[test]
fn yaml_round_trip_preserves_the_login_policy_and_ssh_key_paths() {
    // Given a manifest with a login policy pointing at generated SSH keys
    let manifest = a_vm_manifest();

    // When serialized to YAML and parsed back
    let yaml = serde_yml::to_string(&manifest).unwrap();
    let decoded: VmManifest = serde_yml::from_str(&yaml).unwrap();

    // Then the login username and SSH key paths survive exactly
    assert_eq!(decoded.login.username, "tddy");
    assert_eq!(decoded.login.ssh_private_key.as_deref(), Some("id_web"));
    assert_eq!(decoded.login.ssh_public_key.as_deref(), Some("id_web.pub"));
}

#[test]
fn image_path_and_prepared_base_are_mutually_exclusive_alternatives_like_vm_spec() {
    // Given a manifest that runs an existing, library-unmanaged qcow2 directly instead
    // of a prepared-base-derived overlay
    let mut manifest = a_vm_manifest();
    manifest.prepared_base = None;
    manifest.image_path = Some("/unmanaged/custom.qcow2".to_string());

    // When serialized to YAML and parsed back
    let yaml = serde_yml::to_string(&manifest).unwrap();
    let decoded: VmManifest = serde_yml::from_str(&yaml).unwrap();

    // Then prepared_base is absent and image_path carries the direct path — mirroring
    // VmSpec's existing build_target/image_path duality
    assert!(decoded.prepared_base.is_none());
    assert_eq!(
        decoded.image_path.as_deref(),
        Some("/unmanaged/custom.qcow2")
    );
}

#[test]
fn absent_optional_fields_are_omitted_from_the_rendered_yaml() {
    // Given a manifest with no image_path and no ssh keys set
    let mut manifest = a_vm_manifest();
    manifest.login.ssh_private_key = None;
    manifest.login.ssh_public_key = None;

    // When serialized to YAML
    let yaml = serde_yml::to_string(&manifest).unwrap();

    // Then the omitted fields do not appear at all, keeping the manifest readable
    assert!(
        !yaml.contains("image_path"),
        "unset image_path must be omitted, got:\n{yaml}"
    );
    assert!(
        !yaml.contains("ssh_private_key"),
        "unset ssh_private_key must be omitted, got:\n{yaml}"
    );
}
