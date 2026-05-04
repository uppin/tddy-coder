//! Acceptance-tests goal must materialize session action manifests (`actions/*.yaml`) and support
//! `invoke-action`-style records per PRD Testing Plan (`acceptance_tests_hook_materializes_default_action_manifests`).

mod common;

use std::fs;
use std::sync::Arc;

use serde_json::json;
use tddy_core::session_actions::{parse_action_manifest_yaml, run_manifest_command};
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};

use common::{ctx_acceptance_tests, temp_dir_with_git_repo, write_changeset_for_session};

const ACCEPTANCE_TESTS_JSON_OUTPUT: &str = r#"{"goal":"acceptance-tests","summary":"Created 3 acceptance tests. All failing (Red state).","tests":[{"name":"acceptance_tests_prompt_requires_three_session_actions","file":"packages/tddy-workflow-recipes/src/tdd/acceptance_tests.rs","line":98,"status":"failing"},{"name":"acceptance_tests_hook_materializes_default_action_manifests","file":"packages/tddy-integration-tests/tests/workflow_recipe_acceptance_actions.rs","line":19,"status":"failing"},{"name":"invoke_action_round_trip_for_fixture_manifests","file":"packages/tddy-tools/tests/actions_cli_acceptance.rs","line":409,"status":"failing"}]}"#;

#[tokio::test]
async fn acceptance_tests_hook_materializes_default_action_manifests() {
    let (output_dir, session_dir) = temp_dir_with_git_repo("at-session-actions-hook");
    fs::write(
        session_dir.join("PRD.md"),
        "# PRD\n## Testing Plan\n- acceptance_tests_prompt_requires_three_session_actions\n",
    )
    .expect("write PRD");
    write_changeset_for_session(&session_dir, "sess-at-actions");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_JSON_OUTPUT);

    let storage_dir =
        std::env::temp_dir().join(format!("tddy-at-actions-engine-{}", std::process::id()));
    let _ = fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let context = ctx_acceptance_tests(session_dir.clone(), Some(output_dir), None, false);
    let result = engine
        .run_goal(&GoalId::new("acceptance-tests"), context)
        .await
        .expect("engine run");

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "acceptance-tests should succeed: {:?}",
        result.status
    );

    let actions_dir = session_dir.join("actions");
    let yaml_count = fs::read_dir(&actions_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|ext| ext == "yaml" || ext == "yml")
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    assert!(
        yaml_count >= 3,
        "RED: acceptance-tests must materialize ≥3 session action manifests under actions/; found {}",
        yaml_count
    );

    let yaml_paths: Vec<std::path::PathBuf> = fs::read_dir(&actions_dir)
        .expect("read actions dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext == "yaml" || ext == "yml")
                .unwrap_or(false)
        })
        .collect();

    for path in &yaml_paths {
        let text = fs::read_to_string(path).expect("read manifest");
        parse_action_manifest_yaml(&text).unwrap_or_else(|e| {
            panic!(
                "round-trip ActionManifest parse for {}: {e}",
                path.display()
            )
        });
    }

    let manifest_path = yaml_paths
        .first()
        .expect("at least one yaml path when yaml_count >= 3");
    let manifest =
        tddy_core::session_actions::parse_action_manifest_file(manifest_path).expect("parse");
    let record =
        run_manifest_command(&session_dir, None, &manifest, &json!({})).expect("invoke-style run");
    assert!(
        record.get("exit_code").and_then(|c| c.as_i64()).is_some(),
        "structured record must include numeric exit_code: {record}"
    );

    let _ = fs::remove_dir_all(session_dir.parent().unwrap());
}
