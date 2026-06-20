//! Acceptance test: `buildroot_image` and `qemu_disk_image` target types must be registered
//! in the `tddy-tools build` plugin registry so that demo VM images can be built through
//! the real CLI without the agent having to shell out to make/qemu-img manually.
//!
//! These tests use `build --dry-run` (not `build-list`) because `build-list` reads manifests
//! without lowering targets, so it succeeds regardless of plugin registration. A `--dry-run`
//! invokes `lower_target` which consults the plugin registry — the correct failure mode when
//! a plugin is missing is an error like "unknown target type: buildroot_image".

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use serde_json::Value;

fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

const BUILD_YAML_BUILDROOT: &str = r#"
schema_version: 1
targets:
  - id: "my-os:rootfs"
    name: "OS Rootfs"
    config:
      type: buildroot_image
      defconfig: qemu_x86_64_defconfig
      buildroot_dir: external/buildroot
      output_dir: build/br-out
"#;

const BUILD_YAML_QEMU_ONLY: &str = r#"
schema_version: 1
targets:
  - id: "my-os:qcow2"
    name: "OS qcow2"
    config:
      type: qemu_disk_image
      input: build/br-out/images/rootfs.ext4
"#;

const BUILD_YAML_BOTH: &str = r#"
schema_version: 1
targets:
  - id: "my-os:rootfs"
    name: "OS Rootfs"
    config:
      type: buildroot_image
      defconfig: qemu_x86_64_defconfig
      buildroot_dir: external/buildroot
      output_dir: build/br-out
  - id: "my-os:qcow2"
    name: "OS qcow2"
    deps: ["my-os:rootfs"]
    config:
      type: qemu_disk_image
      input: build/br-out/images/rootfs.ext4
"#;

fn write_repo_with_build_yaml(content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("BUILD.yaml"), content).expect("write BUILD.yaml");
    dir
}

/// `tddy-tools build --dry-run` a `buildroot_image` target must succeed —
/// i.e. `BuildrootPlugin` is registered so `lower_target` can handle the type.
///
/// If `BuildrootPlugin` is NOT registered, the command fails with "unknown target type".
/// This test will be RED until the plugin is added to `plugin_registry()` in `build_cli.rs`.
#[test]
fn buildroot_plugin_registered_in_cli_dry_run() {
    let dir = write_repo_with_build_yaml(BUILD_YAML_BUILDROOT);
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "build",
        "--repo-dir",
        dir.path().to_str().unwrap(),
        "--target",
        "my-os:rootfs",
        "--dry-run",
    ]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");
    let v: Value = serde_json::from_str(&stdout).expect("dry-run output must be JSON");
    // Dry-run output has an `actions` array with the planned commands.
    assert!(
        v.get("actions").is_some() || v.get("target").is_some(),
        "dry-run output must describe the planned actions, got: {v}"
    );
}

/// `tddy-tools build --dry-run` a `qemu_disk_image` target must succeed —
/// i.e. `QemuPlugin` is registered.
///
/// This test will be RED until the plugin is added to `plugin_registry()` in `build_cli.rs`.
#[test]
fn qemu_disk_image_plugin_registered_in_cli_dry_run() {
    let dir = write_repo_with_build_yaml(BUILD_YAML_QEMU_ONLY);
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "build",
        "--repo-dir",
        dir.path().to_str().unwrap(),
        "--target",
        "my-os:qcow2",
        "--dry-run",
    ]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");
    let v: Value = serde_json::from_str(&stdout).expect("dry-run output must be JSON");
    assert!(
        v.get("actions").is_some() || v.get("target").is_some(),
        "dry-run output must describe the planned actions, got: {v}"
    );
}

/// Both plugins registered: a BUILD.yaml with both types must produce two targets on dry-run.
///
/// This is the canonical "both plugins registered" acceptance test (the primary one that
/// maps to `buildroot_and_qemu_plugins_registered_in_cli_registry` in the plan).
#[test]
fn buildroot_and_qemu_plugins_registered_in_cli_registry() {
    let dir = write_repo_with_build_yaml(BUILD_YAML_BOTH);

    // Dry-run the leaf (qcow2) target which depends on rootfs; both plugins must resolve.
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "build",
        "--repo-dir",
        dir.path().to_str().unwrap(),
        "--target",
        "my-os:qcow2",
        "--dry-run",
    ]);
    // If either plugin is unregistered this will exit non-zero with "unknown target type".
    cmd.assert().success();
}
