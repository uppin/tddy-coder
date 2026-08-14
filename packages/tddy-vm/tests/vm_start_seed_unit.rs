//! Unit tests for the NoCloud seed a `VmManager::start` attaches to a boot.
//!
//! A VM the library created from a prepared base has a per-VM keypair, and the only thing
//! that authorizes that key in the guest is the seed written beside it. A VM pointed at an
//! image the library did not provision has no seed at all, and attaching a missing cdrom
//! fails the boot — so the manifest's own discriminator has to decide.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tddy_vm::vm::{ForwardHandle, PortForward, RunningVm, VerifyResult, Vm, VmConfig, VmError};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
use tddy_vm::{VmAccel, VmArch, VmLibrary, VmManager};
use tempfile::{tempdir, TempDir};

/// A backend that keeps every `VmConfig` it is asked to boot, so a test can read back
/// exactly what the manager asked QEMU for.
#[derive(Default)]
struct ConfigRecordingVm {
    boots: Mutex<Vec<VmConfig>>,
}

impl ConfigRecordingVm {
    fn only_boot(&self) -> VmConfig {
        let boots = self.boots.lock().unwrap();
        assert_eq!(boots.len(), 1, "expected exactly one boot");
        boots[0].clone()
    }
}

#[async_trait]
impl Vm for ConfigRecordingVm {
    async fn boot(&self, config: &VmConfig) -> Result<RunningVm, VmError> {
        self.boots.lock().unwrap().push(config.clone());
        Ok(RunningVm {
            ssh_host_port: config.ssh_host_port,
            monitor_socket: "/tmp/tddy-recording-monitor.sock".to_string(),
            pid: 4242,
            login: config.login.clone(),
        })
    }

    async fn deploy(&self, _vm: &RunningVm, _steps: &[String]) -> Result<(), VmError> {
        unreachable!("this backend only records boots")
    }

    async fn verify(&self, _vm: &RunningVm, _command: &str) -> Result<VerifyResult, VmError> {
        unreachable!("this backend only records boots")
    }

    async fn forward(
        &self,
        _vm: &RunningVm,
        _port_forward: &PortForward,
    ) -> Result<ForwardHandle, VmError> {
        unreachable!("this backend only records boots")
    }

    async fn shutdown(&self, _vm: RunningVm) -> Result<(), VmError> {
        unreachable!("this backend only records boots")
    }
}

/// A library-backed manager holding exactly `manifest`, over a boot-recording backend.
/// x86_64 throughout, so a start reaches the backend without resolving UEFI firmware from
/// the ambient environment.
fn a_library_manager_holding(
    manifest: &VmManifest,
) -> (TempDir, VmManager, Arc<ConfigRecordingVm>) {
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    library.write_manifest(manifest).unwrap();
    let backend = Arc::new(ConfigRecordingVm::default());
    let manager = VmManager::from_library(library, Box::new(Arc::clone(&backend)));
    (dir, manager, backend)
}

/// A manifest for `name` that the library created from the prepared base `prepared_base`.
fn a_prepared_base_vm(name: &str, prepared_base: &str) -> VmManifest {
    VmManifest {
        prepared_base: Some(prepared_base.to_string()),
        image_path: None,
        ..a_vm_manifest(name)
    }
}

/// A manifest for `name` pointed straight at an image the library does not manage.
fn a_supplied_image_vm(name: &str, image_path: &str) -> VmManifest {
    VmManifest {
        prepared_base: None,
        image_path: Some(image_path.to_string()),
        ..a_vm_manifest(name)
    }
}

fn a_vm_manifest(name: &str) -> VmManifest {
    VmManifest {
        name: name.to_string(),
        prepared_base: None,
        image_path: None,
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: "20G".to_string(),
            ssh_host_port: 2222,
            port_forwards: vec![],
            arch: VmArch::X86_64,
            accel: VmAccel::Tcg,
        },
        login: LoginPolicy {
            username: "tddy".to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    }
}

#[tokio::test]
async fn starting_a_vm_created_from_a_prepared_base_attaches_the_seed_authorizing_its_own_key() {
    // Given — a library VM created from a prepared base, so `create_vm` wrote it a keypair
    // and the NoCloud seed that authorizes it
    let (dir, manager, backend) =
        a_library_manager_holding(&a_prepared_base_vm("web", "debian-12"));

    // When — it is started
    manager.start("web").await.unwrap();

    // Then — the boot carries that seed as its cdrom; without it the guest authorizes only
    // the key the prepared base's own bake was seeded with, which the host does not hold
    assert_eq!(
        backend.only_boot().seed_iso,
        Some(dir.path().join("vm/web/web-seed.iso").display().to_string())
    );
}

#[tokio::test]
async fn starting_a_vm_from_a_supplied_image_attaches_no_seed() {
    // Given — a library VM pointed at an image the library never provisioned
    let (_dir, manager, backend) =
        a_library_manager_holding(&a_supplied_image_vm("byo", "/images/byo.qcow2"));

    // When — it is started
    manager.start("byo").await.unwrap();

    // Then — no cdrom is attached: there is no seed for such a VM, and naming a missing one
    // would fail the boot
    assert_eq!(backend.only_boot().seed_iso, None);
}
