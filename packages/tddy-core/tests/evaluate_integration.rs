//! Integration tests for the evaluate-changes workflow with MockBackend.
//!
//! All tests reference types and methods introduced by the evaluate-changes rename:
//! - `Goal::Evaluate` (renamed from Goal::Validate)
//! - `EvaluateOptions` (renamed from ValidateOptions)
//! - `WorkflowState::Evaluated` (renamed from WorkflowState::Validated)
//! - `workflow.evaluate()` method (renamed from workflow.validate())
//! - `evaluate_allowlist()` for the evaluate goal
//! - `evaluation-report.md` written to plan_dir (not working_dir)
//! - New report fields: changed_files, affected_tests, validity_assessment
//!
//! These tests are in Red state — they fail to compile because the production
//! code has not been renamed/implemented yet.

use tddy_core::{evaluate_allowlist, EvaluateOptions, Goal, MockBackend, Workflow, WorkflowState};

/// Full evaluate-changes structured response with new fields:
/// changed_files, affected_tests, validity_assessment.
const EVALUATE_OUTPUT: &str = r#"Evaluation complete.

<structured-response content-type="application-json">
{"goal":"evaluate-changes","summary":"Evaluated 3 changed files. Risk level: medium. Found 2 issues.","risk_level":"medium","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[{"severity":"warning","category":"code_quality","file":"src/main.rs","line":42,"description":"Function exceeds 50 lines","suggestion":"Extract into smaller functions"},{"severity":"info","category":"test_infrastructure","file":"src/lib.rs","line":10,"description":"Test helper visible in production","suggestion":"Move to test module"}],"changeset_sync":{"status":"not_found","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/main.rs","lines_changed":25,"changeset_item":null}],"test_impact":{"tests_affected":2,"new_tests_needed":1},"changed_files":[{"path":"src/main.rs","change_type":"modified","lines_added":15,"lines_removed":3},{"path":"src/lib.rs","change_type":"modified","lines_added":5,"lines_removed":0},{"path":"tests/main_test.rs","change_type":"added","lines_added":40,"lines_removed":0}],"affected_tests":[{"path":"tests/main_test.rs","status":"created","description":"New acceptance tests for the main module"},{"path":"tests/integration_test.rs","status":"updated","description":"Updated to cover new code paths"}],"validity_assessment":"The change is valid for the intended use-case. All acceptance criteria from the PRD are addressed. The new code follows existing patterns and does not introduce breaking changes. Risk is medium due to the size of the diff."}
</structured-response>
"#;

/// evaluate() invokes backend with Goal::Evaluate (renamed from Goal::Validate).
#[test]
fn evaluate_workflow_invokes_backend_with_evaluate_goal() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-goal-test");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-goal-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let result = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        Goal::Evaluate,
        "InvokeRequest should have goal Evaluate (renamed from Validate)"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// evaluate() transitions workflow to Evaluated state on success.
#[test]
fn evaluate_workflow_transitions_to_evaluated_state() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-state-test");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-state-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let _ = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    let state = workflow.state();
    assert!(
        matches!(state, WorkflowState::Evaluated { .. }),
        "workflow should transition to Evaluated (not Validated), got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// evaluate() writes evaluation-report.md to plan_dir, NOT to working_dir.
#[test]
fn evaluate_workflow_writes_evaluation_report_to_plan_dir() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-writes-working");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-writes-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let _ = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    let report_in_plan = plan_dir.join("evaluation-report.md");
    assert!(
        report_in_plan.exists(),
        "evaluation-report.md must be written to plan_dir, not found at: {}",
        report_in_plan.display()
    );

    // Must NOT appear in working_dir (old behavior)
    let report_in_working = working_dir.join("evaluation-report.md");
    assert!(
        !report_in_working.exists(),
        "evaluation-report.md must NOT be in working_dir — it belongs in plan_dir, found at: {}",
        report_in_working.display()
    );

    // Confirm old filename is also absent
    assert!(
        !plan_dir.join("validation-report.md").exists(),
        "old validation-report.md must not be created; the new name is evaluation-report.md"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// evaluate() without plan_dir returns an error (plan_dir is required).
#[test]
fn evaluate_workflow_requires_plan_dir() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-no-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    // plan_dir is None — evaluate-changes requires a plan_dir to write its report
    let result = workflow.evaluate(&working_dir, None, None, &options);

    assert!(
        result.is_err(),
        "evaluate should fail when plan_dir is None — plan_dir is required for evaluate-changes"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
}

/// evaluate() report includes a Changed Files section listing all changed files.
#[test]
fn evaluate_workflow_includes_changed_files_in_report() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-changed-files");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-changed-files-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let result = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let report = std::fs::read_to_string(plan_dir.join("evaluation-report.md"))
        .expect("evaluation-report.md should exist in plan_dir");

    assert!(
        report.contains("Changed Files") || report.contains("changed_files") || report.contains("src/main.rs"),
        "evaluation-report.md must include a Changed Files section listing all modified files, got:\n{}",
        report
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// evaluate() report includes an Affected Tests section.
#[test]
fn evaluate_workflow_includes_affected_tests_in_report() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-affected-tests");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-affected-tests-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let result = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let report = std::fs::read_to_string(plan_dir.join("evaluation-report.md"))
        .expect("evaluation-report.md should exist in plan_dir");

    assert!(
        report.contains("Affected Tests")
            || report.contains("affected_tests")
            || report.contains("main_test.rs"),
        "evaluation-report.md must include an Affected Tests section (created/updated/removed/skipped), got:\n{}",
        report
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// evaluate() report includes a Validity Assessment section.
#[test]
fn evaluate_workflow_includes_validity_assessment() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-validity");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-validity-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let result = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    assert!(result.is_ok(), "evaluate should succeed, got: {:?}", result);

    let report = std::fs::read_to_string(plan_dir.join("evaluation-report.md"))
        .expect("evaluation-report.md should exist in plan_dir");

    assert!(
        report.contains("Validity Assessment")
            || report.contains("validity_assessment")
            || report.contains("valid for the intended use-case"),
        "evaluation-report.md must include a Validity Assessment section with detailed analysis, got:\n{}",
        report
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// evaluate() includes PRD/changeset context in prompt when plan_dir is provided.
#[test]
fn evaluate_workflow_includes_plan_dir_context_when_provided() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-with-plan-dir");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-plan-context");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Summary\nAuth feature.").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-ctx-456");

    let backend = MockBackend::new();
    backend.push_ok(EVALUATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let result = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    assert!(
        result.is_ok(),
        "evaluate with plan_dir should succeed, got: {:?}",
        result
    );

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert!(
        req.prompt.contains("Auth feature")
            || req.prompt.contains("PRD")
            || req.prompt.contains("changeset"),
        "prompt must include plan context (PRD/changeset) when plan_dir is provided, got prompt start: {}",
        &req.prompt[..req.prompt.len().min(200)]
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// evaluate_allowlist() contains required read, git and cargo tools.
#[test]
fn evaluate_allowlist_contains_required_tools() {
    let allowlist = evaluate_allowlist();

    assert!(
        allowlist.iter().any(|t| t == "Read"),
        "evaluate_allowlist must include Read, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Glob"),
        "evaluate_allowlist must include Glob, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Grep"),
        "evaluate_allowlist must include Grep, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t.contains("git diff")),
        "evaluate_allowlist must include a Bash(git diff *) entry, got: {:?}",
        allowlist
    );
    assert!(
        allowlist
            .iter()
            .any(|t| t.contains("cargo build") || t.contains("cargo check")),
        "evaluate_allowlist must include a Bash(cargo build *) or Bash(cargo check *) entry, got: {:?}",
        allowlist
    );
}

/// evaluate() returns ParseError when backend returns a response with no structured-response block.
#[test]
fn evaluate_workflow_returns_parse_error_on_malformed_response() {
    let working_dir = std::env::temp_dir().join("tddy-evaluate-parse-error");
    let plan_dir = std::env::temp_dir().join("tddy-evaluate-parse-error-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    // No structured-response block — parser should fail
    backend.push_ok("I evaluated the changes and they look fine. No issues found.");

    let mut workflow = Workflow::new(backend);
    let options = EvaluateOptions::default();
    let result = workflow.evaluate(&working_dir, Some(&plan_dir), None, &options);

    assert!(
        result.is_err(),
        "evaluate should fail on malformed response (missing structured-response block)"
    );
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::ParseError(_))),
        "expected ParseError, got: {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn write_changeset_for_plan_session(plan_dir: &std::path::Path, session_id: &str) {
    let changeset = format!(
        r#"version: 1
models: {{}}
sessions:
  - id: "{}"
    agent: claude
    tag: plan
    created_at: "2026-03-08T10:00:00Z"
state:
  current: Planned
  updated_at: "2026-03-08T10:00:00Z"
  history: []
artifacts: {{}}
"#,
        session_id
    );
    std::fs::write(plan_dir.join("changeset.yaml"), changeset).expect("write changeset");
}
