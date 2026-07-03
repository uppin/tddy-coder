//! Acceptance tests for the `tddy-sandbox-qemu` CLI: the two headline "sandbox builder"
//! capabilities requested for the VM backend — mounting a host directory into the guest
//! (read-write and read-only) and running a command against it.
//!
//! Booting a real, runner-capable guest image is out of scope for this changeset (see
//! docs/dev/1-WIP/qemu-sandbox-cli.md "Open design points"); these tests exercise the
//! CLI's contract end-to-end against a placeholder image and must fail until
//! `spawn_plan_with` actually boots the VM and enforces the mount.

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::tempdir;

fn sandbox_qemu_bin() -> assert_cmd::Command {
    cargo_bin_cmd!("tddy-sandbox-qemu")
}

/// Content is irrelevant to these tests — a real file is enough to give `--image` a path
/// that exists, since image *validity* is not what's under test here.
fn a_placeholder_image() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("image.qcow2"), b"not-a-real-image").unwrap();
    dir
}

#[test]
#[ignore = "blocked on spawn_plan_with's in-guest runner wiring (grpc_socket_path/ready_marker_path \
            are still placeholder paths nothing populates, per spawn.rs's own doc comment) — a real \
            qcow2 backing image would pass the qemu-img format check this currently fails on, but \
            then hang for the full 180s wait_for_ready_marker timeout in lib.rs::run_sandbox_qemu \
            waiting for an in-guest tddy-sandbox-runner that has no OS/wiring to run it; unignore \
            once guest boot + control-port wiring lands"]
fn mounts_a_host_directory_read_write_and_runs_a_command_in_the_guest_vm() {
    // Given a host directory with a marker file and a VM image to boot
    let host_dir = tempdir().unwrap();
    fs::write(host_dir.path().join("marker.txt"), "hello-from-host").unwrap();
    let image_dir = a_placeholder_image();
    let image_path = image_dir.path().join("image.qcow2");

    // When running the sandbox with a read-write mount and a command that reads the mount
    let mut cmd = sandbox_qemu_bin();
    cmd.arg("--image")
        .arg(&image_path)
        .arg("--mount")
        .arg(format!("{}:/work:rw", host_dir.path().display()))
        .arg("--")
        .arg("cat")
        .arg("/work/marker.txt");

    // Then the guest command's output is streamed back and the process exits cleanly
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("hello-from-host"));
}

#[test]
#[ignore = "blocked on spawn_plan_with's in-guest runner wiring — see the ignore reason on \
            mounts_a_host_directory_read_write_and_runs_a_command_in_the_guest_vm above"]
fn rejects_a_write_inside_a_mount_without_the_rw_flag() {
    // Given a host directory mounted without ":rw" (read-only by default)
    let host_dir = tempdir().unwrap();
    fs::write(host_dir.path().join("marker.txt"), "hello-from-host").unwrap();
    let image_dir = a_placeholder_image();
    let image_path = image_dir.path().join("image.qcow2");

    // When the guest command tries to write into that mount
    let mut cmd = sandbox_qemu_bin();
    cmd.arg("--image")
        .arg(&image_path)
        .arg("--mount")
        .arg(format!("{}:/work", host_dir.path().display()))
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("echo blocked > /work/marker.txt");

    // Then the guest denies the write and the failure is surfaced as a read-only error
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("Read-only"));
}
