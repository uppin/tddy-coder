//! Acceptance tests for `tddy-tools restructure` subcommands.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use std::fs;

fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

#[test]
fn restructure_check_rejects_plan_carrying_code_text() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("bad-plan.jsonl");
    fs::write(
        &plan,
        r#"{"v":1,"snapshot":{}}
{"op":"extract_method","anchor":{"symbol":{"file":"src/lib.rs","path":"helper"}},"text":"fn helper() {}"}
"#,
    )
    .expect("plan");

    // When
    let mut cmd = tddy_tools_bin();
    cmd.current_dir(dir.path());
    cmd.args(["restructure", "check", plan.to_str().unwrap()]);
    let assert = cmd.assert().failure();

    // Then
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("code text") || stderr.contains("text"),
        "stderr must refuse code-bearing plans, got: {stderr}"
    );
}

#[test]
fn restructure_check_rejects_malformed_plan_header() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("empty.jsonl");
    fs::write(&plan, "").expect("plan");

    // When
    let mut cmd = tddy_tools_bin();
    cmd.current_dir(dir.path());
    cmd.args(["restructure", "check", plan.to_str().unwrap()]);
    let assert = cmd.assert().failure();

    // Then
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("malformed") || stderr.contains("plan"),
        "stderr must report malformed plan, got: {stderr}"
    );
}
