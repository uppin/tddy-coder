//! Unit tests for the QEMU sandbox backend's pure argv/config builders.
//!
//! These tests fully specify the expected argv/JSON so the implementation can be
//! verified independently of actually spawning QEMU.

use std::path::PathBuf;
use tddy_sandbox::{MountSpec, NetworkSpec, SandboxBuilder};
use tddy_sandbox_qemu::argv::{
    guest_plan_json, ninep_fsdev_args, overlay_create_argv, qemu_sandbox_argv, PLAN_MOUNT_TAG,
    RUNNER_MOUNT_TAG,
};
use tddy_sandbox_qemu::spawn::QemuBackendOptions;

fn a_read_write_mount() -> MountSpec {
    MountSpec {
        host: PathBuf::from("/home/dev/project"),
        jail: Some(PathBuf::from("/work")),
        writable: true,
    }
}

fn a_read_only_mount() -> MountSpec {
    MountSpec {
        host: PathBuf::from("/home/dev/readonly-data"),
        jail: Some(PathBuf::from("/data")),
        writable: false,
    }
}

fn backend_options() -> QemuBackendOptions {
    QemuBackendOptions {
        image_path: PathBuf::from("/images/base.qcow2"),
        overlay_path: PathBuf::from("/tmp/overlay.qcow2"),
        runner_dir_path: PathBuf::from("/tmp/runner"),
        control_port: 6700,
    }
}

// ── ninep_fsdev_args ─────────────────────────────────────────────────────────

/// A writable mount must not carry `readonly=on` and must expose the host path + tag.
#[test]
fn ninep_fsdev_args_marks_a_writable_mount_as_read_write() {
    let mount = a_read_write_mount();
    let args = ninep_fsdev_args(0, &mount, "work0");

    let fsdev_idx = args
        .iter()
        .position(|a| a == "-fsdev")
        .expect("-fsdev flag must be present");
    assert!(
        args[fsdev_idx + 1].contains("path=/home/dev/project"),
        "-fsdev value must reference the host path, got: {:?}",
        args.get(fsdev_idx + 1)
    );
    assert!(
        !args[fsdev_idx + 1].contains("readonly=on"),
        "writable mount must not set readonly=on, got: {:?}",
        args.get(fsdev_idx + 1)
    );

    let device_idx = args
        .iter()
        .position(|a| a == "-device")
        .expect("-device flag must be present");
    assert!(
        args[device_idx + 1].contains("virtio-9p-pci"),
        "-device value must be virtio-9p-pci, got: {:?}",
        args.get(device_idx + 1)
    );
    assert!(
        args[device_idx + 1].contains("mount_tag=work0"),
        "-device value must carry the mount tag, got: {:?}",
        args.get(device_idx + 1)
    );
}

/// A mount without `writable` must carry `readonly=on` in its `-fsdev` value.
#[test]
fn ninep_fsdev_args_marks_a_read_only_mount_with_readonly_on() {
    let mount = a_read_only_mount();
    let args = ninep_fsdev_args(1, &mount, "data0");

    let fsdev_idx = args
        .iter()
        .position(|a| a == "-fsdev")
        .expect("-fsdev flag must be present");
    assert!(
        args[fsdev_idx + 1].contains("readonly=on"),
        "read-only mount must set readonly=on, got: {:?}",
        args.get(fsdev_idx + 1)
    );
}

/// Each `index` must produce a distinct `id=fsN` so concurrent fsdevs don't collide.
#[test]
fn ninep_fsdev_args_uses_a_unique_fsdev_id_per_index() {
    let mount = a_read_write_mount();
    let args_0 = ninep_fsdev_args(0, &mount, "a");
    let args_3 = ninep_fsdev_args(3, &mount, "b");

    let fsdev_idx_0 = args_0
        .iter()
        .position(|a| a == "-fsdev")
        .expect("-fsdev flag must be present for index 0");
    let fsdev_idx_3 = args_3
        .iter()
        .position(|a| a == "-fsdev")
        .expect("-fsdev flag must be present for index 3");
    assert!(
        args_0[fsdev_idx_0 + 1].contains("id=fs0"),
        "index 0 must produce fsdev id fs0, got: {:?}",
        args_0.get(fsdev_idx_0 + 1)
    );
    assert!(
        args_3[fsdev_idx_3 + 1].contains("id=fs3"),
        "index 3 must produce fsdev id fs3, got: {:?}",
        args_3.get(fsdev_idx_3 + 1)
    );
}

