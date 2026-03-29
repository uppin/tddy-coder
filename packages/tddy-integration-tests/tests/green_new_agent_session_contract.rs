//! Contract: when `new_agent_session` is set in context, the green step must NOT resume the
//! previous agent session. Instead it should invoke the backend with `session: None` so a
//! brand-new agent thread is created while the workflow state machine continues from the
//! correct state (GreenImplementing).
//!
//! This addresses the failure mode where a resumed agent session keeps repeating the same
//! mistake (e.g. stripping TDDY_SOCKET) because it retains learned behavior from prior turns.

mod common;

use std::sync::Arc;

use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};

const GREEN_OUTPUT_ALL_PASS: &str = r#"{"goal":"green","summary":"All tests passing.","tests":[{"name":"auth_validates_email","file":"src/service.rs","line":42,"status":"passing"}],"implementations":[{"name":"AuthService","file":"src/service.rs","line":10,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

fn setup_session_ready_for_green(session_dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(session_dir);
    std::fs::create_dir_all(session_dir).expect("create session dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- auth_validates_email",
    )
    .expect("write acceptance-tests.md");
    std::fs::write(
        session_dir.join("progress.md"),
        "# Progress\n## From red\n- [ ] auth_validates_email (failing)\n",
    )
    .expect("write progress.md");
    common::write_changeset_with_state(
        session_dir,
        "RedTestsReady",
        "old-agent-session-id-to-not-resume",
    );
}

#[tokio::test]
async fn green_with_new_agent_session_does_not_resume_old_session() {
    let session_dir = std::env::temp_dir().join(format!(
        "tddy-green-new-agent-session-{}",
        std::process::id()
    ));
    setup_session_ready_for_green(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-green-new-agent-session-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let mut ctx = common::ctx_green(session_dir.clone(), None, false);
    ctx.insert(
        "new_agent_session".to_string(),
        serde_json::json!(true),
    );

    let outcome = engine
        .run_goal(&GoalId::new("green"), ctx)
        .await
        .expect("green should complete");

    assert!(
        matches!(outcome.status, ExecutionStatus::Paused { .. }),
        "green should pause after success; got {:?}",
        outcome.status
    );

    let green_invokes: Vec<_> = backend
        .invocations()
        .into_iter()
        .filter(|r| r.goal_id.as_str() == "green")
        .collect();
    assert_eq!(green_invokes.len(), 1, "expected exactly one green invoke");

    let session_mode = &green_invokes[0].session;
    assert!(
        session_mode.is_none(),
        "with new_agent_session, backend must receive session: None (no resume); got {:?}",
        session_mode
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_without_new_agent_session_resumes_existing_session() {
    let session_dir = std::env::temp_dir().join(format!(
        "tddy-green-resume-existing-{}",
        std::process::id()
    ));
    setup_session_ready_for_green(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-green-resume-existing-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = common::ctx_green(session_dir.clone(), None, false);
    let outcome = engine
        .run_goal(&GoalId::new("green"), ctx)
        .await
        .expect("green should complete");

    assert!(
        matches!(outcome.status, ExecutionStatus::Paused { .. }),
        "green should pause; got {:?}",
        outcome.status
    );

    let green_invokes: Vec<_> = backend
        .invocations()
        .into_iter()
        .filter(|r| r.goal_id.as_str() == "green")
        .collect();
    assert_eq!(green_invokes.len(), 1);

    let session_mode = green_invokes[0]
        .session
        .as_ref()
        .expect("without new_agent_session, session must be Some");
    assert!(
        session_mode.is_resume(),
        "default green should resume; got {:?}",
        session_mode
    );
    assert_eq!(
        session_mode.session_id(),
        "old-agent-session-id-to-not-resume",
        "should resume the persisted session id from changeset"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

#[tokio::test]
async fn green_new_agent_session_remediation_retries_also_start_fresh() {
    let session_dir = std::env::temp_dir().join(format!(
        "tddy-green-new-session-remediation-{}",
        std::process::id()
    ));
    setup_session_ready_for_green(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok_without_submit("Implemented but forgot to call tddy-tools submit.");
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-green-new-session-remediation-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let mut ctx = common::ctx_green(session_dir.clone(), None, false);
    ctx.insert(
        "new_agent_session".to_string(),
        serde_json::json!(true),
    );

    let outcome = engine
        .run_goal(&GoalId::new("green"), ctx)
        .await
        .expect("green should complete after remediation retry");

    assert!(
        matches!(outcome.status, ExecutionStatus::Paused { .. }),
        "green should pause after success; got {:?}",
        outcome.status
    );

    let green_invokes: Vec<_> = backend
        .invocations()
        .into_iter()
        .filter(|r| r.goal_id.as_str() == "green")
        .collect();

    assert!(
        green_invokes.len() >= 2,
        "expected at least two green invokes (retry after missing submit); got {}",
        green_invokes.len()
    );

    for (i, invoke) in green_invokes.iter().enumerate() {
        assert!(
            invoke.session.is_none(),
            "invoke {} with new_agent_session must have session: None; got {:?}",
            i,
            invoke.session
        );
    }

    let _ = std::fs::remove_dir_all(&session_dir);
}
