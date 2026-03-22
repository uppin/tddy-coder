//! Acceptance tests for Session Lifecycle Redesign.
//!
//! These tests define the expected behavior for:
//! - changeset.yaml created before workflow starts
//! - state.session_id in changeset
//! - acceptance-tests creates fresh session (no plan resume)
//! - green resumes from state.session_id
//!
//! Tests are expected to FAIL until implementation is complete.

mod common;

use common::{
    ctx_acceptance_tests, ctx_green, plan_dir_for_input, temp_dir_with_git_repo,
    write_changeset_with_state,
};
use std::sync::Arc;
use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::output::{create_session_dir_in, sessions_base_path};
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{MockBackend, SharedBackend, WorkflowEngine};

const ACCEPTANCE_TESTS_OUTPUT: &str = r#"{"goal":"acceptance-tests","summary":"Created 1 test.","tests":[{"name":"login","file":"tests/auth.it.rs","line":1,"status":"failing"}]}"#;
const GREEN_OUTPUT: &str = r#"{"goal":"green","summary":"Implemented.","tests":[{"name":"auth_validates","file":"src/auth.rs","line":1,"status":"passing"}],"implementations":[{"name":"Auth","file":"src/auth.rs","line":1,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

/// AC1: ChangesetState has optional session_id field.
#[test]
fn changeset_state_has_session_id_field() {
    let dir = std::env::temp_dir().join("tddy-session-lifecycle-ac1");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let mut cs = Changeset::default();
    cs.state.session_id = Some("019ce2c2-e1e1-7141-a2e5-e0165407b553".to_string());
    write_changeset(&dir, &cs).expect("write");

    let read = read_changeset(&dir).expect("read");
    assert_eq!(
        read.state.session_id,
        Some("019ce2c2-e1e1-7141-a2e5-e0165407b553".to_string()),
        "ChangesetState must have session_id field"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// AC2: Changeset exists on disk before any agent invocation (early creation).
#[tokio::test]
async fn changeset_exists_before_workflow_starts() {
    let base = sessions_base_path().expect("sessions base");
    let session_dir = create_session_dir_in(&base).expect("create session dir");
    let plan_dir = session_dir;

    let init_cs = Changeset {
        initial_prompt: Some("Feature X".to_string()),
        ..Changeset::default()
    };
    write_changeset(&plan_dir, &init_cs).expect("write changeset before workflow");

    assert!(
        plan_dir.join("changeset.yaml").exists(),
        "changeset.yaml must exist before workflow starts"
    );
    let cs = read_changeset(&plan_dir).expect("read");
    assert_eq!(cs.state.current, "Init");
    assert_eq!(cs.initial_prompt.as_deref(), Some("Feature X"));

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// AC3: Acceptance-tests step creates fresh session (does not resume plan session).
/// Plan-mode sessions cannot be resumed; acceptance-tests must use a new session.
#[tokio::test]
async fn acceptance_tests_creates_fresh_session_no_crash() {
    let (output_dir, plan_dir) = temp_dir_with_git_repo("session-lifecycle-ac3", "Auth feature");

    std::fs::write(
        plan_dir.join("PRD.md"),
        "# Auth\n\n## Acceptance Tests\n- Login",
    )
    .expect("PRD");
    write_changeset_with_state(&plan_dir, "Planned", "plan-session-id-12345");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-session-lifecycle-ac3-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_acceptance_tests(plan_dir.clone(), Some(output_dir), None, false);
    let result = engine.run_goal("acceptance-tests", ctx).await;

    assert!(
        result.is_ok(),
        "acceptance-tests must not crash with 'No conversation found' — got {:?}",
        result
    );
    let result = result.unwrap();
    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "acceptance-tests must not return Error — got {:?}",
        result.status
    );

    let _ = std::fs::remove_dir_all(plan_dir.parent().unwrap());
}

/// AC4: Green step resumes from state.session_id (not from get_session_for_tag).
/// When state.session_id differs from sessions[tag=impl], green must use state.session_id.
#[tokio::test]
async fn green_resumes_from_state_session_id() {
    let output_dir = std::env::temp_dir().join("tddy-session-lifecycle-ac4");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create");

    let plan_dir = plan_dir_for_input(&output_dir, "Auth");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    std::fs::write(plan_dir.join("PRD.md"), "# Auth").expect("PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Tests\n- login").expect("AT");
    std::fs::write(plan_dir.join("progress.md"), "# Progress\n- red done").expect("progress");

    let mut cs = Changeset::default();
    cs.state.current = "RedTestsReady".to_string();
    cs.state.session_id = Some("correct-impl-from-state".to_string());
    cs.sessions.push(tddy_core::changeset::SessionEntry {
        id: "wrong-impl-from-tag-lookup".to_string(),
        agent: "claude".to_string(),
        tag: "impl".to_string(),
        created_at: "2026-03-12T10:00:00Z".to_string(),
        system_prompt_file: None,
    });
    write_changeset(&plan_dir, &cs).expect("write");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(GREEN_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-session-lifecycle-ac4-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_green(plan_dir.clone(), None, false);
    let result = engine.run_goal("green", ctx).await;

    assert!(result.is_ok(), "green must succeed — got {:?}", result);
    let result = result.unwrap();
    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "green must not return Error — got {:?}",
        result.status
    );
    let inv = backend.invocations();
    assert!(
        !inv.is_empty(),
        "green must invoke backend with session_id from state"
    );
    let req = &inv[0];
    assert_eq!(
        req.session.as_ref().map(|s| s.session_id()),
        Some("correct-impl-from-state"),
        "green must use state.session_id, not get_session_for_tag(impl)"
    );
    assert!(
        req.session.as_ref().is_some_and(|s| s.is_resume()),
        "green must use SessionMode::Resume when resuming"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}
