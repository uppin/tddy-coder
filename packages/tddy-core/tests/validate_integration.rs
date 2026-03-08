//! Integration tests for the validate-changes workflow with MockBackend.

use tddy_core::{Goal, MockBackend, ValidateOptions, Workflow, WorkflowState};

const VALIDATE_OUTPUT: &str = r#"Analysis complete.

<structured-response content-type="application-json">
{"goal":"validate-changes","summary":"Analyzed 3 changed files. Risk level: medium. Found 2 issues.","risk_level":"medium","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[{"severity":"warning","category":"code_quality","file":"src/main.rs","line":42,"description":"Function exceeds 50 lines","suggestion":"Extract into smaller functions"},{"severity":"info","category":"test_infrastructure","file":"src/lib.rs","line":10,"description":"Test helper visible in production","suggestion":"Move to test module"}],"changeset_sync":{"status":"not_found","items_updated":0,"items_added":0},"files_analyzed":[{"file":"src/main.rs","lines_changed":25,"changeset_item":null}],"test_impact":{"tests_affected":2,"new_tests_needed":1}}
</structured-response>
"#;

/// validate() invokes backend with Goal::Validate.
#[test]
fn validate_workflow_invokes_backend_with_validate_goal() {
    let working_dir = std::env::temp_dir().join("tddy-validate-goal-test");
    let plan_dir = std::env::temp_dir().join("tddy-validate-goal-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&working_dir, Some(&plan_dir), None, &options);

    assert!(result.is_ok(), "validate should succeed, got: {:?}", result);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert_eq!(
        req.goal,
        Goal::Validate,
        "InvokeRequest should have goal Validate"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() transitions workflow to Validated state on success.
#[test]
fn validate_workflow_transitions_to_validated_state() {
    let working_dir = std::env::temp_dir().join("tddy-validate-state-test");
    let plan_dir = std::env::temp_dir().join("tddy-validate-state-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let _ = workflow.validate(&working_dir, Some(&plan_dir), None, &options);

    let state = workflow.state();
    assert!(
        matches!(state, WorkflowState::Validated { .. }),
        "workflow should transition to Validated, got {:?}",
        state
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() writes validation-report.md to the plan directory.
#[test]
fn validate_workflow_writes_validation_report_md() {
    let working_dir = std::env::temp_dir().join("tddy-validate-writes-md");
    let plan_dir = std::env::temp_dir().join("tddy-validate-writes-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let _ = workflow.validate(&working_dir, Some(&plan_dir), None, &options);

    let report_path = plan_dir.join("validation-report.md");
    assert!(
        report_path.exists(),
        "validation-report.md should be written to plan directory: {}",
        report_path.display()
    );
    let content = std::fs::read_to_string(&report_path).expect("read validation-report.md");
    assert!(
        content.contains("Analyzed"),
        "report should contain summary text, got: {}",
        content
    );
    assert!(
        content.contains("medium"),
        "report should contain risk level, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() returns PlanDirInvalid when plan_dir is None.
#[test]
fn validate_workflow_requires_plan_dir() {
    let working_dir = std::env::temp_dir().join("tddy-validate-no-plan-dir");
    let _ = std::fs::remove_dir_all(&working_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&working_dir, None, None, &options);

    assert!(
        result.is_err(),
        "validate without plan_dir should fail, got: {:?}",
        result
    );
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::PlanDirInvalid(_))),
        "expected PlanDirInvalid, got: {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&working_dir);
}

/// validate() includes changeset/PRD context in prompt when plan_dir is provided.
#[test]
fn validate_workflow_includes_plan_dir_context_when_provided() {
    let working_dir = std::env::temp_dir().join("tddy-validate-with-plan-dir");
    let plan_dir = std::env::temp_dir().join("tddy-validate-plan-context");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    // Write a PRD and changeset so the validate method can read context
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Summary\nAuth feature.").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-ctx-123");

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&working_dir, Some(&plan_dir), None, &options);

    assert!(
        result.is_ok(),
        "validate with plan_dir should succeed, got: {:?}",
        result
    );

    // When plan_dir is provided, validation-report.md goes to plan_dir
    let report_in_plan = plan_dir.join("validation-report.md");
    assert!(
        report_in_plan.exists(),
        "validation-report.md should be in plan_dir when provided, not found at: {}",
        report_in_plan.display()
    );

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    // When plan_dir is provided, prompt should include context from the plan
    assert!(
        req.prompt.contains("Auth feature")
            || req.prompt.contains("PRD")
            || req.prompt.contains("changeset"),
        "prompt should include plan context when plan_dir provided, got prompt start: {}",
        &req.prompt[..req.prompt.len().min(200)]
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() uses is_resume: false (fresh session, not resumed from prior run).
#[test]
fn validate_workflow_uses_fresh_session_not_resume() {
    let working_dir = std::env::temp_dir().join("tddy-validate-fresh-session");
    let plan_dir = std::env::temp_dir().join("tddy-validate-fresh-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    backend.push_ok(VALIDATE_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let _ = workflow.validate(&working_dir, Some(&plan_dir), None, &options);

    let invocations = workflow.backend().invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let req = invocations.last().unwrap();
    assert!(
        !req.is_resume,
        "validate should use a fresh session (is_resume: false), not resume a prior session"
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate() returns ParseError when backend returns a response with no structured-response block.
#[test]
fn validate_workflow_returns_parse_error_on_malformed_response() {
    let working_dir = std::env::temp_dir().join("tddy-validate-parse-error");
    let plan_dir = std::env::temp_dir().join("tddy-validate-parse-plan");
    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&working_dir).expect("create working dir");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let backend = MockBackend::new();
    // No structured-response block — parser should fail
    backend.push_ok("I analyzed the changes and they look fine. No issues found.");

    let mut workflow = Workflow::new(backend);
    let options = ValidateOptions::default();
    let result = workflow.validate(&working_dir, Some(&plan_dir), None, &options);

    assert!(
        result.is_err(),
        "validate should fail on malformed response"
    );
    assert!(
        matches!(result, Err(tddy_core::WorkflowError::ParseError(_))),
        "expected ParseError, got: {:?}",
        result
    );

    let _ = std::fs::remove_dir_all(&working_dir);
    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// validate_allowlist() contains required read and bash tools for git and cargo.
#[test]
fn validate_allowlist_contains_required_tools() {
    let allowlist = tddy_core::validate_allowlist();

    assert!(
        allowlist.iter().any(|t| t == "Read"),
        "allowlist must include Read, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Glob"),
        "allowlist must include Glob, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t == "Grep"),
        "allowlist must include Grep, got: {:?}",
        allowlist
    );
    assert!(
        allowlist.iter().any(|t| t.contains("git diff")),
        "allowlist must include a Bash(git diff *) entry, got: {:?}",
        allowlist
    );
    assert!(
        allowlist
            .iter()
            .any(|t| t.contains("cargo build") || t.contains("cargo check")),
        "allowlist must include a Bash(cargo build *) or Bash(cargo check *) entry, got: {:?}",
        allowlist
    );
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
    created_at: "2026-03-07T10:00:00Z"
state:
  current: Planned
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {{}}
"#,
        session_id
    );
    std::fs::write(plan_dir.join("changeset.yaml"), changeset).expect("write changeset");
}
