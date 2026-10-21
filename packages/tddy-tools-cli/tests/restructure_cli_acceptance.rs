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
        stdout.contains("budget: 1 of 2 file(s) over 10 production lines"),
        "the budget summary did not reach stdout, got: {stdout}"
    );
    assert!(
        stdout.contains("budget: src/big.rs is 12 production lines, 2 over"),
        "the over-budget file was not named with how far over it is, got: {stdout}"
    );
    assert!(
        !stdout.contains("src/small.rs"),
        "a file within the budget was reported, got: {stdout}"
    );
}

#[test]
fn restructure_load_without_a_daemon_is_refused_as_needing_one() {
    // Given no warm index daemon in the environment
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.jsonl");
    fs::write(&plan, "{\"v\":1,\"snapshot\":{}}\n").expect("plan");

    // When a plan is loaded
    let mut cmd = tddy_tools_bin();
    cmd.env_remove("TDDY_INDEX_SOCKET");
    cmd.current_dir(dir.path());
    cmd.args(["restructure", "load", plan.to_str().unwrap()]);
    let assert = cmd.assert().failure();

    // Then the refusal says a daemon is needed and how to start one
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains(
            "`restructure load` needs the index daemon — start one with ./run-index-daemon and \
             export TDDY_INDEX_SOCKET"
        ),
        "the refusal did not name the daemon, got: {stderr}"
    );
}

/// A workspace holding `src/lib.rs`, and a plan of one item-anchored operation with no header.
fn a_workspace_with_a_headerless_plan() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("src");
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn foo() -> u32 {\n    1\n}\n",
    )
    .expect("source");
    let operation = r#"{"op":"extract_module","anchor":{"kind":"items","file":"src/lib.rs","items":["demo::foo"],"fingerprints":["sha256:aa"]},"name":"grouped"}"#;
    let plan = dir.path().join("plan.jsonl");
    fs::write(&plan, format!("{operation}\n")).expect("plan");
    (dir, plan, operation.to_string())
}

fn the_lines_of(plan: &std::path::Path) -> Vec<String> {
    fs::read_to_string(plan)
        .expect("read the plan")
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn restructure_snapshot_writes_the_header_a_plan_of_operations_lacks() {
    // Given a headerless plan, and no index daemon named
    let (dir, plan, operation) = a_workspace_with_a_headerless_plan();

    // When it is snapshotted
    let mut cmd = tddy_tools_bin();
    cmd.env_remove("TDDY_INDEX_SOCKET");
    cmd.current_dir(dir.path());
    cmd.args(["restructure", "snapshot", plan.to_str().unwrap()]);
    cmd.assert().success();

    // Then line 1 is a header and the operation is as it was written
    let lines = the_lines_of(&plan);
    assert!(
        lines[0].starts_with(r#"{"v":2,"files":{"src/lib.rs""#),
        "{}",
        lines[0]
    );
    assert_eq!(&lines[1..], [operation]);
}

#[test]
fn restructure_snapshot_of_a_headerless_plan_does_not_dial_a_named_daemon() {
    // Given a headerless plan, and an index daemon socket nothing listens on
    let (dir, plan, _) = a_workspace_with_a_headerless_plan();
    let nothing_listens_here = dir.path().join("no-daemon.sock");

    // When it is snapshotted
    let mut cmd = tddy_tools_bin();
    cmd.env("TDDY_INDEX_SOCKET", &nothing_listens_here);
    cmd.current_dir(dir.path());
    cmd.args(["restructure", "snapshot", plan.to_str().unwrap()]);

    // Then it succeeds, which a dial to that socket could not have, and the header is there
    cmd.assert().success();
    assert!(the_lines_of(&plan)[0].starts_with(r#"{"v":2"#));
}

#[test]
fn restructure_check_of_a_headerless_plan_names_snapshot_as_the_remedy() {
    // Given a headerless plan
    let (dir, plan, _) = a_workspace_with_a_headerless_plan();

    // When it is checked
    let mut cmd = tddy_tools_bin();
    cmd.env_remove("TDDY_INDEX_SOCKET");
    cmd.current_dir(dir.path());
    cmd.args(["restructure", "check", plan.to_str().unwrap()]);
    let assert = cmd.assert().failure();

    // Then the refusal names the command that writes the header
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(stderr.contains("restructure snapshot"), "{stderr}");
}
