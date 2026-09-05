//! Real-boot acceptance test for the Nix layer: a cloud-init bake that installs Nix, and a
//! VM off that layer running a command that only exists because it did.
//!
//! ## Why this exists next to `cloud_init_acceptance`
//!
//! [`cloud_init_acceptance`] proves the *mechanism* — a bake produces a bootable, loginable
//! layer — with a provisioning document that creates one account and nothing else. This
//! proves the mechanism carries **work** across the seal: `tddy_vm_testkit::recipes`'
//! [`nix_base_user_data`] is the real document every testkit image chains from, and what it
//! installs is a package manager whose binaries live in a store the bake had to create,
//! populate and leave behind in the overlay.
//!
//! The distinction matters because the failure it catches is invisible to the smaller test.
//! A bake whose `runcmd` silently failed still boots and still lets you log in; cloud-init
//! concatenates those commands into one script with no error handling of its own, which is
//! why the recipe opens with `set -e`. Asking the finished guest to *run Nix* is what
//! separates "cloud-init ran" from "cloud-init did what it was told".
//!
//! ## Production test — manual trigger only
//!
//! `#[ignore]`d (excluded from `./test`, `./verify` and plain `cargo test`) and gated on
//! `TDDY_CLOUDINIT_BASE_IMAGE` naming a cloud image whose architecture matches this host.
//! A missing image fails the test rather than skipping it — see `require_base_image`.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-<arch>.qcow2 \
//!   cargo test -p tddy-vm --test nix_layer_acceptance -- --ignored --nocapture
//! ```

mod common;

use std::time::Duration;

use common::require_base_image;
use serial_test::serial;
use tddy_vm::cloud_init::{build_cloud_init_image, CloudInitBuildOptions, IsoTool};
use tddy_vm::qemu::uefi_firmware_for;
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
use tddy_vm::{VmAccel, VmArch, VmLibrary};
use tddy_vm_testkit::recipes::{nix_base_user_data, SOURCE_NIX_PROFILE, TDDY_SERVICE_USERNAME};
use tddy_vm_testkit::BootedGuest;
use tempfile::tempdir;

/// The layer this bakes, which is both its filename under `images/02-prepared-base/` and
/// the `prepared_base` its VM manifest chains onto.
const LAYER_NAME: &str = "nix-base";

/// Virtual size of the overlays. Big enough for a Nix store, and sparse — the bake writes
/// what it installs, not this figure.
const DISK_SIZE: &str = "20G";

/// How long the bake gets to install Nix and halt the guest.
///
/// The install is a ~50 MB download and an unpack, a couple of minutes on a warm mirror.
/// The testkit gives its own copy of this bake an hour because it runs on whatever
/// connection a developer has; 20 minutes is generous for CI and still fails inside a
/// job's budget rather than at it.
const BAKE_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// Boot budget for the VM created from the baked layer — the same order as the other
/// real-boot suites, with room for cloud-init to apply the VM's own login seed.
const VM_BOOT_TIMEOUT: Duration = Duration::from_secs(300);

#[tokio::test]
#[ignore = "production test: bakes a Nix install into an overlay and boots a VM off it, \
            ~5-10 min; requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with \
            --ignored"]
#[serial(tddy_vm_real_boot)]
async fn a_vm_off_the_baked_nix_layer_runs_a_command_from_the_nix_store() {
    let base_image_src = require_base_image();

    // Given the Nix layer baked from the supplied cloud image, using the same provisioning
    // document the testkit's own image chain starts from rather than a copy of it
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path().join("library"));
    library.init().expect("the library tree must be creatable");
    let bake_dir = dir.path().join("bake");
    let opts = CloudInitBuildOptions {
        name: LAYER_NAME.to_string(),
        base_image_src,
        overlay_output: library
            .prepared_base_dir()
            .join(format!("{LAYER_NAME}.qcow2")),
        output_dir: bake_dir.clone(),
        user_data: nix_base_user_data(),
        disk_size: DISK_SIZE.to_string(),
        memory: "2048M".to_string(),
        cpus: 2,
        ssh_host_port: 2294,
        timeout: BAKE_TIMEOUT,
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
        firmware: uefi_firmware_for(VmArch::host(), &bake_dir, LAYER_NAME)
            .expect("UEFI firmware must be resolvable for the bake"),
        nine_p_shares: vec![],
    };
    build_cloud_init_image(&opts, &|line| eprintln!("{line}"))
        .await
        .expect("the Nix layer must bake");

    let requested = a_vm_manifest("nix-vm", LAYER_NAME, 2293);
    library
        .create_vm(&requested)
        .await
        .expect("a VM must be creatable from the baked Nix layer");
    let manifest = library
        .read_manifest(&requested.name)
        .expect("create_vm must have persisted a manifest naming the per-VM key");
    let guest = BootedGuest::boot(&library, &manifest, vec![])
        .await
        .expect("a VM created from the baked Nix layer must boot");
    guest
        .wait_for_ssh_ready(VM_BOOT_TIMEOUT)
        .await
        .expect("sshd must start answering in the guest");

    // When the host logs in over SSH and asks the guest to run Nix.
    //
    // The profile has to be sourced explicitly: a non-interactive `ssh host cmd` runs a
    // non-login shell, so nothing the installer left in /etc/profile.d is on PATH — the
    // same reason the recipes export `SOURCE_NIX_PROFILE`.
    let whoami = guest
        .run_over_ssh("id -un")
        .await
        .expect("ssh must authenticate with the per-VM key and run the command");
    let nix_path = guest
        .run_over_ssh(&format!("{SOURCE_NIX_PROFILE} && command -v nix"))
        .await
        .expect("the guest must answer over SSH");
    let nix_version = guest
        .run_over_ssh(&format!("{SOURCE_NIX_PROFILE} && nix --version"))
        .await
        .expect("the guest must answer over SSH");

    // Then SSH authenticated as the account the recipe provisions
    whoami.assert_stdout_line(TDDY_SERVICE_USERNAME);

    // And the `nix` that answered is the one the bake installed, out of the store it
    // created — not a distro package that happened to be on PATH
    assert!(
        nix_path.stdout().trim().starts_with("/nix/"),
        "`nix` must resolve into the Nix store the bake created, got {:?}",
        nix_path.stdout()
    );

    // And it runs: a binary that reports its own version has executed, which a store left
    // half-populated by a failed `runcmd` could not do
    assert!(
        nix_version.stdout().starts_with("nix (Nix)"),
        "`nix --version` must come from Nix itself, got {:?}",
        nix_version.stdout()
    );

    guest.shutdown().await.expect("guest must shut down");
}

/// The manifest of a throwaway VM chained onto `prepared_base`, logging in as the account
/// [`nix_base_user_data`] provisions.
fn a_vm_manifest(name: &str, prepared_base: &str, ssh_host_port: u16) -> VmManifest {
    VmManifest {
        name: name.to_string(),
        prepared_base: Some(prepared_base.to_string()),
        image_path: None,
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: DISK_SIZE.to_string(),
            ssh_host_port,
            port_forwards: vec![],
            arch: VmArch::host(),
            accel: VmAccel::host_default(),
        },
        login: LoginPolicy {
            username: TDDY_SERVICE_USERNAME.to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    }
}
