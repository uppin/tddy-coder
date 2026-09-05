//! Real-boot acceptance tests for what a bake actually leaves in the image it seals.
//!
//! Every other test of the bake asserts on the *document* it renders. Three bugs shipped
//! behind those green tests at once — a `bootcmd` that deleted the `runcmd` script cloud-init
//! was about to run, a `users:` list that removed the account `cc_ssh_authkey_fingerprints`
//! looks up, and a reboot the next boot never resumed from — because a rendered document
//! that reads correctly and a guest that provisioned correctly are two different claims.
//!
//! These tests make the second claim the one under assertion: bake a tiny recipe for real,
//! then boot the layer it produced and ask the guest, over SSH, what is in it.
//!
//! ## Production tests — manual trigger only
//!
//! `#[ignore]`d (excluded from `./test`, `./verify`, and plain `cargo test`) *and* gated on
//! `TDDY_CLOUDINIT_BASE_IMAGE` pointing at a real Debian cloud image already on disk.
//! Nothing is downloaded.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-<arch>.qcow2 \
//!   cargo test -p tddy-vm --test bake_provisioning_acceptance -- --ignored --nocapture
//! ```

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{a_console_loginable_user_data, a_test_guest, require_base_image};
use serial_test::serial;
use tddy_vm::cloud_init::{
    build_cloud_init_image, reset_cloud_init_and_reboot, CloudInitBuildOptions, CloudInitUserData,
    IsoTool,
};
use tddy_vm::qemu::{ensure_uefi_vars_file, resolve_uefi_code_path};
use tddy_vm::{UefiFirmware, VmAccel, VmArch};
use tddy_vm_testkit::guest::{BootedGuest, SSH_READY_TIMEOUT};
use tempfile::tempdir;

/// Budget for baking a recipe that installs no packages: one boot of a cloud image, a
/// handful of shell commands, a power-off. Measured at ~2 minutes on an accelerated host;
/// the ceiling absorbs a cold host page cache and a guest that reboots itself once.
const BAKE_TIMEOUT: Duration = Duration::from_secs(900);

/// The virtual disk size every image in these tests is created with. One value, because a
/// layer cannot be smaller than the image it chains onto — the verification guest's own
/// overlay is created from `an_acceptance_run_policy`'s size, so the bake must match it.
const DISK_SIZE: &str = "8G";

/// The file a baked recipe writes to prove its `runcmd` ran. Under `/var/lib` rather than
/// `/tmp`, which a reboot would empty — a marker that does not survive one proves nothing
/// about a bake that reboots.
const MARKER_PATH: &str = "/var/lib/tddy-bake-marker";

/// The file the first pass of a rebooting recipe writes, so the second pass can tell it has
/// already run and skip straight to the work that follows the reboot.
const REBOOT_STAMP_PATH: &str = "/var/lib/tddy-bake-rebooted";

/// The account a Debian cloud image creates by default, and the one
/// `cc_ssh_authkey_fingerprints` resolves by name in `cloud-final`. The base image these
/// tests are pointed at is a Debian cloud image (see the module docs).
const DISTRO_DEFAULT_ACCOUNT: &str = "debian";

#[tokio::test]
#[ignore = "production test: bakes a real image and boots it, ~4 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn bakes_the_recipes_runcmd_into_the_image_it_seals() {
    let base_image = require_base_image();

    // Given a recipe whose provisioning is one command: write a marker file
    let dir = tempdir().unwrap();
    let recipe = CloudInitUserData {
        runcmd: vec![format!("echo provisioned > {MARKER_PATH}")],
        ..a_console_loginable_user_data("bake-marker")
    };

    // When it is baked into a layer, and that layer is booted
    let layer = a_layer_baked_from(&base_image, dir.path(), "bake-marker", recipe, 2271).await;
    let guest = a_guest_booted_from(&layer, dir.path(), "bake-marker-check", 2272).await;

    // Then the marker the recipe wrote is in the image — a bake that seals an unprovisioned
    // image is the failure this test exists to catch
    guest
        .run_over_ssh(&format!("cat {MARKER_PATH}"))
        .await
        .expect("the baked marker file must be readable in the guest")
        .assert_stdout_line("provisioned");

    guest.shutdown().await.expect("the guest must shut down");
}

