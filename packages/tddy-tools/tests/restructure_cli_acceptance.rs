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

/// The file budget is a report, not a gate: a check whose plan names an over-budget file says so
/// and still returns the verdict its findings earned. That is what makes it usable for recording an
/// outcome rather than only for failing a build.
#[test]
fn restructure_check_reports_the_files_a_plan_names_that_are_over_the_budget() {
    // Given a plan naming one 12-line module and one 3-line module
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("src");
    fs::write(dir.path().join("src/big.rs"), "// a line\n".repeat(12)).expect("big module");
    fs::write(dir.path().join("src/small.rs"), "// a line\n".repeat(3)).expect("small module");
    let plan = dir.path().join("plan.jsonl");
    fs::write(
        &plan,
        r#"{"v":1,"snapshot":{}}
{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/big.rs","path":"Registry"},"name":"HostRegistry"}
{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/small.rs","path":"Clock"},"name":"HostClock"}
"#,
    )
    .expect("plan");

    // When the plan is checked against a 10-line budget
    let mut cmd = tddy_tools_bin();
    cmd.current_dir(dir.path());
    cmd.args([
        "restructure",
        "check",
        plan.to_str().unwrap(),
        "--budget",
        "10",
    ]);
    let assert = cmd.assert().success();

    // Then only the file over the budget is reported, with how far over it is
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("budget: 1 of 2 file(s) over 10 lines"),
        "the budget summary did not reach stdout, got: {stdout}"
    );
    assert!(
        stdout.contains("budget: src/big.rs is 12 lines, 2 over"),
        "the over-budget file was not named with how far over it is, got: {stdout}"
    );
    assert!(
        !stdout.contains("src/small.rs"),
        "a file within the budget was reported, got: {stdout}"
    );
}
