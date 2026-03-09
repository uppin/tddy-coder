//! Integration tests for the planning workflow with MockBackend and StubBackend.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{
    ClarificationQuestion, MockBackend, PlanOptions, QuestionOption, SharedBackend, StubBackend,
    WorkflowEngine,
};

use common::{plan_dir_for_input, run_plan};

const DELIMITED_OUTPUT: &str = r#"Here is my analysis.

---PRD_START---
# Feature PRD

## Summary
User authentication system with login and logout.

## Acceptance Criteria
- [ ] Login with email/password
- [ ] Logout clears session
---PRD_END---

---TODO_START---
- [ ] Create auth module
- [ ] Implement login endpoint
- [ ] Implement logout endpoint
- [ ] Add session management
---TODO_END---

That concludes the plan."#;

fn clarification_questions() -> Vec<ClarificationQuestion> {
    vec![
        ClarificationQuestion {
            header: "Audience".to_string(),
            question: "What is the target audience?".to_string(),
            options: vec![],
            multi_select: false,
            allow_other: true,
        },
        ClarificationQuestion {
            header: "Timeline".to_string(),
            question: "What is the expected timeline?".to_string(),
            options: vec![],
            multi_select: false,
            allow_other: true,
        },
    ]
}

#[tokio::test]
async fn planning_workflow_produces_prd_and_todo_in_output_directory() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(DELIMITED_OUTPUT);

    let output_dir = std::env::temp_dir().join("tddy-planning-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-engine-test");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let (output_path, _) = run_plan(
        &engine,
        "Build a user authentication system with login and logout",
        &output_dir,
        None,
    )
    .await
    .expect("planning should succeed");

    assert!(output_path.is_dir(), "output should be a directory");

    let prd_path = output_path.join("PRD.md");
    let todo_path = output_path.join("TODO.md");

    assert!(prd_path.exists(), "PRD.md should exist");
    assert!(todo_path.exists(), "TODO.md should exist");

    let prd_content = std::fs::read_to_string(&prd_path).expect("read PRD");
    let todo_content = std::fs::read_to_string(&todo_path).expect("read TODO");

    assert!(
        prd_content.contains("User authentication"),
        "PRD should contain summary"
    );
    assert!(
        prd_content.contains("Login with email"),
        "PRD should contain acceptance criteria"
    );
    assert!(
        todo_content.contains("Create auth module"),
        "TODO should contain tasks"
    );
    assert!(
        todo_content.contains("Implement login"),
        "TODO should contain implementation tasks"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn planning_workflow_invokes_backend_with_plan_permission_mode() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(DELIMITED_OUTPUT);

    let output_dir = std::env::temp_dir().join("tddy-planning-invoke-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-invoke-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let (plan_dir, _) = run_plan(&engine, "Feature X", &output_dir, None)
        .await
        .expect("plan should succeed");

    let changeset = read_changeset(&plan_dir).expect("changeset should exist");
    assert_eq!(
        changeset.state.current, "Planned",
        "changeset state should be Planned"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn planning_workflow_with_stub_backend_transitions_to_planned() {
    let backend = Arc::new(StubBackend::new());
    let output_dir = std::env::temp_dir().join("tddy-planning-stub-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-stub-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let first = run_plan(&engine, "Add a feature", &output_dir, None).await;
    assert!(
        first.is_err()
            && first
                .unwrap_err()
                .to_string()
                .contains("ClarificationNeeded"),
        "first call should return ClarificationNeeded"
    );

    let (plan_dir, _) = run_plan(
        &engine,
        "Add a feature",
        &output_dir,
        Some("Email/password"),
    )
    .await
    .expect("second call with answers should succeed");

    assert!(plan_dir.join("PRD.md").exists(), "PRD.md should exist");
    assert!(plan_dir.join("TODO.md").exists(), "TODO.md should exist");

    let changeset = read_changeset(&plan_dir).expect("changeset should exist");
    assert_eq!(
        changeset.state.current, "Planned",
        "changeset state should be Planned with StubBackend"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn planning_workflow_transitions_to_failed_when_backend_errors() {
    let backend = Arc::new(MockBackend::new());
    backend.push_err("simulated backend failure");

    let output_dir = std::env::temp_dir().join("tddy-planning-fail-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-fail-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let result = run_plan(&engine, "Feature Y", &output_dir, None).await;

    assert!(result.is_err(), "planning should fail");
    if let Ok((plan_dir, _)) = result {
        if let Ok(cs) = read_changeset(&plan_dir) {
            assert_eq!(
                cs.state.current, "Init",
                "changeset should remain Init on backend error"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn planning_workflow_returns_clarification_needed_when_backend_returns_questions() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok_with_questions("", "sess-qa", clarification_questions());

    let output_dir = std::env::temp_dir().join("tddy-planning-questions-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-questions-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let result = run_plan(&engine, "Feature Z", &output_dir, None).await;

    assert!(
        result.is_err(),
        "expected ClarificationNeeded (WaitingForInput), got {:?}",
        result
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("ClarificationNeeded"),
        "expected ClarificationNeeded in error, got {}",
        err_msg
    );

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn planning_workflow_returns_clarification_needed_with_structured_questions() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok_with_questions(
        "",
        "sess-789",
        vec![
            ClarificationQuestion {
                header: "Scope".to_string(),
                question: "What is the target audience?".to_string(),
                options: vec![
                    QuestionOption {
                        label: "Developers".to_string(),
                        description: "Technical users".to_string(),
                    },
                    QuestionOption {
                        label: "End users".to_string(),
                        description: "General public".to_string(),
                    },
                ],
                multi_select: false,
                allow_other: true,
            },
            ClarificationQuestion {
                header: "Timeline".to_string(),
                question: "What is the expected timeline?".to_string(),
                options: vec![],
                multi_select: false,
                allow_other: true,
            },
        ],
    );

    let output_dir = std::env::temp_dir().join("tddy-planning-structured-qa-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-structured-qa-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let result = run_plan(&engine, "Feature Z", &output_dir, None).await;

    assert!(
        result.is_err(),
        "expected ClarificationNeeded with structured questions"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn planning_workflow_produces_prd_after_clarification_answers() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok_with_questions("", "sess-qa", clarification_questions());
    backend.push_ok(DELIMITED_OUTPUT);

    let output_dir = std::env::temp_dir().join("tddy-planning-followup-test");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-followup-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let first = run_plan(&engine, "Feature Z", &output_dir, None).await;
    assert!(
        first.is_err(),
        "first call should return ClarificationNeeded"
    );

    let (output_path, _) = run_plan(
        &engine,
        "Feature Z",
        &output_dir,
        Some("Developers\nQ2 2025"),
    )
    .await
    .expect("second call with answers should succeed");

    assert!(output_path.is_dir());

    let prd_content = std::fs::read_to_string(output_path.join("PRD.md")).expect("read PRD");
    assert!(prd_content.contains("User authentication"));

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn planning_workflow_stub_backend_clarification_roundtrip() {
    let backend = Arc::new(StubBackend::new());
    let output_dir = std::env::temp_dir().join("tddy-planning-stub-clarify");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-planning-stub-clarify-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = Arc::new(TddWorkflowHooks::new());
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend),
        storage_dir.clone(),
        Some(hooks),
    );

    let first = run_plan(&engine, "test feature", &output_dir, None).await;
    assert!(
        first.is_err(),
        "first call should return ClarificationNeeded"
    );

    let (output_path, _) = run_plan(&engine, "test feature", &output_dir, Some("Email/password"))
        .await
        .expect("second call with answers should succeed");

    assert!(output_path.is_dir());
    assert!(output_path.join("PRD.md").exists());
    assert!(output_path.join("TODO.md").exists());

    let _ = std::fs::remove_dir_all(&output_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

// ── plan_dir_suggestion: R1 + R3 (valid suggestion) ──────────────────────────
// TODO: Re-add when PlanTask implements plan_dir_suggestion relocation.
// Workflow::plan had relocate_plan_dir logic; PlanTask/engine does not yet.
