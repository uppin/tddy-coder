//! Integration tests for the planning workflow with MockBackend.

use tddy_core::{ClarificationQuestion, MockBackend, QuestionOption, Workflow};

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
        },
        ClarificationQuestion {
            header: "Timeline".to_string(),
            question: "What is the expected timeline?".to_string(),
            options: vec![],
            multi_select: false,
        },
    ]
}

#[test]
fn planning_workflow_produces_prd_and_todo_in_output_directory() {
    let backend = MockBackend::new();
    backend.push_ok(DELIMITED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-planning-test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let result = workflow.plan(
        "Build a user authentication system with login and logout",
        &output_dir,
        None,
        None,
        false,
        false,
    );

    let output_path = result.expect("planning should succeed");
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
}

#[test]
fn planning_workflow_invokes_backend_with_plan_permission_mode() {
    let backend = MockBackend::new();
    backend.push_ok(DELIMITED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-planning-invoke-test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let _ = workflow.plan("Feature X", &output_dir, None, None, false, false);

    let state = workflow.state();
    assert!(
        matches!(state, tddy_core::WorkflowState::Planned { .. }),
        "workflow should transition to Planned"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn planning_workflow_transitions_to_failed_when_backend_errors() {
    let backend = MockBackend::new();
    backend.push_err("simulated backend failure");

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-planning-fail-test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let result = workflow.plan("Feature Y", &output_dir, None, None, false, false);

    assert!(result.is_err(), "planning should fail");
    assert!(
        matches!(workflow.state(), tddy_core::WorkflowState::Failed { .. }),
        "workflow should transition to Failed on backend error"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn planning_workflow_returns_clarification_needed_when_backend_returns_questions() {
    let backend = MockBackend::new();
    backend.push_ok_with_questions("", "sess-qa", clarification_questions());

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-planning-questions-test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let result = workflow.plan("Feature Z", &output_dir, None, None, false, false);

    match &result {
        Err(tddy_core::WorkflowError::ClarificationNeeded { questions, .. }) => {
            assert_eq!(questions.len(), 2);
            assert_eq!(questions[0].question, "What is the target audience?");
            assert_eq!(questions[1].question, "What is the expected timeline?");
        }
        _ => panic!("expected ClarificationNeeded, got {:?}", result),
    }

    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn planning_workflow_returns_clarification_needed_with_structured_questions() {
    let backend = MockBackend::new();
    backend.push_ok_with_questions(
        "", // output not used when questions present
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
            },
            ClarificationQuestion {
                header: "Timeline".to_string(),
                question: "What is the expected timeline?".to_string(),
                options: vec![],
                multi_select: false,
            },
        ],
    );

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-planning-structured-qa-test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let result = workflow.plan("Feature Z", &output_dir, None, None, false, false);

    match &result {
        Err(tddy_core::WorkflowError::ClarificationNeeded {
            questions,
            session_id,
        }) => {
            assert_eq!(session_id, "sess-789");
            assert_eq!(questions.len(), 2);
            assert_eq!(questions[0].header, "Scope");
            assert_eq!(questions[0].question, "What is the target audience?");
            assert_eq!(questions[0].options.len(), 2);
            assert_eq!(questions[0].options[0].label, "Developers");
            assert_eq!(questions[1].header, "Timeline");
            assert_eq!(questions[1].question, "What is the expected timeline?");
        }
        _ => panic!(
            "expected ClarificationNeeded with structured questions, got {:?}",
            result
        ),
    }

    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn planning_workflow_produces_prd_after_clarification_answers() {
    let backend = MockBackend::new();
    backend.push_ok_with_questions("", "sess-qa", clarification_questions());
    backend.push_ok(DELIMITED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-planning-followup-test");
    let _ = std::fs::remove_dir_all(&output_dir);

    let first = workflow.plan("Feature Z", &output_dir, None, None, false, false);
    assert!(
        matches!(
            first,
            Err(tddy_core::WorkflowError::ClarificationNeeded { .. })
        ),
        "first call should return ClarificationNeeded"
    );

    let answers = "Developers\nQ2 2025";
    let second = workflow.plan("Feature Z", &output_dir, Some(answers), None, false, false);

    let output_path = second.expect("second call with answers should succeed");
    assert!(output_path.is_dir());

    let prd_content = std::fs::read_to_string(output_path.join("PRD.md")).expect("read PRD");
    assert!(prd_content.contains("User authentication"));

    let _ = std::fs::remove_dir_all(&output_dir);
}
