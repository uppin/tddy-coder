//! Contract: newly generated workflow and agent session identifiers must be UUID v7 (sortable,
//! consistent with daemon StartSession and `tddy_core::output::create_session_dir_in`).

mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::output::create_session_dir_in;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};
use tddy_workflow_recipes::TddWorkflowHooks;

use common::{tdd_manifest, tdd_recipe};

/// Plan output as JSON (tddy-tools submit format).
const PLANNING_OUTPUT: &str = r##"{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]}}"##;

/// RFC 4122 hyphenated form: version is the first hex digit of the third group (index 14).
fn assert_uuid_v7(label: &str, s: &str) {
    let normalized = s.to_ascii_lowercase();
    assert_eq!(
        normalized.len(),
        36,
        "{label}: expected hyphenated UUID (36 chars), got len {} for {s:?}",
        normalized.len()
    );
    assert_eq!(
        normalized.as_bytes().get(14).copied(),
        Some(b'7'),
        "{label}: expected UUID v7 (version digit '7' at index 14), got {s:?}"
    );
    uuid::Uuid::parse_str(&normalized).unwrap_or_else(|e| {
        panic!("{label}: value must parse as UUID, got {s:?}: {e}");
    });
}

/// Engine `session_id` must match the pre-created `{session_base}/sessions/{id}/` tree (UUID v7 dirname).
#[tokio::test]
async fn workflow_engine_session_id_is_uuid_v7_with_precreated_session_tree() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLANNING_OUTPUT);

    let base = std::env::temp_dir().join(format!("tddy-v7-engine-sid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let pre_session = create_session_dir_in(&base).expect("pre-create session dir");
    let sid = pre_session
        .file_name()
        .and_then(|n| n.to_str())
        .expect("session dirname")
        .to_string();

    let storage_dir =
        std::env::temp_dir().join(format!("tddy-v7-engine-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(tdd_recipe().create_hooks(None)),
    );

    let mut ctx = HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("feature for v7 contract"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert("session_id".to_string(), serde_json::json!(sid));

    let result = engine
        .run_goal(&GoalId::new("plan"), ctx)
        .await
        .expect("plan goal");

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "plan should succeed: {:?}",
        result.status
    );

    assert_uuid_v7("WorkflowEngine result.session_id", &result.session_id);

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn plan_task_allocated_session_dir_basename_is_uuid_v7() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLANNING_OUTPUT);

    let base = std::env::temp_dir().join(format!("tddy-v7-plan-dirname-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let pre_session = create_session_dir_in(&base).expect("pre-create session dir");
    let sid = pre_session
        .file_name()
        .and_then(|n| n.to_str())
        .expect("session dirname")
        .to_string();

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-v7-plan-dirname-storage-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(tdd_recipe().create_hooks(None)),
    );

    let mut ctx = HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("feature for v7 dir"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert("session_id".to_string(), serde_json::json!(sid));

    let result = engine
        .run_goal(&GoalId::new("plan"), ctx)
        .await
        .expect("plan goal");

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "plan should succeed: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .expect("get_session")
        .expect("session exists");
    let session_dir: PathBuf = session
        .context
        .get_sync("session_dir")
        .expect("session_dir in context");
    let basename = session_dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("session_dir basename");

    assert_uuid_v7("pre-created session_dir dirname (entry layer)", basename);

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn before_acceptance_tests_hook_sets_uuid_v7_session_id_in_context() {
    let output_dir =
        std::env::temp_dir().join(format!("tddy-v7-at-hook-out-{}", std::process::id()));
    let session_dir = output_dir.join("session");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    common::write_changeset_with_state(&session_dir, "Planned", "sess-at-v7");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Summary\nx").expect("PRD");

    let hooks = TddWorkflowHooks::new(tdd_recipe(), tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("output_dir", output_dir.clone());
    ctx.set_sync("backend_name", "stub".to_string());

    hooks
        .before_task("acceptance-tests", &ctx)
        .expect("before_task acceptance-tests");

    let sid: String = ctx
        .get_sync("session_id")
        .expect("before_acceptance_tests must set session_id in context");

    assert_uuid_v7("before_acceptance_tests context.session_id", &sid);

    let _ = std::fs::remove_dir_all(&output_dir);
}

#[tokio::test]
async fn before_red_hook_sets_uuid_v7_session_id_in_context() {
    let session_dir = std::env::temp_dir().join(format!("tddy-v7-red-hook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("mkdir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan\n").expect("PRD");
    std::fs::write(session_dir.join("acceptance-tests.md"), "# AT\n- t1\n")
        .expect("acceptance-tests.md");
    common::write_changeset_with_state(&session_dir, "AcceptanceTestsReady", "sess-red-v7");

    let hooks = TddWorkflowHooks::new(tdd_recipe(), tdd_manifest());
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("output_dir", session_dir.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks.before_task("red", &ctx).expect("before_task red");

    let sid: String = ctx
        .get_sync("session_id")
        .expect("before_red must set session_id in context");

    assert_uuid_v7("before_red context.session_id", &sid);

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[test]
fn recipes_create_session_dir_in_uses_uuid_v7_dirname() {
    let base = std::env::temp_dir().join(format!("tddy-v7-recipes-writer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let session_dir =
        tddy_workflow_recipes::create_session_dir_in(&base).expect("create_session_dir_in");
    let name = session_dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("dirname");

    assert_uuid_v7("tddy_workflow_recipes::create_session_dir_in dirname", name);

    let _ = std::fs::remove_dir_all(&base);
}