// ── overlay_create_argv ──────────────────────────────────────────────────────

/// Must build `qemu-img create -f qcow2 -b <base> -F qcow2 <overlay>` argv exactly.
#[test]
fn overlay_create_argv_builds_a_qcow2_backed_overlay() {
    let base = PathBuf::from("/images/base.qcow2");
    let overlay = PathBuf::from("/tmp/overlay.qcow2");
    let args = overlay_create_argv(&base, &overlay);

    assert_eq!(
        args,
        vec![
            "create".to_string(),
            "-f".to_string(),
            "qcow2".to_string(),
            "-b".to_string(),
            "/images/base.qcow2".to_string(),
            "-F".to_string(),
            "qcow2".to_string(),
            "/tmp/overlay.qcow2".to_string(),
        ],
        "overlay_create_argv must build 'create -f qcow2 -b <base> -F qcow2 <overlay>'"
    );
}

// ── guest_plan_json ──────────────────────────────────────────────────────────

/// The guest init hook needs mounts, env, cwd, and command carried through as JSON.
#[test]
fn guest_plan_json_includes_mounts_env_cwd_and_command() {
    let plan = SandboxBuilder::new(
        "/repo",
        "/scratch",
        "/scratch/egress",
        vec!["cat".to_string(), "/work/marker.txt".to_string()],
    )
    .mount(a_read_write_mount())
    .env_map(std::collections::BTreeMap::from([(
        "GREETING".to_string(),
        "hello".to_string(),
    )]))
    .cwd(Some(PathBuf::from("/work")))
    .build()
    .expect("plan must build");

    let json = guest_plan_json(&plan);
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("guest_plan_json must produce valid JSON");

    assert_eq!(
        parsed["command"],
        serde_json::json!(["cat", "/work/marker.txt"]),
        "guest plan JSON must carry the command"
    );
    assert_eq!(
        parsed["cwd"],
        serde_json::json!("/work"),
        "guest plan JSON must carry the cwd"
    );
    assert_eq!(
        parsed["env"]["GREETING"],
        serde_json::json!("hello"),
        "guest plan JSON must carry env vars"
    );
    assert_eq!(
        parsed["mounts"][0]["host"],
        serde_json::json!("/home/dev/project"),
        "guest plan JSON must carry mount host paths"
    );
    assert_eq!(
        parsed["mounts"][0]["jail"],
        serde_json::json!("/work"),
        "guest plan JSON must carry mount jail paths"
    );
    assert_eq!(
        parsed["mounts"][0]["writable"],
        serde_json::json!(true),
        "guest plan JSON must carry the writable flag"
    );
}

// ── qemu_sandbox_argv ────────────────────────────────────────────────────────

/// The VM must always boot the ephemeral overlay, never the immutable base image.
#[test]
fn qemu_sandbox_argv_boots_the_overlay_not_the_base_image() {
    let plan = SandboxBuilder::new(
        "/repo",
        "/scratch",
        "/scratch/egress",
        vec!["true".to_string()],
    )
    .build()
    .expect("plan must build");
    let opts = backend_options();

    let args = qemu_sandbox_argv(&plan, &opts);

    let drive_idx = args
        .iter()
        .position(|a| a == "-drive")
        .expect("-drive flag must be present");
    assert!(
        args[drive_idx + 1].contains("/tmp/overlay.qcow2"),
        "-drive must boot the overlay, not the base image, got: {:?}",
        args.get(drive_idx + 1)
    );
    assert!(
        !args[drive_idx + 1].contains("/images/base.qcow2"),
        "-drive must not reference the immutable base image directly, got: {:?}",
        args.get(drive_idx + 1)
    );
}

