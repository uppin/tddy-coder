//! Unit tests for `tddy_vm::vm_manifest::VmManifest` — the per-VM manifest persisted
//! as `vm/<name>/manifest.yaml`: run policy, login policy, and prepared-base reference.
//! These already pass once the struct/derive shape below compiles (serde round-trips
//! are not hand-written logic) — the point is to lock in the exact public shape that
//! `VmLibrary`/`VmManager` depend on.
//!
//! The last section covers what `VmLibrary::create_vm` *persists into* that manifest, which
//! is the same contract seen from the writing side.

use tddy_vm::library::VmLibrary;
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
use tddy_vm::{VmAccel, VmArch};
use tempfile::tempdir;

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
            arch: VmArch::host(),
            accel: VmAccel::host_default(),
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

// ── Backwards compatibility with manifests written before arch/accel existed ──

/// `RunPolicy::arch`/`accel` carry `#[serde(default = …)]` specifically so manifests written
/// before those fields existed keep parsing. Nothing else exercises that, and the failure
/// mode is an unreadable library: every VM defined before the upgrade disappears from
/// `list_manifests`, which swallows manifests it cannot parse.
#[test]
fn a_manifest_written_before_arch_and_accel_existed_still_parses_with_host_defaults() {
    // Given a manifest persisted by an older build, with no arch: or accel: keys
    let legacy_yaml = "\
name: web
prepared_base: debian-12
run:
  memory: 2048M
  cpus: 2
  disk_size: 20G
  ssh_host_port: 2222
  port_forwards: []
login:
  username: tddy
";

    // When it is read back
    let decoded: VmManifest = serde_yml::from_str(legacy_yaml).unwrap();

    // Then it parses, and the guest is assumed to be a host-architecture one — which it
    // necessarily was, since that is all the older build could run
    assert_eq!(decoded.name, "web");
    assert_eq!(decoded.run.arch, VmArch::host());
    assert_eq!(decoded.run.accel, VmAccel::host_default());
}

#[test]
fn yaml_round_trip_preserves_a_non_default_arch_and_accel() {
    // Given a manifest for an emulated guest of the other architecture — the case the
    // defaults would silently overwrite
    let mut manifest = a_vm_manifest();
    manifest.run.arch = VmArch::X86_64;
    manifest.run.accel = VmAccel::Tcg;

    // When serialized to YAML and parsed back
    let yaml = serde_yml::to_string(&manifest).unwrap();
    let decoded: VmManifest = serde_yml::from_str(&yaml).unwrap();

    // Then both survive exactly, rather than falling back to the host's
    assert_eq!(decoded.run.arch, VmArch::X86_64);
    assert_eq!(decoded.run.accel, VmAccel::Tcg);
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

// ── What VmLibrary::create_vm persists into the manifest ─────────────────────

/// Place a real (tiny, empty) qcow2 in `images/02-prepared-base/` for an overlay to be
/// chained onto — `qemu-img create -b` refuses a backing file it cannot open.
fn a_prepared_base_in(library: &VmLibrary, name: &str) {
    let path = library.prepared_base_dir().join(format!("{name}.qcow2"));
    let status = std::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&path)
        .arg("1M")
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        status.status.success(),
        "qemu-img create failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

/// A manifest whose login policy already names keys, as a caller building one by hand would.
fn a_manifest_naming_the_callers_own_keys() -> VmManifest {
    let mut manifest = a_vm_manifest();
    manifest.login.ssh_private_key = Some("/home/dev/.ssh/id_ed25519".to_string());
    manifest.login.ssh_public_key = Some("/home/dev/.ssh/id_ed25519.pub".to_string());
    manifest
}

#[tokio::test]
async fn create_vm_supersedes_the_callers_login_keys_with_the_pair_it_generates() {
    // Given a library holding a prepared base, and a manifest naming the caller's own keys
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    a_prepared_base_in(&library, "debian-12");

    // When the VM is created from that prepared base
    library
        .create_vm(&a_manifest_naming_the_callers_own_keys())
        .await
        .unwrap();

    // Then the persisted manifest points at the per-VM pair generated into vm/web/, so the
    // launcher reaches the guest as its own user with its own key rather than falling back
    // to root with whatever the ambient agent holds
    let persisted = library.read_manifest("web").unwrap();
    let vm_dir = library.vm_dir("web");
    assert_eq!(
        persisted.login.ssh_private_key,
        Some(vm_dir.join("id_web").display().to_string())
    );
    assert_eq!(
        persisted.login.ssh_public_key,
        Some(vm_dir.join("id_web.pub").display().to_string())
    );
}

#[tokio::test]
async fn create_vm_writes_the_generated_keypair_next_to_the_manifest_it_records_it_in() {
    // Given a library holding a prepared base, and a manifest naming the caller's own keys
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    a_prepared_base_in(&library, "debian-12");

    // When the VM is created from that prepared base
    library
        .create_vm(&a_manifest_naming_the_callers_own_keys())
        .await
        .unwrap();

    // Then the recorded paths are not a promise about files that were never written
    let vm_dir = library.vm_dir("web");
    assert!(vm_dir.join("id_web").is_file(), "private key must exist");
    assert!(vm_dir.join("id_web.pub").is_file(), "public key must exist");
}
