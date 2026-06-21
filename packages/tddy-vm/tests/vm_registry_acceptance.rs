//! VM registry acceptance tests — full lifecycle via MockVm backend.
//! Fails until VmManager methods are implemented.

use tddy_vm::{MockVm, VmManager, VmSpec, VmState};
use tempfile::tempdir;

fn test_spec(name: &str) -> VmSpec {
    VmSpec {
        name: name.to_string(),
        build_target: None,
        image_path: Some("/fake/image.qcow2".to_string()),
        port_forwards: vec![],
        ssh_host_port: 2222,
    }
}

fn make_manager(state_file: &std::path::Path) -> VmManager {
    VmManager::new(state_file, Box::new(MockVm::new()))
}

#[tokio::test]
async fn define_and_list_vm() {
    // Given — a fresh VmManager
    let dir = tempdir().unwrap();
    let manager = make_manager(&dir.path().join("vms.json"));

    // When — a VM is defined
    manager.define(test_spec("web")).await.unwrap();

    // Then — list returns it with Defined state
    let vms = manager.list().await;
    assert_eq!(vms.len(), 1);
    let (spec, state) = &vms[0];
    assert_eq!(spec.name, "web");
    assert_eq!(*state, VmState::Defined);
}

#[tokio::test]
async fn full_lifecycle_define_start_stop_remove() {
    // Given — a fresh VmManager with MockVm backend
    let dir = tempdir().unwrap();
    let manager = make_manager(&dir.path().join("vms.json"));

    // When — define → start → status → stop → status → remove
    manager.define(test_spec("app")).await.unwrap();
    manager.start("app").await.unwrap();

    let state = manager.status("app").await.unwrap();
    assert_eq!(state, VmState::Running);

    manager.stop("app").await.unwrap();

    let state = manager.status("app").await.unwrap();
    assert_eq!(state, VmState::Stopped);

    manager.remove("app").await.unwrap();

    let vms = manager.list().await;
    assert!(vms.is_empty(), "removed VM must not appear in list");
}

#[tokio::test]
async fn duplicate_define_is_rejected() {
    // Given — a VmManager with one VM already defined
    let dir = tempdir().unwrap();
    let manager = make_manager(&dir.path().join("vms.json"));
    manager.define(test_spec("dup")).await.unwrap();

    // When — defining again with the same name
    let result = manager.define(test_spec("dup")).await;

    // Then — an error is returned (duplicate names not allowed)
    assert!(result.is_err(), "duplicate define must return an error");
}

#[tokio::test]
async fn spec_persistence_survives_restart() {
    // Given — a VmManager that defines a VM and is dropped
    let dir = tempdir().unwrap();
    let state_file = dir.path().join("vms.json");
    {
        let manager = make_manager(&state_file);
        manager.define(test_spec("persist")).await.unwrap();
    }

    // When — a new VmManager is created pointing at the same state file
    let manager2 = make_manager(&state_file);

    // Then — the previously defined VM is loaded from disk
    let vms = manager2.list().await;
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].0.name, "persist");
}
