//! Production test for the `tddy-vm-build cloud-init` subcommand: copies an immutable
//! base cloud image and produces a cloud-init-provisioned, chained qcow2 delta overlay
//! — the "ready to use" image this feature exists to build.
//!
//! ## Real QEMU boot (production test — manual trigger only)
//!
//! Boots a real `qemu-system-x86_64` VM to bake cloud-init into the overlay (see
//! `packages/tddy-vm/tests/cloud_init_acceptance.rs` for the underlying pipeline this
//! CLI wraps).
//!
//! This is a production test: it never runs on its own. `#[ignore]`d (excluded from
//! `./test`/`./verify`/plain `cargo test`) *and* gated on `TDDY_CLOUDINIT_BASE_IMAGE`
//! pointing at a real cloud-init-compatible qcow2 image (e.g. a Debian genericcloud
//! image) — the same config the CLI's own `--base-image` flag reads. There is no
//! bundled or auto-downloaded image; a developer must supply one explicitly to run
//! this test.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/base.qcow2 \
//!   cargo test -p tddy-vm-build --test cloud_init_cli_acceptance -- --ignored --nocapture
//! ```

use assert_cmd::cargo::cargo_bin_cmd;
use serial_test::serial;
use std::path::PathBuf;
use tempfile::tempdir;

fn tddy_vm_build_bin() -> assert_cmd::Command {
    cargo_bin_cmd!("tddy-vm-build")
}

/// The env var this production test reads its base image path from — the same config
/// knob the CLI's own `--base-image` flag reads.
const BASE_IMAGE_ENV: &str = "TDDY_CLOUDINIT_BASE_IMAGE";

/// Resolve the base image path from `TDDY_CLOUDINIT_BASE_IMAGE`, or `None` if unset.
fn configured_base_image() -> Option<PathBuf> {
    std::env::var(BASE_IMAGE_ENV).ok().map(PathBuf::from)
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
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_copies_the_configured_base_and_produces_a_chained_overlay_image() {
    let Some(base_image) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given an output directory and a minimal cloud-init user-data file
    let dir = tempdir().unwrap();
    let user_data_path = dir.path().join("user-data.yaml");
    std::fs::write(&user_data_path, a_minimal_user_data_yaml()).unwrap();

    // When running the cloud-init subcommand against the configured base image
    let mut cmd = tddy_vm_build_bin();
    cmd.arg("cloud-init")
        .arg("--name")
        .arg("cli-demo")
        .arg("--base-image")
        .arg(base_image)
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
