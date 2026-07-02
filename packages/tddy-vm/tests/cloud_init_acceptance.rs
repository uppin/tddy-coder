//! Cloud-init image build integration test — real qemu-img/qemu-system-x86_64/xorriso
//! boot, gated on tool + base-image-cache availability (mirrors the skip-guard
//! convention in `build_image_acceptance.rs`).
//!
//! ## Real QEMU boot (opt-in, slow)
//!
//! This test copies the makers-lt Debian 12 cloud image cache, chains an immutable
//! base + delta overlay, bakes a NoCloud cloud-init seed into the overlay by actually
//! booting `qemu-system-x86_64` and watching the serial console for a completion
//! token, then asserts the guest shut itself down and the overlay is a valid,
//! base-backed qcow2. `#[ignore]`d and excluded from `./test`/`./verify`/plain
//! `cargo test` by default because it boots a real VM (~1-3 min).
//!
//! Run explicitly with:
//! ```text
//! cargo test -p tddy-vm --test cloud_init_acceptance -- --ignored --nocapture
//! ```

use serial_test::serial;
use std::path::PathBuf;
use std::time::Duration;
use tddy_vm::cloud_init::{
    build_cloud_init_image, CloudInitBuildOptions, CloudInitUser, CloudInitUserData, IsoTool,
};
use tempfile::tempdir;

/// The makers-lt cache path this feature copies its immutable base from (never
/// downloaded, never mutated by this feature). Not part of this repo — the test skips
/// itself if the host doesn't have a `~/Code/makers-lt` checkout with this image
/// already cached.
fn makers_lt_base_image() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME must be set")).join(
        "Code/makers-lt/packages/agentic-drone/.maker-build-cache/external-sources/\
         7191be88beba48b2/debian-12-genericcloud-amd64.qcow2",
    )
}

fn binary_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn tooling_available() -> bool {
    binary_available("qemu-img")
        && binary_available("qemu-system-x86_64")
        && binary_available("xorriso")
        && makers_lt_base_image().exists()
}

fn a_minimal_cloud_init_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("cloud-init-acceptance".to_string()),
        users: vec![CloudInitUser {
            name: "tddy".to_string(),
            shell: Some("/bin/bash".to_string()),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
        }],
        packages: vec![],
        runcmd: vec![],
        write_files: vec![],
        bootcmd: vec![],
    }
}

#[tokio::test]
#[ignore = "boots a real QEMU VM to bake cloud-init, ~1-3 min; run with --ignored (see module docs)"]
#[serial(cloud_init_qemu_vm)]
async fn builds_a_ready_to_use_provisioned_qcow2_by_baking_cloud_init_into_an_overlay() {
    if !tooling_available() {
        eprintln!(
            "qemu-img/qemu-system-x86_64/xorriso/makers-lt base cache not available — \
             skipping cloud-init build test"
        );
        return;
    }

    // Given an output directory and a minimal provisioning spec
    let dir = tempdir().unwrap();
    let opts = CloudInitBuildOptions {
        name: "cloud-init-demo".to_string(),
        base_image_src: makers_lt_base_image(),
        output_dir: dir.path().to_path_buf(),
        user_data: a_minimal_cloud_init_user_data(),
        disk_size: "10G".to_string(),
        memory: "1024M".to_string(),
        cpus: 1,
        ssh_host_port: 2299,
        timeout: Duration::from_secs(180),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
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
#[ignore = "boots a real QEMU VM to bake cloud-init, ~1-3 min; run with --ignored (see module docs)"]
#[serial(cloud_init_qemu_vm)]
async fn the_overlay_records_its_immutable_base_as_a_relative_backing_file() {
    if !tooling_available() {
        eprintln!(
            "qemu-img/qemu-system-x86_64/xorriso/makers-lt base cache not available — \
             skipping cloud-init build test"
        );
        return;
    }

    // Given a completed cloud-init build
    let dir = tempdir().unwrap();
    let opts = CloudInitBuildOptions {
        name: "cloud-init-backing".to_string(),
        base_image_src: makers_lt_base_image(),
        output_dir: dir.path().to_path_buf(),
        user_data: a_minimal_cloud_init_user_data(),
        disk_size: "10G".to_string(),
        memory: "1024M".to_string(),
        cpus: 1,
        ssh_host_port: 2298,
        timeout: Duration::from_secs(180),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
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
