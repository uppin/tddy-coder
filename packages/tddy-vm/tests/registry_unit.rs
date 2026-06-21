//! Unit tests for VmSpec serde, validation invariants, and VmManager basic ops.
//! VmSpec serde and structural tests pass immediately.
//! VmManager method tests fail until methods are implemented.

use tddy_vm::registry::VmState;
use tddy_vm::{MockVm, VmManager, VmSpec};
use tempfile::tempdir;

// ── VmSpec serde ─────────────────────────────────────────────────────────────

#[test]
fn vm_spec_serde_round_trip_with_image_path() {
    // Given — a spec with image_path set
    let spec = VmSpec {
        name: "web".to_string(),
        build_target: None,
        image_path: Some("/images/web.qcow2".to_string()),
        port_forwards: vec![],
        ssh_host_port: 2222,
    };

    // When — serialized then deserialized
    let json = serde_json::to_string(&spec).unwrap();
    let decoded: VmSpec = serde_json::from_str(&json).unwrap();

    // Then — all fields round-trip correctly
    assert_eq!(decoded.name, "web");
    assert_eq!(decoded.image_path.as_deref(), Some("/images/web.qcow2"));
    assert!(decoded.build_target.is_none());
    assert_eq!(decoded.ssh_host_port, 2222);
}

#[test]
fn vm_spec_serde_round_trip_with_build_target() {
    // Given — a spec with build_target set
    let spec = VmSpec {
        name: "app".to_string(),
        build_target: Some("qemu-minimal".to_string()),
        image_path: None,
        port_forwards: vec![tddy_vm::PortForward {
            host_port: 8080,
            guest_port: 80,
        }],
        ssh_host_port: 2223,
    };

    // When — serialized then deserialized
    let json = serde_json::to_string(&spec).unwrap();
    let decoded: VmSpec = serde_json::from_str(&json).unwrap();

    // Then
    assert_eq!(decoded.build_target.as_deref(), Some("qemu-minimal"));
    assert!(decoded.image_path.is_none());
    assert_eq!(decoded.port_forwards.len(), 1);
    assert_eq!(decoded.port_forwards[0].host_port, 8080);
}

// ── VmManager: unknown-name errors ──────────────────────────────────────────

fn make_manager() -> (tempfile::TempDir, VmManager) {
    let dir = tempdir().unwrap();
    let manager = VmManager::new(&dir.path().join("vms.json"), Box::new(MockVm::new()));
    (dir, manager)
}

#[tokio::test]
async fn start_unknown_vm_returns_error() {
    // Given — a fresh VmManager with no VMs defined
    let (_dir, manager) = make_manager();

    // When — start is called for a name that doesn't exist
    let result = manager.start("ghost").await;

    // Then — an error is returned
    assert!(result.is_err(), "start of unknown VM must return an error");
}

#[tokio::test]
async fn status_unknown_vm_returns_error() {
    // Given — a fresh VmManager with no VMs defined
    let (_dir, manager) = make_manager();

    // When — status is called for a name that doesn't exist
    let result = manager.status("ghost").await;

    // Then — an error is returned
    assert!(result.is_err(), "status of unknown VM must return an error");
}

#[tokio::test]
async fn remove_unknown_vm_returns_error() {
    // Given — a fresh VmManager with no VMs defined
    let (_dir, manager) = make_manager();

    // When — remove is called for a name that doesn't exist
    let result = manager.remove("ghost").await;

    // Then — an error is returned
    assert!(result.is_err(), "remove of unknown VM must return an error");
}

// ── VmManager: define then list ──────────────────────────────────────────────

#[tokio::test]
async fn define_increments_list_count() {
    // Given — a VmManager and two distinct specs
    let (_dir, manager) = make_manager();
    let spec_a = VmSpec {
        name: "alpha".to_string(),
        build_target: None,
        image_path: Some("/a.qcow2".to_string()),
        port_forwards: vec![],
        ssh_host_port: 2222,
    };
    let spec_b = VmSpec {
        name: "beta".to_string(),
        build_target: None,
        image_path: Some("/b.qcow2".to_string()),
        port_forwards: vec![],
        ssh_host_port: 2223,
    };

    // When — both are defined
    manager.define(spec_a).await.unwrap();
    manager.define(spec_b).await.unwrap();

    // Then — list returns exactly two entries
    let vms = manager.list().await;
    assert_eq!(vms.len(), 2);
}

#[tokio::test]
async fn list_returns_defined_state_after_define() {
    // Given — a freshly defined VM
    let (_dir, manager) = make_manager();
    manager
        .define(VmSpec {
            name: "web".to_string(),
            build_target: None,
            image_path: Some("/web.qcow2".to_string()),
            port_forwards: vec![],
            ssh_host_port: 2222,
        })
        .await
        .unwrap();

    // When — list is called
    let vms = manager.list().await;

    // Then — the VM is in Defined state
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].1, VmState::Defined);
}

#[tokio::test]
async fn remove_after_define_empties_list() {
    // Given — one defined VM
    let (_dir, manager) = make_manager();
    manager
        .define(VmSpec {
            name: "temp".to_string(),
            build_target: None,
            image_path: Some("/t.qcow2".to_string()),
            port_forwards: vec![],
            ssh_host_port: 2222,
        })
        .await
        .unwrap();

    // When — remove is called
    manager.remove("temp").await.unwrap();

    // Then — list is empty
    let vms = manager.list().await;
    assert!(vms.is_empty());
}