/// Each `MountSpec` becomes its own 9p share, on top of the reserved runner+plan shares,
/// with read-only flags matching `writable`.
#[test]
fn qemu_sandbox_argv_includes_one_9p_share_per_mount_with_correct_readonly_flags() {
    let plan = SandboxBuilder::new(
        "/repo",
        "/scratch",
        "/scratch/egress",
        vec!["true".to_string()],
    )
    .mounts(vec![a_read_write_mount(), a_read_only_mount()])
    .build()
    .expect("plan must build");
    let opts = backend_options();

    let args = qemu_sandbox_argv(&plan, &opts);

    let fsdev_values: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| *a == "-fsdev" && args.get(i + 1).is_some())
        .map(|(i, _)| &args[i + 1])
        .collect();
    assert_eq!(
        fsdev_values.len(),
        4,
        "must emit one -fsdev per MountSpec (2) plus the reserved runner+plan shares (2), got: {args:?}"
    );
    assert!(
        fsdev_values
            .iter()
            .any(|v| v.contains("/home/dev/project") && !v.contains("readonly=on")),
        "the read-write mount must appear without readonly=on, got: {fsdev_values:?}"
    );
    assert!(
        fsdev_values
            .iter()
            .any(|v| v.contains("/home/dev/readonly-data") && v.contains("readonly=on")),
        "the read-only mount must appear with readonly=on, got: {fsdev_values:?}"
    );
}

/// The runner binary and the guest plan config must each get their own reserved 9p
/// share, regardless of how many `MountSpec`s the caller adds.
#[test]
fn qemu_sandbox_argv_includes_the_reserved_runner_and_plan_shares() {
    let plan = SandboxBuilder::new(
        "/repo",
        "/scratch",
        "/scratch/egress",
        vec!["true".to_string()],
    )
    .build()
    .expect("plan must build");
    let opts = backend_options();

    let args = qemu_sandbox_argv(&plan, &opts);
    let device_values: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| *a == "-device" && args.get(i + 1).is_some())
        .map(|(i, _)| &args[i + 1])
        .collect();

    assert!(
        device_values
            .iter()
            .any(|v| v.contains(&format!("mount_tag={RUNNER_MOUNT_TAG}"))),
        "argv must reserve a 9p share for the in-guest runner, got: {device_values:?}"
    );
    assert!(
        device_values
            .iter()
            .any(|v| v.contains(&format!("mount_tag={PLAN_MOUNT_TAG}"))),
        "argv must reserve a 9p share for the guest plan config, got: {device_values:?}"
    );
}

/// The control port and every `loopback_allow_ports` entry must be forwarded via
/// `-netdev user,...,hostfwd=...`.
#[test]
fn qemu_sandbox_argv_forwards_the_control_port_and_loopback_ports() {
    let plan = SandboxBuilder::new(
        "/repo",
        "/scratch",
        "/scratch/egress",
        vec!["true".to_string()],
    )
    .network(NetworkSpec {
        loopback_allow_ports: vec![9000],
        allow_oauth_inbound: false,
    })
    .build()
    .expect("plan must build");
    let opts = backend_options();

    let args = qemu_sandbox_argv(&plan, &opts);
    let netdev_idx = args
        .iter()
        .position(|a| a == "-netdev")
        .expect("-netdev flag must be present");
    assert!(
        args[netdev_idx + 1].contains("hostfwd=tcp::6700-:6700"),
        "-netdev value must forward the control port, got: {:?}",
        args.get(netdev_idx + 1)
    );
    assert!(
        args[netdev_idx + 1].contains("hostfwd=tcp::9000-:9000"),
        "-netdev value must forward loopback_allow_ports entries, got: {:?}",
        args.get(netdev_idx + 1)
    );
}
