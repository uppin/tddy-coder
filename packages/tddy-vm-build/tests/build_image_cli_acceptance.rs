//! Acceptance tests for the `tddy-vm-build` CLI: build a VM image from a Buildroot spec
//! file and write it to an explicit output path, in either qcow2 or raw format.
//!
//! ## Real Buildroot build (opt-in, slow)
//!
//! Both tests here run a genuine Buildroot build (bootstrapping a host cross-toolchain
//! from source, then building a target rootfs) — not a fixture or a fake CLI. That can
//! take 45–90+ minutes depending on the host/Docker VM's resources (see
//! `docs/dev/1-WIP/qemu-sandbox-cli.md` for why macOS routes through Docker at all), so
//! they are `#[ignore]`d and excluded from `./test`/`./verify`/plain `cargo test` by
//! default. `#[serial]` keeps the two tests from running concurrently against the same
//! Docker VM, which can otherwise oversubscribe its (often modest) CPU/RAM allocation.
//!
//! Run explicitly with:
//! ```text
//! cargo test -p tddy-vm-build --test build_image_cli_acceptance -- --ignored --nocapture
//! ```
//! Requires `BUILDROOT_DIR` set (provided by the nix dev shell) and, on macOS, Docker
//! installed and running.

use assert_cmd::cargo::cargo_bin_cmd;
use serial_test::serial;
use std::fs;
use tempfile::tempdir;

fn tddy_vm_build_bin() -> assert_cmd::Command {
    cargo_bin_cmd!("tddy-vm-build")
}

/// A real, minimal Buildroot spec that reliably produces `images/rootfs.ext2` — `make
/// olddefconfig` silently drops `BR2_TARGET_ROOTFS_EXT2` back to just the always-built
/// `rootfs.tar` unless an explicit size is given, so `_SIZE` is required, not decorative.
fn a_minimal_buildroot_spec() -> &'static str {
    "BR2_x86_64=y\nBR2_TOOLCHAIN_BUILDROOT_GLIBC=y\nBR2_TARGET_ROOTFS_EXT2=y\nBR2_TARGET_ROOTFS_EXT2_SIZE=\"60M\"\n"
}

#[test]
#[ignore = "real Buildroot build, 45-90+ min; run with --ignored (see module docs)"]
#[serial(buildroot_docker_vm)]
fn builds_a_qcow2_image_from_a_buildroot_spec_file() {
    // Given a spec file and an output path that does not yet exist
    let dir = tempdir().unwrap();
    let spec_path = dir.path().join("spec.config");
    fs::write(&spec_path, a_minimal_buildroot_spec()).unwrap();
    let output_path = dir.path().join("image.qcow2");

    // When building the image as qcow2
    let mut cmd = tddy_vm_build_bin();
    cmd.arg("--spec")
        .arg(&spec_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--format")
        .arg("qcow2");

    // Then the CLI succeeds and produces a qcow2 file at the requested path
    cmd.assert().success();
    let magic = fs::read(&output_path).expect("output image must exist and be readable");
    assert_eq!(&magic[..4], b"QFI\xfb", "output must be a qcow2 image");
}

#[test]
#[ignore = "real Buildroot build, 45-90+ min; run with --ignored (see module docs)"]
#[serial(buildroot_docker_vm)]
fn writes_a_raw_image_when_the_format_flag_requests_raw() {
    // Given a spec file and an output path that does not yet exist
    let dir = tempdir().unwrap();
    let spec_path = dir.path().join("spec.config");
    fs::write(&spec_path, a_minimal_buildroot_spec()).unwrap();
    let output_path = dir.path().join("image.raw");

    // When building the image as raw
    let mut cmd = tddy_vm_build_bin();
    cmd.arg("--spec")
        .arg(&spec_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--format")
        .arg("raw");

    // Then the CLI succeeds and the raw image exists at the requested path
    cmd.assert().success();
    assert!(
        output_path.exists(),
        "raw output image must exist at {}",
        output_path.display()
    );
}
