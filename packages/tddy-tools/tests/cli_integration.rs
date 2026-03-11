//! Acceptance tests for tddy-tools CLI: submit, ask, --mcp, help.

use assert_cmd::Command;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn tddy_tools_bin() -> Command {
    Command::cargo_bin("tddy-tools").expect("tddy-tools binary")
}

fn copy_plan_schema_to(dir: &Path) -> std::io::Result<std::path::PathBuf> {
    let schema_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tddy-core/schemas/plan.schema.json");
    let schema_dest = dir.join("plan.schema.json");
    fs::copy(&schema_src, &schema_dest)?;
    Ok(schema_dest)
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
        .stdout(predicates::str::contains("--schema"));
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("--data"));
}

#[test]
fn submit_valid_json_with_schema_returns_success() {
    let tmp = TempDir::new().expect("temp dir");
    let schema_path = copy_plan_schema_to(tmp.path()).expect("copy schema");
    let valid_json =
        r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X","todo":"- [ ] Task 1"}"##;

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "submit",
        "--schema",
        schema_path.to_str().unwrap(),
        "--data",
        valid_json,
    ]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""))
        .stdout(predicates::str::contains("\"goal\":\"plan\""));
}

#[test]
fn submit_invalid_json_returns_validation_error() {
    let tmp = TempDir::new().expect("temp dir");
    let schema_path = copy_plan_schema_to(tmp.path()).expect("copy schema");
    let invalid_json = r#"{"goal":"plan","todo":"- [ ] Task 1"}"#;

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "submit",
        "--schema",
        schema_path.to_str().unwrap(),
        "--data",
        invalid_json,
    ]);
    cmd.assert()
        .code(3)
        .stdout(predicates::str::contains("\"status\":\"error\""))
        .stdout(predicates::str::contains("\"errors\""));
}

#[test]
fn submit_reads_from_stdin() {
    let tmp = TempDir::new().expect("temp dir");
    let schema_path = copy_plan_schema_to(tmp.path()).expect("copy schema");
    let valid_json =
        r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X","todo":"- [ ] Task 1"}"##;

    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--schema", schema_path.to_str().unwrap()])
        .write_stdin(valid_json);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""));
}

#[test]
fn submit_malformed_json_returns_parse_error() {
    let tmp = TempDir::new().expect("temp dir");
    let schema_path = copy_plan_schema_to(tmp.path()).expect("copy schema");

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "submit",
        "--schema",
        schema_path.to_str().unwrap(),
        "--data",
        "not valid json {",
    ]);
    cmd.assert()
        .code(1)
        .stdout(predicates::str::contains("error"));
}

#[test]
fn schema_file_not_found_returns_error() {
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "submit",
        "--schema",
        "/nonexistent/schemas/plan.schema.json",
        "--data",
        r#"{"goal":"plan","prd":"x","todo":"y"}"#,
    ]);
    cmd.assert()
        .code(1)
        .stderr(predicates::str::contains("not found"));
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