#[tokio::test]
#[ignore = "production test: bakes a real image across a mid-bake reboot and boots it, \
            ~6 min; requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn resumes_a_recipe_whose_first_pass_rebooted_the_guest() {
    let base_image = require_base_image();

    // Given a recipe that reboots the guest on its first pass — as the kernel-swap step of a
    // real recipe does — and only writes its marker on the pass after that
    let dir = tempdir().unwrap();
    let recipe = CloudInitUserData {
        runcmd: vec![
            "set -e".to_string(),
            format!(
                "if [ ! -f {REBOOT_STAMP_PATH} ]; then touch {REBOOT_STAMP_PATH}; {}; exit 0; fi",
                reset_cloud_init_and_reboot()
            ),
            format!("echo provisioned-after-reboot > {MARKER_PATH}"),
        ],
        ..a_console_loginable_user_data("bake-reboot")
    };

    // When it is baked into a layer, and that layer is booted
    let layer = a_layer_baked_from(&base_image, dir.path(), "bake-reboot", recipe, 2273).await;
    let guest = a_guest_booted_from(&layer, dir.path(), "bake-reboot-check", 2274).await;

    // Then the step that only ever runs after the reboot did run: cloud-init picked the
    // recipe back up on the next boot instead of recognising an instance it had already
    // provisioned and skipping runcmd entirely
    guest
        .run_over_ssh(&format!("cat {MARKER_PATH}"))
        .await
        .expect("the marker written after the reboot must be readable in the guest")
        .assert_stdout_line("provisioned-after-reboot");

    guest.shutdown().await.expect("the guest must shut down");
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM, ~2 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(tddy_vm_real_boot)]
async fn keeps_the_images_own_default_account_alongside_the_ones_the_recipe_defines() {
    let base_image = require_base_image();

    // Given a guest seeded with a recipe that defines an account of its own
    let dir = tempdir().unwrap();
    let guest = a_guest_booted_from(&base_image, dir.path(), "distro-default", 2275).await;

    // When the guest is asked for the account the image's own distro creates by default
    let default_account = guest
        .run_over_ssh(&format!(
            "getent passwd {DISTRO_DEFAULT_ACCOUNT} | cut -d: -f1"
        ))
        .await
        .expect("the guest must answer over SSH");

    // Then it is still there. A `users:` list replaces the distro default rather than adding
    // to it, and `cc_ssh_authkey_fingerprints` resolves that account by name — on a guest
    // where it was never created the module dies with `KeyError: "getpwnam(): name not
    // found: 'debian'"`, failing cloud-final with exit 1 however well provisioning went
    default_account.assert_stdout_line(DISTRO_DEFAULT_ACCOUNT);

    guest.shutdown().await.expect("the guest must shut down");
}

/// Bake `recipe` into a layer chained onto `base_image`, and return the sealed layer.
async fn a_layer_baked_from(
    base_image: &Path,
    work_dir: &Path,
    name: &str,
    recipe: CloudInitUserData,
    ssh_host_port: u16,
) -> PathBuf {
    let layer_dir = work_dir.join("images/02-prepared-base");
    std::fs::create_dir_all(&layer_dir).expect("the layer directory must be creatable");
    let arch = VmArch::host();

    let opts = CloudInitBuildOptions {
        name: name.to_string(),
        base_image_src: base_image.to_path_buf(),
        overlay_output: layer_dir.join(format!("{name}.qcow2")),
        output_dir: work_dir.to_path_buf(),
        user_data: recipe,
        disk_size: DISK_SIZE.to_string(),
        memory: "2048M".to_string(),
        cpus: 2,
        ssh_host_port,
        timeout: BAKE_TIMEOUT,
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch,
        accel: VmAccel::host_default(),
        firmware: a_host_firmware(work_dir, name),
        nine_p_shares: vec![],
    };

    build_cloud_init_image(&opts, &|line| eprintln!("{line}"))
        .await
        .expect("the bake must succeed")
}

/// Boot a guest off `image` — a freshly baked layer, or the base image itself — with an SSH
/// key this process holds, and wait until sshd answers.
async fn a_guest_booted_from(
    image: &Path,
    work_dir: &Path,
    name: &str,
    ssh_host_port: u16,
) -> BootedGuest {
    let keys = tddy_vm::library::generate_vm_ssh_keypair(work_dir, name)
        .expect("a per-guest keypair must be generatable");
    let mut user_data = a_console_loginable_user_data(name);
    user_data.users[0].ssh_authorized_keys = vec![std::fs::read_to_string(&keys.public_key_path)
        .expect("the generated public key must be readable")];

    let guest = a_test_guest(image, work_dir, name)
        .with_ssh_host_port(ssh_host_port)
        .with_user_data(user_data)
        .with_ssh_key_login(&keys.private_key_path, &keys.public_key_path)
        .boot()
        .await;

    // Slirp accepts the forwarded connection from the moment QEMU starts, so waiting for
    // sshd to answer is a separate step from asking it anything.
    guest
        .wait_for_ssh_ready(SSH_READY_TIMEOUT)
        .await
        .expect("sshd must start answering in the guest");
    guest
}

/// The UEFI firmware pair a bake boots through on this host, with its writable variables
/// store in `work_dir`. `None` on x86_64, whose `q35` machine boots through SeaBIOS.
fn a_host_firmware(work_dir: &Path, name: &str) -> Option<UefiFirmware> {
    let arch = VmArch::host();
    if arch == VmArch::X86_64 {
        return None;
    }
    let vars = work_dir.join(format!("{name}-bake-vars.fd"));
    ensure_uefi_vars_file(&vars).expect("UEFI vars file must be creatable");
    Some(UefiFirmware {
        code_path: resolve_uefi_code_path(arch)
            .expect("UEFI firmware must be resolvable")
            .display()
            .to_string(),
        vars_path: vars.display().to_string(),
    })
}
