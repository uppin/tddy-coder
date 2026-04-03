//! Unit-level acceptance tests: argv interpolation, validation-before-exec, MCP argument mapping.
//!
//! See PRD Testing Plan (Session actions via tddy-tools).

use serde_json::json;
use tddy_tools::session_actions::{
    discover_action_yaml_paths, map_mcp_tool_arguments, materialize_argv_from_templates,
    validate_and_interpolate_cmd_argv, validate_instance_against_schema,
};

/// Fixture Cmd action: literals and `/package`, `/filter` placeholders (JSON Pointer to input).
const CMD_FIXTURE_YAML: &str = r#"
id: cargo_test_filtered
description: Run cargo test with package and filter
input_schema:
  type: object
  required: [package, filter]
  additionalProperties: false
  properties:
    package: { type: string }
    filter: { type: string }
output_schema:
  type: object
executor:
  type: cmd
  argv:
    - literal: cargo
    - literal: test
    - literal: "-p"
    - from: /package
    - from: /filter
"#;

#[test]
fn interpolate_structured_input_to_argv() {
    let input = json!({"package":"tddy-core","filter":"interpolate_structured_input_to_argv"});
    let argv = validate_and_interpolate_cmd_argv(CMD_FIXTURE_YAML, &input)
        .expect("valid input must resolve to argv (no spawn)");
    assert_eq!(
        argv,
        vec![
            "cargo".to_string(),
            "test".to_string(),
            "-p".to_string(),
            "tddy-core".to_string(),
            "interpolate_structured_input_to_argv".to_string(),
        ],
        "deep equality: literals and bound fields form the golden argv"
    );
}

const CMD_INTEGER_FIELD_YAML: &str = r#"
id: typed_action
description: requires integer count
input_schema:
  type: object
  required: [count]
  additionalProperties: false
  properties:
    count: { type: integer }
output_schema:
  type: object
executor:
  type: cmd
  argv:
    - from: /count
"#;

#[test]
fn interpolate_rejects_invalid_input_before_exec() {
    let bad = json!({"count": "not-an-int"});
    let err = validate_and_interpolate_cmd_argv(CMD_INTEGER_FIELD_YAML, &bad)
        .expect_err("wrong type must fail with structured validation error before any subprocess");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("count")
            || msg.contains("/count")
            || msg.contains("integer")
            || msg.contains("type"),
        "stderr-style message must mention schema path or type; got: {msg}"
    );
}

const MCP_FIXTURE_YAML: &str = r#"
id: sample_mcp
description: map input to MCP tool args
input_schema:
  type: object
  required: [path]
  additionalProperties: false
  properties:
    path: { type: string }
    deep:
      type: object
      properties:
        n: { type: integer }
output_schema:
  type: object
executor:
  type: mcp
  tool: fixtures/echo
  arguments_from_input:
    path: /path
    n: /deep/n
"#;

#[test]
fn mcp_mapping_from_validated_input() {
    let input = json!({"path":"/tmp/x","deep":{"n": 42}});
    let (tool, args) = map_mcp_tool_arguments(MCP_FIXTURE_YAML, &input)
        .expect("validated input must map to a single MCP call payload");
    assert_eq!(tool, "fixtures/echo");
    assert_eq!(args, json!({"path":"/tmp/x","n":42}));
}

// --- Lower-level skeleton coverage (granular Red) ---

#[test]
fn discover_action_yaml_lists_files_under_actions_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let actions = dir.path().join("actions");
    std::fs::create_dir_all(&actions).expect("mkdir");
    std::fs::write(actions.join("a.yaml"), "id: a\n").expect("write");
    std::fs::write(actions.join("b.yaml"), "id: b\n").expect("write");
    let paths = discover_action_yaml_paths(&actions).expect("discovery must succeed");
    assert_eq!(paths.len(), 2, "one path per yaml file");
    let names: Vec<_> = paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"a.yaml".to_string()));
    assert!(names.contains(&"b.yaml".to_string()));
}

#[test]
fn validate_instance_against_schema_accepts_fixture_object() {
    let schema = json!({"type":"object","properties":{"k":{"type":"string"}}});
    let instance = json!({"k":"v"});
    validate_instance_against_schema(&instance, &schema).expect("fixture must validate");
}

#[test]
fn materialize_argv_resolves_literal_fragments() {
    let fragments = vec![json!({"literal":"one"}), json!({"literal":"two"})];
    let input = json!({});
    let argv = materialize_argv_from_templates(&fragments, &input).expect("literals only");
    assert_eq!(argv, vec!["one".to_string(), "two".to_string()]);
}
