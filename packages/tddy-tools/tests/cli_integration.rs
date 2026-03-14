//! Acceptance tests for tddy-tools CLI: submit, ask, get-schema, --mcp, help.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

fn tddy_tools_bin() -> Command {
    cargo_bin_cmd!("tddy-tools")
}

#[test]
fn help_text_is_comprehensive() {
    let mut cmd = tddy_tools_bin();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("submit"));
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("ask"));
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("--mcp"));
}

#[test]
fn submit_help_includes_examples() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("--goal"));
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("--data"));
}

#[test]
fn submit_valid_json_with_goal_returns_success() {
    let valid_json =
        r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X","todo":"- [ ] Task 1"}"##;

    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--goal", "plan", "--data", valid_json]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""))
        .stdout(predicates::str::contains("\"goal\":\"plan\""));
}

#[test]
fn submit_invalid_json_returns_validation_error() {
    let invalid_json = r#"{"goal":"plan","todo":"- [ ] Task 1"}"#;

    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--goal", "plan", "--data", invalid_json]);
    cmd.assert()
        .code(3)
        .stdout(predicates::str::contains("\"status\":\"error\""))
        .stdout(predicates::str::contains("\"errors\""));
}

#[test]
fn submit_reads_from_stdin() {
    let valid_json =
        r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X","todo":"- [ ] Task 1"}"##;

    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--goal", "plan"])
        .write_stdin(valid_json);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""));
}

#[test]
fn submit_data_stdin_reads_json_from_stdin() {
    let valid_json = r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X. Session state is logged for debugging.","todo":"- [ ] Task 1"}"##;

    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--goal", "plan", "--data-stdin"])
        .write_stdin(valid_json);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""))
        .stdout(predicates::str::contains("\"goal\":\"plan\""));
}

#[test]
fn submit_malformed_json_returns_parse_error() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--goal", "plan", "--data", "not valid json {"]);
    cmd.assert()
        .code(1)
        .stdout(predicates::str::contains("error"));
}

#[test]
fn get_schema_plan_outputs_schema() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["get-schema", "plan"]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("$schema"))
        .stdout(predicates::str::contains("plan"));
}

#[test]
fn get_schema_unknown_goal_returns_error() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["get-schema", "unknown"]);
    cmd.assert()
        .code(2)
        .stderr(predicates::str::contains("unknown goal"));
}

#[test]
fn ask_valid_questions_returns_success_when_no_socket() {
    let questions = r#"{"questions":[{"header":"Scope","question":"Which modules?","options":[{"label":"A","description":"Option A"}],"multiSelect":false}]}"#;

    let mut cmd = tddy_tools_bin();
    cmd.args(["ask", "--data", questions]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""));
}

#[test]
fn ask_malformed_input_returns_error() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["ask", "--data", "not json"]);
    cmd.assert().code(1);
}

#[test]
fn ask_missing_questions_array_returns_error() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["ask", "--data", "{}"]);
    cmd.assert().code(2);
}

#[test]
fn mcp_mode_does_not_require_subcommand() {
    let mut cmd = tddy_tools_bin();
    cmd.arg("--mcp").write_stdin("");
    let output = cmd.output().expect("run tddy-tools --mcp");
    let code = output.status.code();
    assert!(
        code == Some(0) || code == Some(1),
        "MCP mode should exit (0=ok, 1=connection closed); got {:?}",
        code
    );
}
