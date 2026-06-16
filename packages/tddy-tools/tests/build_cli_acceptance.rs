//! Acceptance tests for the `tddy-tools build` / `build-list` subcommands.
//!
//! These run the built binary in local mode (no `TDDY_SOCKET`) against a
//! temporary repo containing a `BUILD.yaml`.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use serde_json::Value;

fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

const BUILD_YAML: &str = r#"
schema_version: 1
targets:
  - id: "app:bin"
    name: "App Binary"
    config:
      type: rust_binary
      package: app
      bin_name: app
  - id: "hello:script"
    name: "Hello Script"
    config:
      type: script
      command: ["echo", "hello-from-cli"]
"#;

fn write_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("BUILD.yaml"), BUILD_YAML).expect("write BUILD.yaml");
    dir
}

#[test]
fn build_list_outputs_json_with_all_targets() {
    let dir = write_repo();
    let mut cmd = tddy_tools_bin();
    cmd.args(["build-list", "--repo-dir", dir.path().to_str().unwrap()]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");

    let v: Value = serde_json::from_str(&stdout).expect("stdout must be JSON");
    let targets = v
        .get("targets")
        .and_then(|t| t.as_array())
        .expect("JSON must have a `targets` array");
    assert_eq!(targets.len(), 2, "both declared targets must be listed");
    assert_eq!(v.get("total").and_then(|t| t.as_u64()), Some(2));
}

#[test]
fn build_cli_executes_script_target() {
    let dir = write_repo();
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "build",
        "--repo-dir",
        dir.path().to_str().unwrap(),
        "--target",
        "hello:script",
    ]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    assert!(
        stdout.contains("hello-from-cli"),
        "build output must include captured script stdout, got: {stdout}"
    );
}

#[test]
fn build_cli_dry_run_prints_plan_without_executing() {
    let dir = write_repo();
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "build",
        "--repo-dir",
        dir.path().to_str().unwrap(),
        "--target",
        "app:bin",
        "--dry-run",
    ]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    assert!(
        stdout.contains("cargo"),
        "dry-run must surface the planned argv (cargo), got: {stdout}"
    );
    // Dry-run must not have produced any build output under the repo.
    assert!(
        !dir.path().join(".tddy-build/out").exists(),
        "dry-run must not materialize build outputs"
    );
}
