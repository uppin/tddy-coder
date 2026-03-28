//! Acceptance tests: workflow artifact layout and recipe-driven paths (PRD Testing Plan).
//!
//! `artifacts_subdir_used_for_new_sessions`, `approval_content_matches_recipe_primary_artifact`,
//! `context_header_uses_recipe_artifact_basenames` — these encode intended behavior after
//! tddy-core is decoupled from fixed PRD paths; they should fail until implementation lands.

mod common;

use std::sync::Arc;

use tddy_core::workflow::build_context_header;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine, WorkflowRecipe};
use tddy_workflow::{
    read_primary_planning_document_utf8, resolve_existing_primary_planning_document,
};
use tddy_workflow_recipes::TddRecipe;

/// Plan output as JSON (tddy-tools submit format).
const PLANNING_OUTPUT: &str = r##"{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]}}"##;

/// New sessions using the default TDD recipe must write primary planning artifacts under
/// `session_dir/artifacts/`, not the session directory root.
#[tokio::test]
async fn artifacts_subdir_used_for_new_sessions() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLANNING_OUTPUT);

    let base = std::env::temp_dir().join(format!("tddy-artifacts-subdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-artifacts-subdir-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let storage_dir_for_cleanup = storage_dir.clone();
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let mut ctx = std::collections::HashMap::new();
    ctx.insert("feature_input".to_string(), serde_json::json!("Build auth"));
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );

    let result = engine
        .run_goal(&GoalId::new("plan"), ctx)
        .await
        .expect("plan goal should not panic");

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "plan goal should succeed when session_base is provided: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .expect("get_session should work")
        .expect("session should exist");
    let session_dir: std::path::PathBuf = session
        .context
        .get_sync("session_dir")
        .expect("session_dir in context after plan");

    let artifacts_root = session_dir.join("artifacts");
    let prd_under_artifacts = artifacts_root.join("PRD.md");
    assert!(
        prd_under_artifacts.is_file(),
        "default recipe must write PRD.md under session_dir/artifacts/; expected {}",
        prd_under_artifacts.display()
    );

    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&storage_dir_for_cleanup);
}

/// Elicitation and plan approval must load the same bytes as the recipe-defined primary artifact
/// on disk (under `artifacts/` for the default TDD recipe), not a legacy flat `PRD.md` path.
#[test]
fn approval_content_matches_recipe_primary_artifact() {
    let base = std::env::temp_dir().join(format!("tddy-approval-primary-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("artifacts")).unwrap();

    let recipe = TddRecipe;
    let basename = recipe
        .default_artifacts()
        .get("prd")
        .expect("TddRecipe maps prd output key")
        .clone();
    let primary_path = base.join("artifacts").join(&basename);
    std::fs::write(&primary_path, "PRIMARY_PLAN_BODY_FROM_RECIPE_PATH").unwrap();

    let resolved = resolve_existing_primary_planning_document(&base, &basename)
        .expect("resolver must find primary planning document for approval / elicitation");
    let resolved_bytes = read_primary_planning_document_utf8(&base, &basename)
        .expect("read helper must load primary planning document bytes");
    let recipe_bytes = std::fs::read_to_string(&primary_path).unwrap();

    assert_eq!(
        resolved,
        primary_path,
        "approval path must be the recipe primary artifact at {}",
        primary_path.display()
    );
    assert_eq!(
        resolved_bytes,
        recipe_bytes,
        "plan approval / elicitation must read the recipe primary artifact at {}",
        primary_path.display()
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Context header paths must be resolved from the recipe manifest (artifact basenames under the
/// session `artifacts/` root), not only from hard-coded filenames at the session root.
#[test]
fn context_header_uses_recipe_artifact_basenames() {
    let recipe = TddRecipe;
    let basenames: Vec<&str> = recipe.context_header_session_artifact_filenames();

    let dir = std::env::temp_dir().join(format!("tddy-ctx-header-recipe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("artifacts")).unwrap();
    std::fs::write(dir.join("artifacts").join("PRD.md"), "# PRD").unwrap();

    let header = build_context_header(Some(&dir), None, &basenames);
    assert!(
        header.contains("PRD.md:"),
        "build_context_header must list PRD.md when present under session_dir/artifacts/ per recipe basenames; got: {:?}",
        header
    );
    let prd_line = header
        .lines()
        .find(|l| l.starts_with("PRD.md:"))
        .expect("PRD.md line");
    let path_part = prd_line.trim_start_matches("PRD.md:").trim();
    assert!(
        path_part.contains("/artifacts/") || path_part.contains("\\artifacts\\"),
        "listed PRD path must be under session artifacts/; got: {}",
        path_part
    );

    let _ = std::fs::remove_dir_all(&dir);
}
