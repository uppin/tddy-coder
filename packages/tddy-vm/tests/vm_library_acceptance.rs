//! VM & Image Library acceptance tests.
//!
//! - `VmManager::from_library` — per-VM `manifest.yaml` files under the library are
//!   the source of truth, superseding the single shared `vm-registry.json`. Fails
//!   until the library-mode branches of `VmManager` are implemented.
//! - `VmLibrary::create_vm` — builds a real per-VM overlay backed by an absolute path
//!   to a prepared base. This is a production test (see module docs on the `#[ignore]`d
//!   fn below): gated on `TDDY_CLOUDINIT_BASE_IMAGE` pointing at a real prepared-base
//!   qcow2 (e.g. reuse `~/Code/makers-lt`'s already-built
//!   `maker-build/maker-vm/target/debian-12/build/debian-12-base.qcow2` — never
//!   downloaded by this test).

use tddy_vm::library::VmLibrary;
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
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

fn a_vm_manifest(name: &str, prepared_base: &str) -> VmManifest {
    VmManifest {
        name: name.to_string(),
        prepared_base: Some(prepared_base.to_string()),
        image_path: None,
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: "20G".to_string(),
            ssh_host_port: 2222,
            port_forwards: vec![],
        },
        login: LoginPolicy {
            username: "tddy".to_string(),
            ssh_private_key: Some(format!("id_{name}")),
            ssh_public_key: Some(format!("id_{name}.pub")),
        },
    }
}

// ── VmManager::from_library — library is the source of truth ─────────────────

#[tokio::test]
async fn define_persists_a_per_vm_manifest_file_under_the_library_instead_of_a_shared_registry() {
    // Given a fresh library-backed VmManager
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let manager = VmManager::from_library(library, Box::new(MockVm::new()));

    // When a VM is defined
    manager.define(test_spec("web")).await.unwrap();

    // Then a per-VM manifest file exists under vm/web/manifest.yaml
    assert!(
        dir.path()
            .join("vm")
            .join("web")
            .join("manifest.yaml")
            .exists(),
        "expected vm/web/manifest.yaml to exist"
    );

    // And no shared vm-registry.json is written anywhere in the library root
    assert!(
        !dir.path().join("vm-registry.json").exists(),
        "library-backed VmManager must not write the old shared registry file"
    );
}

#[tokio::test]
async fn a_fresh_manager_reloads_previously_defined_vms_from_the_library_on_construction() {
    // Given a library with one VM defined, then the manager is dropped
    let dir = tempdir().unwrap();
    {
        let library = VmLibrary::new(dir.path());
        library.init().unwrap();
        let manager = VmManager::from_library(library, Box::new(MockVm::new()));
        manager.define(test_spec("persist")).await.unwrap();
    }

    // When a new VmManager is constructed from the same library root
    let library2 = VmLibrary::new(dir.path());
    let manager2 = VmManager::from_library(library2, Box::new(MockVm::new()));

    // Then the previously defined VM is loaded from its manifest file
    let vms = manager2.list().await;
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].0.name, "persist");
}

#[tokio::test]
async fn full_lifecycle_define_start_stop_remove_deletes_the_vm_directory() {
    // Given a library-backed VmManager with one VM defined
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let manager = VmManager::from_library(library, Box::new(MockVm::new()));
    manager.define(test_spec("app")).await.unwrap();

    // When started, stopped, then removed
    manager.start("app").await.unwrap();
    assert_eq!(manager.status("app").await.unwrap(), VmState::Running);
    manager.stop("app").await.unwrap();
    assert_eq!(manager.status("app").await.unwrap(), VmState::Stopped);
    manager.remove("app").await.unwrap();

    // Then the VM's entire library directory is deleted, not just removed from memory
    assert!(
        !dir.path().join("vm").join("app").exists(),
        "expected vm/app/ to be deleted from disk"
    );
    let vms = manager.list().await;
    assert!(vms.is_empty());
}

#[tokio::test]
async fn duplicate_define_is_rejected_in_library_mode() {
    // Given a library-backed VmManager with one VM already defined
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let manager = VmManager::from_library(library, Box::new(MockVm::new()));
    manager.define(test_spec("dup")).await.unwrap();

    // When defining again with the same name
    let result = manager.define(test_spec("dup")).await;

    // Then an error is returned, same contract as the JSON-backed manager
    assert!(result.is_err(), "duplicate define must return an error");
}

// ── VmLibrary::create_vm — real overlay creation (production test) ────────────

/// The env var this production test reads its prepared-base path from — a real qcow2
/// that already exists on disk (e.g. makers-lt's built Debian 12 base). There is no
/// bundled or auto-downloaded image; a developer must supply one explicitly to run
/// this test.
const PREPARED_BASE_IMAGE_ENV: &str = "TDDY_CLOUDINIT_BASE_IMAGE";

#[tokio::test]
#[ignore = "production test: shells out to a real `qemu-img`; requires \
            TDDY_CLOUDINIT_BASE_IMAGE pointing at a real prepared-base qcow2 (e.g. \
            reuse makers-lt's already-built debian-12-base.qcow2); run with --ignored"]
#[serial_test::serial(vm_library_create_vm)]
async fn create_vm_builds_a_mutable_overlay_backed_by_the_absolute_prepared_base_path() {
    let Some(base_src) = std::env::var(PREPARED_BASE_IMAGE_ENV).ok() else {
        eprintln!(
            "{PREPARED_BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given a library with a prepared base already in place (copied from the
    // developer-supplied source — never downloaded by this test)
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    let prepared_base_path = library.prepared_base_dir().join("debian-12.qcow2");
    std::fs::copy(&base_src, &prepared_base_path).unwrap();

    let manifest = a_vm_manifest("web", "debian-12");

    // When create_vm builds the per-VM overlay
    let overlay_path = library.create_vm(&manifest).await.unwrap();

    // Then the overlay lives under vm/web/ (not co-located with the prepared base)
    assert_eq!(overlay_path, library.vm_dir("web").join("web.qcow2"));

    // And `qemu-img info` reports the overlay's backing file as the absolute path to
    // the prepared base — not the relative, co-located reference cloud-init uses
    let output = tokio::process::Command::new("qemu-img")
        .args(["info", "--output=json", overlay_path.to_str().unwrap()])
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "qemu-img info failed: {output:?}");
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let backing = info["backing-filename"]
        .as_str()
        .expect("overlay must report a backing file");
    assert_eq!(backing, prepared_base_path.to_str().unwrap());

    // And the manifest was written alongside the overlay
    assert!(library.vm_dir("web").join("manifest.yaml").exists());
}
