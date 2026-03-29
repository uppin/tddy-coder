//! Contract: when the agent completes a green turn without a relayed `tddy-tools submit`, the
//! workflow must re-invoke the backend with the validation/error text in the prompt so the agent
//! can run `tddy-tools submit` with a corrected JSON payload and the goal can succeed.

mod common;

use std::sync::Arc;

use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};

const GREEN_OUTPUT_ALL_PASS: &str = r#"{"goal":"green","summary":"Implemented 2 methods. All 3 unit tests and 2 acceptance tests passing.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"passing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

fn setup_session_ready_for_green(session_dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(session_dir);
    std::fs::create_dir_all(session_dir).expect("create plan dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- auth_service_validates_email",
    )
    .expect("write acceptance-tests.md");
    std::fs::write(
        session_dir.join("progress.md"),
        "# Progress\n## From red\n- [ ] auth_service_validates_email (failing)\n",
    )
    .expect("write progress.md");
    common::write_changeset_with_state(session_dir, "RedTestsReady", "persisted-green-retry-sess");
}

#[tokio::test]
async fn green_missing_submit_is_followed_by_retry_invoke_carrying_error_and_successful_submit() {
    let session_dir = std::env::temp_dir().join(format!(
        "tddy-green-submit-remediation-{}",
        std::process::id()
    ));
    setup_session_ready_for_green(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok_without_submit("Implemented code but did not call tddy-tools submit.");
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-green-submit-remediation-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx_green = common::ctx_green(session_dir.clone(), None, false);
    let outcome = engine.run_goal(&GoalId::new("green"), ctx_green).await;

    let green_invokes: Vec<_> = backend
        .invocations()
        .into_iter()
        .filter(|r| r.goal_id.as_str() == "green")
        .collect();

    assert!(
        green_invokes.len() >= 2,
        "expected at least two green backend invokes (retry after missing submit); got {}",
        green_invokes.len()
    );

    assert!(
        green_invokes[1]
            .prompt
            .contains("without calling tddy-tools submit"),
        "retry prompt must include the missing-submit error for the agent; second prompt={}",
        green_invokes[1].prompt
    );

    let result = outcome.expect("green should complete after corrective submit");
    assert!(
        matches!(result.status, ExecutionStatus::Paused { .. }),
        "green should pause after success; got {:?}",
        result.status
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}
