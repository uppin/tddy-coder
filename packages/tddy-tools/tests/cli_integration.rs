//! Acceptance tests for tddy-tools CLI: submit, ask, get-schema, --mcp, help.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;

/// Parity with `tddy_tools::schema::GOAL_SCHEMA_FILES` — the registered workflow goals.
const REGISTERED_GOALS: &[&str] = &[
    "plan",
    "acceptance-tests",
    "red",
    "green",
    "post-green-review",
    "evaluate-changes",
    "validate",
    "refactor",
    "update-docs",
    "demo",
];

/// Expected `$id` for each CLI goal (differs from the CLI name where the URN uses a shorter id).
fn expected_schema_id_for_goal(goal: &str) -> &'static str {
    match goal {
        "plan" => "urn:tddy:goal/plan",
        "acceptance-tests" => "urn:tddy:goal/acceptance-tests",
        "red" => "urn:tddy:goal/red",
        "green" => "urn:tddy:goal/green",
        "post-green-review" => "urn:tddy:goal/post-green-review",
        "evaluate-changes" => "urn:tddy:goal/evaluate",
        "validate" => "urn:tddy:goal/validate-subagents",
        "refactor" => "urn:tddy:goal/refactor",
        "update-docs" => "urn:tddy:goal/update-docs",
        "demo" => "urn:tddy:goal/demo",
        _ => panic!("unexpected goal: {goal}"),
    }
}

/// Subprocess must not inherit `TDDY_SOCKET` from the parent (e.g. a running tddy-coder session),
/// or submit/ask would relay to the live relay instead of the no-socket success path.
fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

/// Path to the generated schema manifest once the proto → JSON Schema pipeline exists (PRD F2/F6).
fn workflow_recipes_generated_manifest() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tddy-workflow-recipes/generated/schema-manifest.json")
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

/// AC3/F4: `list-schemas` must enumerate every registered goal (stable sort). Fails until implemented.
#[test]
fn list_schemas_prints_all_registered_goals() {
    let mut cmd = tddy_tools_bin();
    cmd.args(["list-schemas"]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let v: Value = serde_json::from_str(stdout.trim()).expect("list-schemas stdout must be JSON");
    let goals = v
        .get("goals")
        .and_then(|g| g.as_array())
        .expect("list-schemas JSON must contain a \"goals\" array");
    let mut names: Vec<&str> = goals
        .iter()
        .map(|x| x.as_str().expect("goal name string"))
        .collect();
    names.sort_unstable();
    let mut expected: Vec<&str> = REGISTERED_GOALS.to_vec();
    expected.sort_unstable();
    assert_eq!(
        names, expected,
        "list-schemas must list exactly the registered workflow goals"
    );
}

/// F2/F6 + AC2: generated manifest must exist; each `get-schema <goal>` must return parseable JSON Schema with keywords for that goal.
#[test]
fn get_schema_returns_non_empty_json_for_each_registered_goal() {
    let manifest_path = workflow_recipes_generated_manifest();
    assert!(
        manifest_path.is_file(),
        "expected generated schema manifest at {} (proto → JSON Schema pipeline; PRD F2/F6)",
        manifest_path.display()
    );

    for goal in REGISTERED_GOALS {
        let mut cmd = tddy_tools_bin();
        cmd.args(["get-schema", goal]);
        let assert = cmd.assert().success();
        let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
        let schema: Value =
            serde_json::from_str(stdout.trim()).expect("get-schema stdout must be JSON");
        assert!(
            schema.get("$schema").is_some(),
            "goal {goal}: schema must declare $schema"
        );
        assert_eq!(
            schema.get("$id").and_then(|v| v.as_str()),
            Some(expected_schema_id_for_goal(goal)),
            "goal {goal}: $id must match registered URN"
        );
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "goal {goal}: top-level type must be object"
        );
    }
}

/// AC4: invalid plan payload must fail with non-zero exit; errors must mention a concrete path/constraint; F4 discovery tip must mention `list-schemas`.
#[test]
fn submit_rejects_invalid_json_for_plan_goal() {
    let invalid_json = r#"{"goal":"plan","todo":"- [ ] Task 1"}"#;

    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--goal", "plan", "--data", invalid_json]);
    let assert = cmd.assert().code(3);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stdout.contains("\"status\":\"error\"") && stdout.contains("\"errors\""),
        "stdout must surface validation errors: {stdout}"
    );
    assert!(
        stdout.contains("/prd") || stdout.contains("prd"),
        "errors should mention prd or /prd path: {stdout}"
    );
    assert!(
        stderr.contains("list-schemas") || stdout.contains("list-schemas"),
        "validation tip must mention list-schemas for discovery (AC3/F4); stderr={stderr} stdout={stdout}"
    );
}

/// AC4 + AC3: minimal valid plan passes submit; registered goals are discoverable via `list-schemas`.
#[test]
fn submit_accepts_minimal_valid_plan_payload_matching_schema() {
    let valid_json = r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X"}"##;

    let mut cmd = tddy_tools_bin();
    cmd.args(["submit", "--goal", "plan", "--data", valid_json]);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("\"status\":\"ok\""))
        .stdout(predicates::str::contains("\"goal\":\"plan\""));

    let mut cmd = tddy_tools_bin();
    cmd.args(["list-schemas"]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let v: Value = serde_json::from_str(stdout.trim()).expect("list-schemas JSON");
    let goals = v["goals"].as_array().expect("goals array");
    assert!(
        goals.iter().filter_map(|g| g.as_str()).any(|g| g == "plan"),
        "list-schemas must include plan; got {v}"
    );
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
fn ask_accepts_options_without_description() {
    let questions = r#"{"questions":[{"header":"Scope","question":"Pick one","options":[{"label":"Only label"}],"multiSelect":false}]}"#;

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

/// Relay `ToolCallResponse::Error` uses `message`, not `errors`. tddy-tools must surface it
/// (matches `packages/tddy-core/src/toolcall/mod.rs` serialization).
#[cfg(unix)]
#[test]
fn submit_relay_error_with_message_surfaces_detail() {
    use std::io::{BufRead, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    let dir = tempfile::tempdir().expect("tempdir");
    let sock_path = dir.path().join("relay.sock");
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path).expect("bind");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut line = String::new();
        std::io::BufReader::new(&mut stream)
            .read_line(&mut line)
            .expect("read request");
        let resp = r#"{"status":"error","message":"presenter did not respond to submit relay in time — poll_tool_calls"}"#;
        writeln!(stream, "{}", resp).expect("write response");
        stream.flush().ok();
    });

    let valid_json =
        r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X","todo":"- [ ] Task 1"}"##;

    let mut cmd = tddy_tools_bin();
    cmd.env("TDDY_SOCKET", &sock_path);
    cmd.args(["submit", "--goal", "plan", "--data", valid_json]);

    let assertion = cmd.assert().code(1);
    let stdout = String::from_utf8_lossy(&assertion.get_output().stdout);
    assert!(
        !stdout.contains("\"relay failed\""),
        "expected relay detail to replace generic relay failure; stdout={stdout}"
    );
    assertion
        .stdout(predicates::str::contains("presenter did not respond"))
        .stdout(predicates::str::contains("poll_tool_calls"));

    handle.join().expect("server thread");
}
