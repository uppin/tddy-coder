//! PRD acceptance: branch/worktree intent persistence via `changeset-workflow` + `persist-changeset-workflow`.
//!
//! RED: extended workflow fields must survive validation, CLI persist, and YAML round-trip.

use std::fs;
use std::process::Command;

use tddy_core::changeset::{merge_persisted_workflow_into_context, write_changeset, Changeset};
use tddy_core::workflow::context::Context;
use tddy_testing_commons::fs::temp_session_dir;
use tddy_tools::schema::validate_output;

// Note: do not assert `validate_output(...).is_ok()` on the full intent payload here — until the
// schema lists intent fields, jsonschema may still accept unknown properties; the round-trip and
// schema-file tests are the RED guardrails.

/// Canonical extended payload (PRD): intent + refs + naming; must validate and round-trip.
const BRANCH_INTENT_WORKFLOW_JSON: &str = r#"{
  "run_optional_step_x": false,
  "demo_options": ["unchanged-fixture"],
  "tool_schema_id": "urn:tddy:tool/changeset-workflow",
  "branch_worktree_intent": "new_branch_from_base",
  "selected_integration_base_ref": "origin/main",
  "new_branch_name": "feature/intent-from-acceptance"
}"#;

#[test]
fn persist_changeset_workflow_accepts_branch_intent_and_round_trips() {
    // Given
    let dir = temp_session_dir("persist");
    write_changeset(&dir, &Changeset::default()).expect("seed changeset");

    // When
    let status = Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .args([
            "persist-changeset-workflow",
            "--session-dir",
            dir.to_str().expect("utf8 path"),
            "--data",
            BRANCH_INTENT_WORKFLOW_JSON,
        ])
        .status()
        .expect("spawn tddy-tools");

    // Then
    assert!(
        status.success(),
        "persist-changeset-workflow must exit 0 for extended workflow JSON; got {:?}",
        status.code()
    );

    let raw = fs::read_to_string(dir.join("changeset.yaml")).expect("read changeset.yaml");
    assert!(
        raw.contains("branch_worktree_intent"),
        "changeset.yaml must contain 'branch_worktree_intent'; got:\n{raw}"
    );
    assert!(
        raw.contains("new_branch_from_base"),
        "changeset.yaml must contain 'new_branch_from_base'; got:\n{raw}"
    );
    assert!(
        raw.contains("selected_integration_base_ref"),
        "changeset.yaml must contain 'selected_integration_base_ref'; got:\n{raw}"
    );
    assert!(
        raw.contains("new_branch_name"),
        "changeset.yaml must contain 'new_branch_name'; got:\n{raw}"
    );

    let ctx = Context::new();
    merge_persisted_workflow_into_context(&dir, &ctx).expect("merge workflow into context");
    assert!(
        ctx.get_sync::<String>("branch_worktree_intent").is_some(),
        "resume/hooks must read branch_worktree_intent from Context after merge"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn changeset_workflow_schema_json_includes_branch_intent_contract() {
    // Given
    const SCHEMA: &str =
        include_str!("../../tddy-workflow-recipes/generated/tdd/changeset-workflow.schema.json");

    // Then
    assert!(
        SCHEMA.contains("branch_worktree_intent"),
        "embedded changeset-workflow.schema.json must define branch_worktree_intent (regenerate from PRD)"
    );
    assert!(
        SCHEMA.contains("selected_integration_base_ref"),
        "schema must include selected_integration_base_ref for routing"
    );
}

#[test]
fn invalid_branch_worktree_intent_value_fails_validation() {
    // Given
    let bad = r#"{
      "run_optional_step_x": false,
      "demo_options": [],
      "tool_schema_id": "urn:tddy:tool/changeset-workflow",
      "branch_worktree_intent": "__not_a_valid_intent__"
    }"#;

    // When
    let result = validate_output("changeset-workflow", bad);

    // Then
    assert!(
        result.is_err(),
        "schema must reject unknown branch_worktree_intent enum values"
    );
}
