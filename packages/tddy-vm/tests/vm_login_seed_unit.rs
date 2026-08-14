//! The NoCloud seed a per-VM overlay boots with.
//!
//! `create_vm` mints a fresh keypair for every VM and records it in the manifest, but the
//! prepared base underneath only ever authorized the key its own *bake* was seeded with.
//! Nothing else re-renders `{{SSH_PUBLIC_KEY}}` into a child layer, so without a seed
//! carrying the new public key into the overlay there is no account in the guest that the
//! manifest's private key opens — and `QemuVm`'s `BatchMode=yes` + `IdentitiesOnly=yes`
//! leave nothing else to try.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::vm_login_user_data;
use tddy_vm::library::VmLibrary;
use tddy_vm::vm::{VmAccel, VmArch};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
use tempfile::tempdir;

const LOGIN_USERNAME: &str = "tddy";

/// A manifest for a VM built off the `debian-12` prepared base.
fn a_vm_manifest(name: &str) -> VmManifest {
    VmManifest {
        name: name.to_string(),
        prepared_base: Some("debian-12".to_string()),
        image_path: None,
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: "64M".to_string(),
            ssh_host_port: 2222,
            port_forwards: vec![],
            arch: VmArch::X86_64,
            accel: VmAccel::Tcg,
        },
        login: LoginPolicy {
            username: LOGIN_USERNAME.to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    }
}

/// Place a real (tiny, empty) qcow2 in `images/02-prepared-base/` for an overlay to be
/// chained onto — `qemu-img create -b` refuses a backing file it cannot open.
fn a_prepared_base_in(library: &VmLibrary, name: &str) {
    let output = std::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(library.prepared_base_dir().join(format!("{name}.qcow2")))
        .arg("1M")
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "qemu-img create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The `ssh_authorized_keys` the seed grants, per account, read back off the seed's own
/// `user-data` document rather than out of the ISO it is packed into.
///
/// Read as YAML rather than back through [`CloudInitUserData`], because the rendered list is
/// the mixed one cloud-init takes: the bare string `default` for the distro's own account —
/// reported here as an account granted nothing — and a map per account the document defines.
fn authorized_accounts_in(seed_dir: &Path) -> Vec<(String, Vec<String>)> {
    let yaml = std::fs::read_to_string(seed_dir.join("user-data"))
        .expect("the seed's user-data must be on disk");
    let document: serde_yml::Value =
        serde_yml::from_str(&yaml).expect("the seed's user-data must be a cloud-config document");
    document["users"]
        .as_sequence()
        .expect("the seed's user-data must carry a users list")
        .iter()
        .map(|entry| {
            let Some(distro_default) = entry.as_str() else {
                return (
                    entry["name"]
                        .as_str()
                        .expect("every defined account must have a name")
                        .to_string(),
                    entry["ssh_authorized_keys"]
                        .as_sequence()
                        .map(|keys| {
                            keys.iter()
                                .map(|key| key.as_str().unwrap_or_default().to_string())
                                .collect()
                        })
                        .unwrap_or_default(),
                );
            };
            (distro_default.to_string(), vec![])
        })
        .collect()
}

/// The public key `create_vm` generated for `name`, as `authorized_keys` spells it.
fn generated_public_key(library: &VmLibrary, name: &str) -> String {
    std::fs::read_to_string(library.vm_dir(name).join(format!("id_{name}.pub")))
        .expect("create_vm must have generated a public key")
        .trim()
        .to_string()
}

#[tokio::test]
async fn seeds_a_new_vm_with_the_public_key_of_the_pair_it_generated_for_that_vm() {
    // Given a library holding a prepared base
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    a_prepared_base_in(&library, "debian-12");

    // When a VM is created off it
    library.create_vm(&a_vm_manifest("web")).await.unwrap();

    // Then the seed authorizes that VM's own generated key for the account the manifest
    // logs in as, and no other — the distro's own account is kept (cloud-final resolves it
    // by name) but is granted nothing
    assert_eq!(
        authorized_accounts_in(&library.vm_seed_dir("web")),
        vec![
            ("default".to_string(), vec![]),
            (
                LOGIN_USERNAME.to_string(),
                vec![generated_public_key(&library, "web")]
            )
        ]
    );
}

#[tokio::test]
async fn names_a_fresh_instance_so_cloud_init_re_runs_its_ssh_module_on_the_new_overlay() {
    // Given a library holding a prepared base
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    a_prepared_base_in(&library, "debian-12");

    // When a VM is created off it
    library.create_vm(&a_vm_manifest("web")).await.unwrap();

    // Then the seed's meta-data names this VM as the instance — distinct from the
    // `cloud-init-<layer>` the prepared base was baked under, which is what makes
    // cloud-init treat the boot as a new instance and apply the key at all
    let meta_data = std::fs::read_to_string(library.vm_seed_dir("web").join("meta-data")).unwrap();
    assert_eq!(meta_data, "instance-id: tddy-vm-web\nlocal-hostname: web\n");
}

#[tokio::test]
async fn packs_the_seed_into_a_cidata_iso_beside_the_vms_own_overlay() {
    // Given a library holding a prepared base
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path());
    library.init().unwrap();
    a_prepared_base_in(&library, "debian-12");

    // When a VM is created off it
    library.create_vm(&a_vm_manifest("web")).await.unwrap();

    // Then the ISO the boot attaches as a cdrom is on disk, in the VM's own directory
    assert!(
        library.vm_seed_iso_path("web").is_file(),
        "expected a packed seed ISO at {}",
        library.vm_dir("web").join("web-seed.iso").display()
    );
}

#[test]
fn leaves_the_login_accounts_password_usable_so_the_serial_console_still_works() {
    // Given the seed a per-VM boot attaches to authorize its own key
    let user_data = vm_login_user_data("tddy");

    // When the login account it declares is read
    let login = user_data
        .users
        .first()
        .expect("the seed must declare the login account");

    // Then the password is explicitly left unlocked. Omitting this lets cloud-init apply
    // its own default of `lock_passwd: true`, which locks the password the prepared base
    // set — and the serial console, the only way into a guest whose network or sshd has
    // not come up, then answers every attempt with `Login incorrect`
    assert_eq!(login.lock_passwd, Some(false));
}
