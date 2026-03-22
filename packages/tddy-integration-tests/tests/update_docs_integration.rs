//! Integration tests for the update-docs goal.
//!
//! Mirrors refactor_integration.rs pattern.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::{CodingBackend, CursorBackend, GoalId, MockBackend, SharedBackend, WorkflowEngine};

use common::{ctx_update_docs, run_goal_until_done, write_changeset_with_state};

/// Minimal update-docs output as JSON (tddy-tools submit format).
const UPDATE_DOCS_OUTPUT: &str =
    r#"{"goal":"update-docs","summary":"Updated 3 documentation files.","docs_updated":3}"#;

fn write_minimal_artifacts(session_dir: &std::path::Path) {
    std::fs::write(
        session_dir.join("PRD.md"),
        "# PRD\n\nStub PRD for update-docs test.\n",
    )
    .expect("write PRD.md");
}

/// update-docs() invokes backend with goal_id "update-docs".
#[tokio::test]
async fn update_docs_invokes_backend_with_update_docs_goal() {
    let session_dir = std::env::temp_dir().join("tddy-update-docs-goal-test");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_minimal_artifacts(&session_dir);
    write_changeset_with_state(&session_dir, "RefactorComplete", "sess-update-docs-goal");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-update-docs-goal-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_update_docs(session_dir.clone());
    let result = run_goal_until_done(&engine, "update-docs", ctx).await;

    assert!(
        result.is_ok(),
        "update-docs should succeed, got: {:?}",
        result
    );

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal_id,
        GoalId::new("update-docs"),
        "InvokeRequest must have goal_id update-docs"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// update-docs() transitions workflow to DocsUpdated state on success.
#[tokio::test]
async fn update_docs_transitions_to_docs_updated() {
    let session_dir = std::env::temp_dir().join("tddy-update-docs-state-test");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_minimal_artifacts(&session_dir);
    write_changeset_with_state(&session_dir, "RefactorComplete", "sess-refactor-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-update-docs-state-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_update_docs(session_dir.clone());
    let _ = run_goal_until_done(&engine, "update-docs", ctx)
        .await
        .unwrap();

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert_eq!(
        changeset.state.current,
        WorkflowState::new("DocsUpdated"),
        "workflow should transition to DocsUpdated, got {}",
        changeset.state.current
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// update-docs() parses structured response with summary and docs_updated.
#[tokio::test]
async fn update_docs_parses_structured_response() {
    let session_dir = std::env::temp_dir().join("tddy-update-docs-parse-test");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    write_minimal_artifacts(&session_dir);
    write_changeset_with_state(&session_dir, "RefactorComplete", "sess-update-docs-parse");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(UPDATE_DOCS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-update-docs-parse-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_update_docs(session_dir.clone());
    let result = run_goal_until_done(&engine, "update-docs", ctx)
        .await
        .unwrap();

    let session = engine
        .get_session(&result.session_id)
        .await
        .unwrap()
        .unwrap();
    let output_str: String = session.context.get_sync("output").unwrap();
    let output =
        tddy_workflow_recipes::parse_update_docs_response(&output_str).expect("parse output");

    assert!(!output.summary.is_empty(), "summary must not be empty");
    assert_eq!(output.docs_updated, 3, "docs_updated should be 3");

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// CursorBackend must accept update-docs goal (same `agent` invocation path as other goals).
#[tokio::test]
async fn cursor_backend_accepts_update_docs() {
    let backend = CursorBackend::with_path(std::path::PathBuf::from("/nonexistent/agent"));
    let req = common::stub_invoke_request("update-docs", "update-docs");

    let result = backend.invoke(req).await;

    // CursorBackend does NOT reject UpdateDocs. It may return BinaryNotFound
    // (agent CLI not installed) or InvocationFailed for other reasons, but must
    // NOT return InvocationFailed("update-docs is not supported").
    if let Err(tddy_core::BackendError::InvocationFailed(ref msg)) = result {
        assert!(
            !msg.to_lowercase().contains("update-docs")
                || !msg.to_lowercase().contains("not supported"),
            "CursorBackend must accept update-docs goal, got: {}",
            msg
        );
    }
}
