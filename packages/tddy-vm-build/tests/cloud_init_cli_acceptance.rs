//! Acceptance test for the `tddy-vm-build cloud-init` subcommand: copies an immutable
//! base cloud image and produces a cloud-init-provisioned, chained qcow2 delta overlay
//! — the "ready to use" image this feature exists to build.
//!
//! ## Real QEMU boot (opt-in, slow)
//!
//! Boots a real `qemu-system-x86_64` VM to bake cloud-init into the overlay (see
//! `packages/tddy-vm/tests/cloud_init_acceptance.rs` for the underlying pipeline this
//! CLI wraps). `#[ignore]`d and excluded from `./test`/`./verify`/plain `cargo test` by
//! default; also skipped at runtime unless `qemu-img`, `qemu-system-x86_64`, `xorriso`,
//! and the makers-lt base-image cache are all present on the host.
//!
//! Run explicitly with:
//! ```text
//! cargo test -p tddy-vm-build --test cloud_init_cli_acceptance -- --ignored --nocapture
//! ```

use assert_cmd::cargo::cargo_bin_cmd;
use serial_test::serial;
use std::path::PathBuf;
use tempfile::tempdir;

fn tddy_vm_build_bin() -> assert_cmd::Command {
    cargo_bin_cmd!("tddy-vm-build")
}

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

fn a_minimal_user_data_yaml() -> &'static str {
    "hostname: cloud-init-cli-demo\n\
     users:\n  \
       - name: tddy\n    \
         shell: /bin/bash\n    \
         sudo: \"ALL=(ALL) NOPASSWD:ALL\"\n    \
         ssh_authorized_keys:\n      \
           - \"{{SSH_PUBLIC_KEY}}\"\n"
}

#[test]
#[ignore = "boots a real QEMU VM to bake cloud-init, ~1-3 min; run with --ignored (see module docs)"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_copies_the_makers_lt_base_and_produces_a_chained_overlay_image() {
    if !tooling_available() {
        eprintln!(
            "qemu-img/qemu-system-x86_64/xorriso/makers-lt base cache not available — \
             skipping cloud-init CLI acceptance test"
        );
        return;
    }

    // Given an output directory and a minimal cloud-init user-data file
    let dir = tempdir().unwrap();
    let user_data_path = dir.path().join("user-data.yaml");
    std::fs::write(&user_data_path, a_minimal_user_data_yaml()).unwrap();

    // When running the cloud-init subcommand against the makers-lt base
    let mut cmd = tddy_vm_build_bin();
    cmd.arg("cloud-init")
        .arg("--name")
        .arg("cli-demo")
        .arg("--base-image")
        .arg(makers_lt_base_image())
        .arg("--output-dir")
        .arg(dir.path())
        .arg("--user-data")
        .arg(&user_data_path)
        .arg("--disk-size")
        .arg("10G")
        .arg("--memory")
        .arg("1024M")
        .arg("--cpus")
        .arg("1")
        .arg("--ssh-host-port")
        .arg("2297")
        .arg("--timeout-secs")
        .arg("180");

    // Then the CLI succeeds and produces a delta overlay chained onto an immutable base
    cmd.assert().success();
    let overlay_path = dir.path().join("cli-demo.qcow2");
    let base_path = dir.path().join("cli-demo-base.qcow2");
    assert!(
        overlay_path.exists(),
        "provisioned overlay must exist at {}",
        overlay_path.display()
    );
    assert!(
        base_path.exists(),
        "immutable base must exist at {}",
        base_path.display()
    );
    let magic = std::fs::read(&overlay_path).expect("overlay must be readable");
    assert_eq!(&magic[..4], b"QFI\xfb", "overlay must be a qcow2 image");
}
