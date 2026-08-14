//! Cloud-init image build production test — boots a real `qemu-system-x86_64` VM
//! against a developer-supplied base cloud image.
//!
//! ## Real QEMU boot (production test — manual trigger only)
//!
//! Chains a delta overlay onto a real cloud image, bakes a NoCloud cloud-init seed into the
//! overlay by actually booting `qemu-system-x86_64` and watching the serial console for a
//! completion token, then asserts the guest shut itself down and the overlay is a valid
//! qcow2 that still names the image it was baked from.
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

    // Given a scratch directory, a place for the finished layer, and a minimal
    // provisioning spec
    let dir = tempdir().unwrap();
    let overlay_output = a_layer_directory_in(dir.path()).join("cloud-init-demo.qcow2");
    let opts = CloudInitBuildOptions {
        name: "cloud-init-demo".to_string(),
        base_image_src: base_image_src.clone(),
        overlay_output: overlay_output.clone(),
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

    // Then it succeeds and returns a provisioned overlay at the path it was told to build,
    // a real qcow2
    let overlay_path = result.expect("cloud-init image build must succeed");
    assert_eq!(overlay_path, overlay_output);
    let magic = std::fs::read(&overlay_path).expect("overlay must be readable");
    assert_eq!(&magic[..4], b"QFI\xfb", "overlay must be a qcow2 image");

    // And the layer holds its own delta rather than a copy of what it derives from. The
    // exact byte count of a baked delta depends on what cloud-init installed, so this
    // compares against the one thing that is certain: a copy could not be smaller
    assert!(
        size_on_disk(&overlay_path) < size_on_disk(&base_image_src),
        "a delta must be smaller than the image it chains onto: {} vs {}",
        size_on_disk(&overlay_path),
        size_on_disk(&base_image_src)
    );
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
async fn the_overlay_records_the_image_it_was_baked_from_as_a_relative_backing_file() {
    let Some(base_image_src) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given a completed cloud-init build
    let dir = tempdir().unwrap();
    let overlay_output = a_layer_directory_in(dir.path()).join("cloud-init-backing.qcow2");
    let opts = CloudInitBuildOptions {
        name: "cloud-init-backing".to_string(),
        base_image_src: base_image_src.clone(),
        overlay_output,
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
    let backing = backing_file_of(&overlay_path);

    // Then the reference is relative, so the layer travels with its ancestors when the
    // library moves
    assert!(
        !PathBuf::from(&backing).is_absolute(),
        "the backing reference must be relative, got: {backing}"
    );

    // And it resolves — against the overlay's own directory, which is how qcow2 reads it —
    // back to the exact image the bake chained onto
    let overlay_dir = overlay_path
        .parent()
        .expect("the overlay must live in a directory");
    assert_eq!(
        overlay_dir
            .join(&backing)
            .canonicalize()
            .expect("the backing reference must resolve to a real file"),
        base_image_src
            .canonicalize()
            .expect("the base image must still be readable")
    );
}

/// A library-shaped `images/02-prepared-base/` directory under `root`, so a layer is built
/// where a layer belongs — and, more importantly, is built where it will stay.
fn a_layer_directory_in(root: &std::path::Path) -> PathBuf {
    let dir = root.join("images/02-prepared-base");
    std::fs::create_dir_all(&dir).expect("the layer directory must be creatable");
    dir
}

/// Bytes `image` occupies, which for a qcow2 delta is its own content only.
fn size_on_disk(image: &std::path::Path) -> u64 {
    std::fs::metadata(image)
        .expect("the image must exist")
        .len()
}

/// The backing reference `qemu-img info` reports as recorded in `image` — without the
/// ` (actual path: …)` annotation qemu-img adds for a relative one, which describes where
/// the file sits today rather than what the image records.
fn backing_file_of(image: &std::path::Path) -> String {
    let output = std::process::Command::new("qemu-img")
        .arg("info")
        .arg(image)
        .output()
        .expect("qemu-img info must run");
    let info = String::from_utf8_lossy(&output.stdout);
    let reported = info
        .lines()
        .find_map(|line| line.strip_prefix("backing file: "))
        .unwrap_or_else(|| panic!("qemu-img info reported no backing file:\n{info}"));
    reported
        .split(" (actual path:")
        .next()
        .unwrap_or(reported)
        .trim()
        .to_string()
}
