//! Unit tests for VmSpec serde, validation invariants, and VmManager basic ops.
//! VmSpec serde and structural tests pass immediately.
//! VmManager method tests fail until methods are implemented.

use std::sync::Arc;

use async_trait::async_trait;
use serial_test::serial;
use tddy_vm::qemu::UEFI_CODE_ENV;
use tddy_vm::registry::VmState;
use tddy_vm::vm::{ForwardHandle, PortForward, RunningVm, VerifyResult, Vm, VmConfig, VmError};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
use tddy_vm::{MockVm, UefiFirmware, VmAccel, VmArch, VmLibrary, VmManager, VmSpec};
use tempfile::{tempdir, TempDir};

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

// ── VmManager: UEFI firmware for a started VM ────────────────────────────────

/// A library-backed manager holding one VM whose manifest names `arch`, plus a handle on
/// the mock backend it boots through so the test can read back the boot it was asked for.
fn a_library_manager_with_vm(arch: VmArch, name: &str) -> (TempDir, VmManager, Arc<MockVm>) {
    let dir = tempdir().unwrap();
    let library = a_library_with_vm(&dir, arch, name);
    let backend = Arc::new(MockVm::new());
    let manager = VmManager::from_library(library, Box::new(Arc::clone(&backend)));
    (dir, manager, backend)
}

/// A library rooted at `dir` holding exactly one VM manifest, for `name` on `arch`.
fn a_library_with_vm(dir: &TempDir, arch: VmArch, name: &str) -> VmLibrary {
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    library
        .write_manifest(&VmManifest {
            name: name.to_string(),
            prepared_base: None,
            image_path: Some(format!("/images/{name}.qcow2")),
            run: RunPolicy {
                memory: "2048M".to_string(),
                cpus: 2,
                disk_size: "20G".to_string(),
                ssh_host_port: 2222,
                port_forwards: vec![],
                arch,
                accel: VmAccel::Tcg,
            },
            login: LoginPolicy {
                username: "tddy".to_string(),
                ssh_private_key: None,
                ssh_public_key: None,
            },
        })
        .unwrap();
    library
}

/// Write a firmware image this test owns, so the assertion does not depend on which QEMU
/// installation happens to be on `$PATH`. Pair it with a [`UefiCodeEnv`] to point firmware
/// resolution at it.
fn a_uefi_firmware_image(dir: &TempDir) -> String {
    let path = dir.path().join("edk2-aarch64-code.fd");
    std::fs::write(&path, b"firmware").unwrap();
    path.display().to_string()
}

/// Points [`UEFI_CODE_ENV`] at a path for as long as it is held, restoring whatever the
/// process had before on drop — a leaked override would otherwise decide the outcome of
/// every later test in this binary.
struct UefiCodeEnv(Option<String>);

impl UefiCodeEnv {
    fn pointing_at(path: &str) -> Self {
        let previous = std::env::var(UEFI_CODE_ENV).ok();
        std::env::set_var(UEFI_CODE_ENV, path);
        Self(previous)
    }
}

impl Drop for UefiCodeEnv {
    fn drop(&mut self) {
        match &self.0 {
            Some(previous) => std::env::set_var(UEFI_CODE_ENV, previous),
            None => std::env::remove_var(UEFI_CODE_ENV),
        }
    }
}

#[tokio::test]
#[serial(uefi_code_env)]
async fn starting_an_aarch64_vm_boots_it_through_a_per_vm_uefi_firmware_pair() {
    // Given — a library VM whose manifest names the aarch64 `virt` machine, which has no BIOS
    let (dir, manager, backend) = a_library_manager_with_vm(VmArch::Aarch64, "arm-guest");
    let code_path = a_uefi_firmware_image(&dir);
    let _uefi_code = UefiCodeEnv::pointing_at(&code_path);

    // When — it is started
    manager.start("arm-guest").await.unwrap();

    // Then — the backend was handed the resolved firmware and this VM's own variables store
    assert_eq!(
        backend.boot_calls()[0].firmware,
        Some(UefiFirmware {
            code_path,
            vars_path: dir
                .path()
                .join("vm/arm-guest/arm-guest-vars.fd")
                .display()
                .to_string(),
        })
    );
}

#[tokio::test]
#[serial(uefi_code_env)]
async fn starting_an_x86_64_vm_boots_it_through_its_own_bios() {
    // Given — a library VM whose manifest names the x86_64 `q35` machine, which has SeaBIOS
    let (dir, manager, backend) = a_library_manager_with_vm(VmArch::X86_64, "intel-guest");

    // When — it is started
    manager.start("intel-guest").await.unwrap();

    // Then — no UEFI pair is resolved, and no variables store is left behind
    assert_eq!(backend.boot_calls()[0].firmware, None);
    assert!(!dir
        .path()
        .join("vm/intel-guest/intel-guest-vars.fd")
        .exists());
}

