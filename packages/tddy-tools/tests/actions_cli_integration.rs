//! Integration acceptance tests: `tddy-tools actions list|run` with temp `session_dir/actions/`.
//!
//! See PRD Testing Plan (Session actions via tddy-tools).

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;

fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

fn write_action(session_root: &Path, filename: &str, yaml: &str) {
    let actions = session_root.join("actions");
    fs::create_dir_all(&actions).expect("actions dir");
    fs::write(actions.join(filename), yaml).expect("write action yaml");
}

fn assert_instance_matches_schema(instance: &Value, schema: &Value) {
    let validator = jsonschema::options()
        .build(schema)
        .expect("fixture output_schema must compile");
    let errs: Vec<_> = validator.iter_errors(instance).collect();
    assert!(
        errs.is_empty(),
        "output must conform to output_schema: {:?}",
        errs
    );
}

#[test]
fn cli_lists_session_actions() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_action(
        dir.path(),
        "one.yaml",
        r#"
id: alpha
description: First
input_schema:
  type: object
output_schema:
  type: object
executor:
  type: cmd
  argv:
    - literal: "true"
"#,
    );
    write_action(
        dir.path(),
        "two.yaml",
        r#"
id: beta
description: Second
input_schema:
  type: object
output_schema:
  type: object
executor:
  type: cmd
  argv:
    - literal: "true"
"#,
    );

    let mut cmd = tddy_tools_bin();
    cmd.args(["actions", "list", "--session-dir"])
        .arg(dir.path());
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let v: Value = serde_json::from_str(stdout.trim()).expect("list stdout must be JSON");
    let actions = v
        .get("actions")
        .and_then(|a| a.as_array())
        .expect("\"actions\" array");
    assert_eq!(
        actions.len(),
        2,
        "one entry per YAML file under session_dir/actions/"
    );
    let mut ids: Vec<String> = actions
        .iter()
        .map(|a| {
            a.get("id")
                .and_then(|x| x.as_str())
                .expect("action.id")
                .to_string()
        })
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
}

/// JSON Schema for `run` stdout (mirrors `output_schema` in the action YAML).
const RUN_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["status", "exit_code"],
  "additionalProperties": true,
  "properties": {
    "status": { "type": "string", "const": "completed" },
    "exit_code": { "type": "integer" }
  }
}"#;

#[test]
fn run_outputs_match_output_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_action(
        dir.path(),
        "trivial.yaml",
        r#"
id: trivial_ok
description: structured run output
input_schema:
  type: object
  additionalProperties: false
output_schema:
  type: object
  required: [status, exit_code]
  additionalProperties: true
  properties:
    status:
      type: string
      const: completed
    exit_code:
      type: integer
executor:
  type: cmd
  argv:
    - literal: "true"
"#,
    );

    let output_schema: Value =
        serde_json::from_str(RUN_OUTPUT_SCHEMA.trim()).expect("RUN_OUTPUT_SCHEMA");

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "actions",
        "run",
        "--session-dir",
        dir.path().to_str().unwrap(),
        "--action",
        "trivial_ok",
        "--data",
        "{}",
    ]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let out: Value = serde_json::from_str(stdout.trim()).expect("run stdout must be JSON");
    assert_instance_matches_schema(&out, &output_schema);
    assert_eq!(out["status"], "completed");
    assert_eq!(out["exit_code"], 0);
}

const SWEEP_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["results"],
  "properties": {
    "results": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "stdout_path", "stderr_path", "passed"],
        "properties": {
          "name": { "type": "string" },
          "stdout_path": { "type": "string" },
          "stderr_path": { "type": "string" },
          "passed": { "type": "boolean" }
        }
      }
    }
  }
}"#;

#[test]
fn acceptance_sweep_per_test_array() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_action(
        dir.path(),
        "sweep.yaml",
        r#"
id: acceptance_sweep
description: sequential per-test artifacts
input_schema:
  type: object
  required: [tests]
  additionalProperties: false
  properties:
    tests:
      type: array
      items:
        type: string
output_schema:
  type: object
  required: [results]
  properties:
    results:
      type: array
      items:
        type: object
        required: [name, stdout_path, stderr_path, passed]
        properties:
          name: { type: string }
          stdout_path: { type: string }
          stderr_path: { type: string }
          passed: { type: boolean }
executor:
  type: cmd
  argv:
    - literal: "true"
"#,
    );

    let schema: Value = serde_json::from_str(SWEEP_OUTPUT_SCHEMA).expect("schema");
    let n = 2usize;
    let input = serde_json::json!({"tests": ["case_a", "case_b"]});

    let mut cmd = tddy_tools_bin();
    cmd.args([
        "actions",
        "run",
        "--session-dir",
        dir.path().to_str().unwrap(),
        "--action",
        "acceptance_sweep",
        "--data",
        &input.to_string(),
    ]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let out: Value = serde_json::from_str(stdout.trim()).expect("run JSON");

    assert_instance_matches_schema(&out, &schema);
    let results = out["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        n,
        "N declared tests => N per-test result objects"
    );
    let session_base = dir.path().canonicalize().expect("canonical");
    for (i, item) in results.iter().enumerate() {
        let passed = item["passed"].as_bool().expect("passed");
        assert!(
            passed,
            "fixture subprocesses must succeed; result[{i}] passed={passed}"
        );
        for key in ["stdout_path", "stderr_path"] {
            let p = item[key].as_str().expect(key);
            let path = Path::new(p);
            assert!(
                path.starts_with(&session_base),
                "{key} must be under session_dir: {p}"
            );
            assert!(
                fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false),
                "{key} must exist and be non-empty when subprocess wrote output: {p}"
            );
        }
    }
}
