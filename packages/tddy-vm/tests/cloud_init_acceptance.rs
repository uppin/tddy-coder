//! Cloud-init image build production test — boots a real `qemu-system-x86_64` VM
//! against a developer-supplied base cloud image.
//!
//! ## Real QEMU boot (production test — manual trigger only)
//!
//! Chains an immutable base + delta overlay from a real cloud image, bakes a NoCloud
//! cloud-init seed into the overlay by actually booting `qemu-system-x86_64` and
//! watching the serial console for a completion token, then asserts the guest shut
//! itself down and the overlay is a valid, base-backed qcow2.
//!
//! This is a production test: it never runs on its own. `#[ignore]`d (excluded from
//! `./test`/`./verify`/plain `cargo test`) *and* gated on `TDDY_CLOUDINIT_BASE_IMAGE`
//! — the same config the `tddy-vm-build cloud-init` CLI reads for `--base-image` —
//! pointing at a real cloud-init-compatible qcow2 image (e.g. a Debian genericcloud
//! image). There is no bundled or auto-downloaded image; a developer must supply one
//! explicitly to run this test.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/base.qcow2 \
//!   cargo test -p tddy-vm --test cloud_init_acceptance -- --ignored --nocapture
//! ```

use serial_test::serial;
use std::path::PathBuf;
use std::time::Duration;
use tddy_vm::cloud_init::{
    build_cloud_init_image, CloudInitBuildOptions, CloudInitUser, CloudInitUserData, IsoTool,
};
use tddy_vm::qemu::{ensure_uefi_vars_file, resolve_uefi_code_path};
use tddy_vm::{UefiFirmware, VmAccel, VmArch};
use tempfile::tempdir;

/// The env var this production test reads its base image path from — the same config
/// knob the `tddy-vm-build cloud-init` CLI's `--base-image` flag reads.
const BASE_IMAGE_ENV: &str = "TDDY_CLOUDINIT_BASE_IMAGE";

/// Resolve the base image path from `TDDY_CLOUDINIT_BASE_IMAGE`, or `None` if unset.
fn configured_base_image() -> Option<PathBuf> {
    std::env::var(BASE_IMAGE_ENV).ok().map(PathBuf::from)
}

/// The UEFI firmware pair the bake boots through on this host, with its writable variables
/// store placed in `work_dir`.
///
/// `None` on x86_64, whose `q35` machine boots the image's own bootloader through SeaBIOS;
/// the aarch64 `virt` machine has no BIOS at all, so firmware there is mandatory.
fn a_host_firmware(work_dir: &std::path::Path, name: &str) -> Option<UefiFirmware> {
    let arch = VmArch::host();
    if arch == VmArch::X86_64 {
        return None;
    }
    let vars = work_dir.join(format!("{name}-vars.fd"));
    ensure_uefi_vars_file(&vars).expect("UEFI vars file must be creatable");
    Some(UefiFirmware {
        code_path: resolve_uefi_code_path(arch)
            .expect("UEFI firmware must be resolvable")
            .display()
            .to_string(),
        vars_path: vars.display().to_string(),
    })
}

fn a_minimal_cloud_init_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("cloud-init-acceptance".to_string()),
        users: vec![CloudInitUser {
            name: "tddy".to_string(),
            shell: Some("/bin/bash".to_string()),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
            plain_text_passwd: None,
            lock_passwd: None,
        }],
        packages: vec![],
        runcmd: vec![],
        write_files: vec![],
        bootcmd: vec![],
    }
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
async fn builds_a_ready_to_use_provisioned_qcow2_by_baking_cloud_init_into_an_overlay() {
    let Some(base_image_src) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given an output directory and a minimal provisioning spec
    let dir = tempdir().unwrap();
    let opts = CloudInitBuildOptions {
        name: "cloud-init-demo".to_string(),
        base_image_src,
        output_dir: dir.path().to_path_buf(),
        user_data: a_minimal_cloud_init_user_data(),
        disk_size: "10G".to_string(),
        memory: "1024M".to_string(),
        cpus: 1,
        ssh_host_port: 2299,
        timeout: Duration::from_secs(180),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
        firmware: a_host_firmware(dir.path(), "cloud-init-demo"),
        nine_p_shares: vec![],
    };

    // When building the cloud-init image
    let result = build_cloud_init_image(&opts, &|line| eprintln!("{line}")).await;

    // Then it succeeds and returns the provisioned overlay, backed by an immutable base
    let overlay_path = result.expect("cloud-init image build must succeed");
    assert!(
        overlay_path.exists(),
        "overlay must exist at {}",
        overlay_path.display()
    );
    let magic = std::fs::read(&overlay_path).expect("overlay must be readable");
    assert_eq!(&magic[..4], b"QFI\xfb", "overlay must be a qcow2 image");

    let base_path = dir.path().join("cloud-init-demo-base.qcow2");
    assert!(
        base_path.exists(),
        "immutable base must exist at {}",
        base_path.display()
    );
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
async fn the_overlay_records_its_immutable_base_as_a_relative_backing_file() {
    let Some(base_image_src) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given a completed cloud-init build
    let dir = tempdir().unwrap();
    let opts = CloudInitBuildOptions {
        name: "cloud-init-backing".to_string(),
        base_image_src,
        output_dir: dir.path().to_path_buf(),
        user_data: a_minimal_cloud_init_user_data(),
        disk_size: "10G".to_string(),
        memory: "1024M".to_string(),
        cpus: 1,
        ssh_host_port: 2298,
        timeout: Duration::from_secs(180),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
        firmware: a_host_firmware(dir.path(), "cloud-init-backing"),
        nine_p_shares: vec![],
    };
    let overlay_path = build_cloud_init_image(&opts, &|line| eprintln!("{line}"))
        .await
        .expect("cloud-init image build must succeed");

    // When inspecting the overlay's backing file via qemu-img info
    let output = std::process::Command::new("qemu-img")
        .arg("info")
        .arg(&overlay_path)
        .output()
        .expect("qemu-img info must run");
    let info = String::from_utf8_lossy(&output.stdout);

    // Then the backing file is the co-located base's relative basename, not an
    // absolute path — the overlay and base must stay co-located to remain valid
    assert!(
        info.contains("backing file: cloud-init-backing-base.qcow2"),
        "qemu-img info must report the relative base filename as the backing file, got:\n{info}"
    );
}