#[tokio::test]
#[serial(uefi_code_env)]
async fn starting_an_aarch64_vm_with_unresolvable_firmware_fails_instead_of_booting() {
    // Given — a library VM needing UEFI, with the configured firmware image missing
    let (_dir, manager, backend) = a_library_manager_with_vm(VmArch::Aarch64, "arm-guest");
    let _uefi_code = UefiCodeEnv::pointing_at("/nonexistent/edk2-aarch64-code.fd");

    // When — it is started
    let result = manager.start("arm-guest").await;

    // Then — the start fails and nothing was booted, rather than silently falling back to a
    // BIOS boot the `virt` machine cannot do
    assert!(
        result.is_err(),
        "an unresolvable UEFI firmware image must fail the start"
    );
    assert!(backend.boot_calls().is_empty());
    assert_eq!(manager.status("arm-guest").await.unwrap(), VmState::Defined);
}

// ── VmManager: a manifest that can no longer be read ─────────────────────────

#[tokio::test]
async fn starting_a_vm_whose_manifest_is_unreadable_fails_instead_of_guessing_how_to_run_it() {
    // Given — a library VM whose manifest.yaml was truncated after the manager loaded it
    let (dir, manager, backend) = a_library_manager_with_vm(VmArch::X86_64, "intel-guest");
    let manifest_path = dir.path().join("vm/intel-guest/manifest.yaml");
    std::fs::write(&manifest_path, "name: [unterminated").unwrap();

    // When — it is started
    let failure = manager.start("intel-guest").await.unwrap_err().to_string();

    // Then — the start fails naming the manifest, rather than booting the VM as `root` with
    // no key and default resources, which is what a spec-derived manifest would have said.
    // The tail of the message is serde_yml's own diagnostic, which is not ours to pin.
    assert!(
        failure.starts_with(&format!(
            "VM image build failed: failed to parse {}",
            manifest_path.display()
        )),
        "expected the failure to name the unparseable manifest, got: {failure}"
    );
    assert!(backend.boot_calls().is_empty());
}

// ── VmManager: a boot the backend refuses ────────────────────────────────────

/// A backend that always refuses to boot, for the path where QEMU never comes up.
struct UnbootableVm {
    reason: String,
    boot_attempts: std::sync::atomic::AtomicUsize,
}

impl UnbootableVm {
    fn refusing_with(reason: &str) -> Self {
        Self {
            reason: reason.to_string(),
            boot_attempts: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn boot_attempts(&self) -> usize {
        self.boot_attempts
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait]
impl Vm for UnbootableVm {
    async fn boot(&self, _config: &VmConfig) -> Result<RunningVm, VmError> {
        self.boot_attempts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(VmError::BootFailed(self.reason.clone()))
    }

    async fn deploy(&self, _vm: &RunningVm, _steps: &[String]) -> Result<(), VmError> {
        unreachable!("nothing ever boots on this backend, so deploy is never reached")
    }

    async fn verify(&self, _vm: &RunningVm, _command: &str) -> Result<VerifyResult, VmError> {
        unreachable!("nothing ever boots on this backend, so verify is never reached")
    }

    async fn forward(
        &self,
        _vm: &RunningVm,
        _port_forward: &PortForward,
    ) -> Result<ForwardHandle, VmError> {
        unreachable!("nothing ever boots on this backend, so forward is never reached")
    }

    async fn shutdown(&self, _vm: RunningVm) -> Result<(), VmError> {
        unreachable!("nothing ever boots on this backend, so shutdown is never reached")
    }
}

/// A library-backed manager for one x86_64 VM over a backend that cannot boot it. x86_64
/// boots through its own BIOS, so the start reaches the backend without resolving UEFI
/// firmware from the environment.
fn a_manager_that_cannot_boot(reason: &str) -> (TempDir, VmManager, Arc<UnbootableVm>) {
    let dir = tempdir().unwrap();
    let library = a_library_with_vm(&dir, VmArch::X86_64, "intel-guest");
    let backend = Arc::new(UnbootableVm::refusing_with(reason));
    let manager = VmManager::from_library(library, Box::new(Arc::clone(&backend)));
    (dir, manager, backend)
}

#[tokio::test]
async fn a_vm_whose_boot_fails_records_the_reason_rather_than_staying_mid_transition() {
    // Given — a VM over a backend that refuses to boot it
    let (_dir, manager, _backend) = a_manager_that_cannot_boot("no accelerator available");

    // When — it is started
    manager.start("intel-guest").await.unwrap_err();

    // Then — it carries the boot failure, not the Booting state no later call can leave
    assert_eq!(
        manager.status("intel-guest").await.unwrap(),
        VmState::Error("VM boot failed: no accelerator available".to_string())
    );
}

#[tokio::test]
async fn a_vm_can_be_started_again_after_a_failed_boot() {
    // Given — a VM whose first start failed to boot
    let (_dir, manager, backend) = a_manager_that_cannot_boot("no accelerator available");
    manager.start("intel-guest").await.unwrap_err();

    // When — it is started a second time
    let failure = manager.start("intel-guest").await.unwrap_err().to_string();

    // Then — the retry reaches the backend and reports the real reason, rather than being
    // refused as already Booting
    assert_eq!(failure, "VM boot failed: no accelerator available");
    assert_eq!(backend.boot_attempts(), 2);
}
