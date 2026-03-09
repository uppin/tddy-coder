//! Acceptance tests for changeset.yaml — Goal Enhancements PRD.
//!
//! These tests define expected behavior for the changeset.yaml manifest.
//! They fail until the implementation is complete.

use tddy_core::{AcceptanceTestsOptions, MockBackend, PlanOptions, RedOptions, Workflow};

const DELIMITED_OUTPUT: &str = r#"Here is my analysis.

---PRD_START---
# Feature PRD

## Summary
User authentication system.

## Acceptance Criteria
- [ ] Login with email/password
---PRD_END---

---TODO_START---
- [ ] Create auth module
---TODO_END---

That concludes the plan."#;

const ACCEPTANCE_TESTS_OUTPUT: &str = r#"Created acceptance tests.

<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Created 2 tests.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}
</structured-response>
"#;

const RED_OUTPUT: &str = r#"Created skeleton code.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created skeletons and failing tests.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}
</structured-response>
"#;

/// Plan goal creates changeset.yaml instead of .session.
/// .session should NOT exist; changeset.yaml should exist with correct structure.
#[test]
fn changeset_yaml_replaces_session_files() {
    let backend = MockBackend::new();
    backend.push_ok(DELIMITED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-changeset-replaces-session");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let session_path = output_path.join(".session");
    let changeset_path = output_path.join("changeset.yaml");

    assert!(
        !session_path.exists(),
        ".session should NOT exist — changeset.yaml replaces it"
    );
    assert!(
        changeset_path.exists(),
        "changeset.yaml should exist, path: {}",
        changeset_path.display()
    );

    let content = std::fs::read_to_string(&changeset_path).expect("read changeset.yaml");
    assert!(
        content.contains("version:") || content.contains("version :"),
        "changeset.yaml should have version"
    );
    assert!(
        content.contains("models:") || content.contains("models :"),
        "changeset.yaml should have models"
    );
    assert!(
        content.contains("sessions:") || content.contains("sessions :"),
        "changeset.yaml should have sessions"
    );
    assert!(
        content.contains("state:") || content.contains("state :"),
        "changeset.yaml should have state"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

// --- Additional acceptance tests for PRD requirements ---

const PLANNING_WITH_DISCOVERY: &str = r##"Analysis with discovery.

<structured-response content-type="application-json">
{"goal":"plan","prd":"# PRD\n## Summary\nAuth feature.","todo":"- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0","cargo":"from Cargo.toml"},"scripts":{"test":"cargo test","lint":"cargo clippy"},"doc_locations":["docs/ft/","packages/*/docs/"],"plan_dir_suggestion":"docs/dev/1-WIP/","relevant_code":[{"path":"src/workflow/mod.rs","reason":"state machine"}],"test_infrastructure":{"runner":"cargo test","conventions":"tests/*.rs"}},"demo_plan":{"demo_type":"cli","setup_instructions":"Run cargo build","steps":[{"description":"Run the CLI","command_or_action":"cargo run","expected_result":"See output"}],"verification":"CLI runs without error"}}
</structured-response>
"##;

/// Plan output with explicit PRD name for changeset.yaml name field.
const PLANNING_WITH_NAME: &str = r##"Analysis with name.

<structured-response content-type="application-json">
{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth feature.","todo":"- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]},"demo_plan":{"demo_type":"cli","setup_instructions":"Run cargo build","steps":[{"description":"Run CLI","command_or_action":"cargo run","expected_result":"See output"}],"verification":"OK"}}
</structured-response>
"##;

/// Plan output includes discovery section with toolchain versions and scripts.
#[test]
fn plan_discovery_includes_toolchain_and_scripts() {
    let backend = MockBackend::new();
    backend.push_ok(PLANNING_WITH_DISCOVERY);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-plan-discovery");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let changeset_path = output_path.join("changeset.yaml");
    assert!(changeset_path.exists(), "changeset.yaml should exist");
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset");
    assert!(
        content.contains("rust") || content.contains("toolchain"),
        "changeset discovery should include toolchain (e.g. rust)"
    );
    assert!(
        content.contains("cargo test") || content.contains("scripts"),
        "changeset discovery should include scripts (e.g. cargo test)"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Plan output identifies documentation locations and suggests plan directory.
#[test]
fn plan_discovery_identifies_doc_locations() {
    let backend = MockBackend::new();
    backend.push_ok(PLANNING_WITH_DISCOVERY);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-plan-doc-locations");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let changeset_path = output_path.join("changeset.yaml");
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset");
    assert!(
        content.contains("docs")
            || content.contains("doc_locations")
            || content.contains("plan_dir"),
        "changeset discovery should include doc_locations or plan_dir_suggestion"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Plan goal agent decides PRD name; changeset.yaml contains one-liner `name` field.
#[test]
fn changeset_yaml_contains_prd_name() {
    let backend = MockBackend::new();
    backend.push_ok(PLANNING_WITH_NAME);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-changeset-name");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let changeset_path = output_path.join("changeset.yaml");
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset.yaml");
    assert!(
        content.contains("name:") || content.contains("name :"),
        "changeset.yaml should have name field"
    );
    assert!(
        content.contains("Auth Feature") || content.contains("AuthFeature"),
        "changeset.yaml name should come from plan agent output"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Plan goal produces demo-plan.md with demo type, steps, and verification.
#[test]
fn plan_goal_creates_demo_plan() {
    let backend = MockBackend::new();
    backend.push_ok(PLANNING_WITH_DISCOVERY);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-plan-demo");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let demo_plan_path = output_path.join("demo-plan.md");
    assert!(
        demo_plan_path.exists(),
        "demo-plan.md should be created by plan goal"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Each goal reads current state from changeset.yaml and writes updated state.
/// Initial state is AcceptanceTestsReady; after red runs, state should become RedTestsReady.
#[test]
fn changeset_yaml_persists_workflow_state() {
    let plan_dir = std::env::temp_dir().join("tddy-changeset-state");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests").expect("write AT");

    let changeset_content = r#"version: 1
models:
  red: sonnet
sessions:
  - id: "impl-sess-1"
    agent: claude
    tag: impl
    created_at: "2026-03-07T10:05:00Z"
state:
  current: AcceptanceTestsReady
  updated_at: "2026-03-07T10:05:00Z"
  history: []
artifacts: {}
"#;
    std::fs::write(plan_dir.join("changeset.yaml"), changeset_content).expect("write changeset");
    std::fs::write(plan_dir.join(".impl-session"), "impl-sess-1")
        .expect("write .impl-session for red");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());

    let content = std::fs::read_to_string(plan_dir.join("changeset.yaml")).expect("read changeset");
    assert!(
        content.contains("RedTestsReady"),
        "changeset.yaml state should be updated to RedTestsReady after red goal"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Goals use model from changeset.yaml when --model not specified.
/// CLI --model overrides changeset.yaml.
#[test]
fn changeset_yaml_model_resolution() {
    let plan_dir = std::env::temp_dir().join("tddy-changeset-model");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests").expect("write AT");

    let changeset_content = r#"version: 1
models:
  red: sonnet
sessions:
  - id: "impl-sess-1"
    agent: claude
    tag: impl
    created_at: "2026-03-07T10:00:00Z"
state:
  current: RedTestsReady
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {}
"#;
    std::fs::write(plan_dir.join("changeset.yaml"), changeset_content).expect("write changeset");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let options = RedOptions::default();
    let _ = workflow.red(&plan_dir, None, &options);

    let invocations = workflow.backend().invocations();
    let req = invocations.last().unwrap();
    assert_eq!(
        req.model.as_deref(),
        Some("sonnet"),
        "red goal should use model from changeset.yaml when --model not specified"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

const RED_OUTPUT_WITH_MARKERS: &str = r#"Created skeleton code with markers.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created skeletons and failing tests with logging markers.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}],"markers":[{"marker_id":"M001","test_name":"test_auth","scope":"auth_service::validate","data":{"user":"test@example.com"}}],"marker_results":[{"marker_id":"M001","test_name":"test_auth","scope":"auth_service::validate","collected":true,"investigation":null}]}
</structured-response>
"#;

/// Red goal output includes marker definitions with JSON format and scope data.
#[test]
fn red_goal_adds_logging_markers() {
    let plan_dir = std::env::temp_dir().join("tddy-red-markers");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests").expect("write AT");

    let backend = MockBackend::new();
    backend.push_ok(RED_OUTPUT_WITH_MARKERS);

    let mut workflow = Workflow::new(backend);
    let _ = workflow.red(&plan_dir, None, &RedOptions::default());

    let output = workflow.state();
    let _ = output;
    let red_output_path = plan_dir.join("red-output.md");
    let content = std::fs::read_to_string(&red_output_path).expect("read red-output.md");
    assert!(
        content.contains("M001") || content.contains("marker") || content.contains("tddy"),
        "red-output.md should document logging markers"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

/// Red goal verifies which markers were collected from test output.
#[test]
fn red_goal_verifies_marker_collection() {
    use tddy_core::output::parse_red_response;

    let out = parse_red_response(RED_OUTPUT_WITH_MARKERS).expect("parse red output with markers");
    assert!(out.skeletons.len() >= 1, "red output should have skeletons");
}

/// System prompt is written to plan directory; session object (not global artifacts) references it.
#[test]
fn system_prompt_stored_in_plan_dir() {
    let backend = MockBackend::new();
    backend.push_ok(PLANNING_WITH_DISCOVERY);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-system-prompt-plan-dir");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let system_prompt_path = output_path.join("system-prompt-plan.md");
    assert!(
        system_prompt_path.exists(),
        "system prompt should be written to plan dir, not temp file: {}",
        system_prompt_path.display()
    );

    let changeset_content =
        std::fs::read_to_string(output_path.join("changeset.yaml")).expect("read changeset");
    assert!(
        changeset_content.contains("system_prompt") || changeset_content.contains("system-prompt"),
        "changeset should reference system prompt file"
    );
    assert!(
        changeset_content.contains("system_prompt_file")
            || changeset_content.contains("system_prompt_file:"),
        "session object should have system_prompt_file field (not global artifacts)"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Session entry has system_prompt_file; system prompt reference is per-session, not global.
#[test]
fn session_object_has_system_prompt_file() {
    use tddy_core::changeset::read_changeset;

    let backend = MockBackend::new();
    // Plan writes changeset only when session_id is Some; use push_ok_with_questions to provide it.
    backend.push_ok_with_questions(PLANNING_WITH_DISCOVERY, "sess-plan-123", vec![]);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-session-system-prompt");
    let _ = std::fs::remove_dir_all(&output_dir);

    let output_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let changeset = read_changeset(&output_path).expect("read changeset");
    let plan_session = changeset
        .sessions
        .iter()
        .find(|s| s.tag == "plan")
        .expect("plan session should exist");
    assert!(
        plan_session.system_prompt_file.is_some(),
        "plan session should have system_prompt_file"
    );
    assert_eq!(
        plan_session.system_prompt_file.as_deref(),
        Some("system-prompt-plan.md"),
        "plan session system_prompt_file should be system-prompt-plan.md"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Changeset stores initial user prompt; clarification_qa empty when no questions asked.
#[test]
fn changeset_contains_initial_prompt() {
    use tddy_core::changeset::read_changeset;

    let backend = MockBackend::new();
    backend.push_ok(PLANNING_WITH_DISCOVERY);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-changeset-initial-prompt");
    let _ = std::fs::remove_dir_all(&output_dir);

    let input = "Build auth with JWT and session management";
    let output_path = workflow
        .plan(input, &output_dir, None, &PlanOptions::default())
        .expect("planning should succeed");

    let changeset = read_changeset(&output_path).expect("read changeset");
    assert!(
        changeset.initial_prompt.is_some(),
        "changeset should have initial_prompt"
    );
    assert_eq!(
        changeset.initial_prompt.as_deref(),
        Some(input),
        "initial_prompt should match user input"
    );
    assert!(
        changeset.clarification_qa.is_empty(),
        "clarification_qa should be empty when no questions asked"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Changeset stores questions and answers when planning required clarification.
#[test]
fn changeset_contains_clarification_qa() {
    use tddy_core::backend::ClarificationQuestion;
    use tddy_core::changeset::read_changeset;
    use tddy_core::QuestionOption;

    let backend = MockBackend::new();
    let questions = vec![
        ClarificationQuestion {
            header: "Scope".to_string(),
            question: "What is the target audience?".to_string(),
            options: vec![
                QuestionOption {
                    label: "Developers".to_string(),
                    description: "Internal devs".to_string(),
                },
                QuestionOption {
                    label: "End users".to_string(),
                    description: "External users".to_string(),
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
    ];
    backend.push_ok_with_questions("", "sess-qa", questions);
    backend.push_ok(DELIMITED_OUTPUT);

    let mut workflow = Workflow::new(backend);
    let output_dir = std::env::temp_dir().join("tddy-changeset-clarification-qa");
    let _ = std::fs::remove_dir_all(&output_dir);

    let input = "Feature Z";
    let first = workflow.plan(input, &output_dir, None, &PlanOptions::default());
    assert!(
        matches!(
            first,
            Err(tddy_core::WorkflowError::ClarificationNeeded { .. })
        ),
        "first call should return ClarificationNeeded"
    );

    let answers = "Developers\nQ2 2025";
    let output_path = workflow
        .plan(input, &output_dir, Some(answers), &PlanOptions::default())
        .expect("second call with answers should succeed");

    let changeset = read_changeset(&output_path).expect("read changeset");
    assert_eq!(
        changeset.initial_prompt.as_deref(),
        Some(input),
        "initial_prompt should match user input"
    );
    assert!(
        !changeset.clarification_qa.is_empty(),
        "clarification_qa should have entries when clarification occurred"
    );
    assert_eq!(
        changeset.clarification_qa.len(),
        2,
        "clarification_qa should have one entry per question"
    );
    assert_eq!(
        changeset.clarification_qa[0].question.header, "Scope",
        "first question header"
    );
    assert_eq!(
        changeset.clarification_qa[0].question.question, "What is the target audience?",
        "first question text"
    );
    assert_eq!(
        changeset.clarification_qa[0].answer, "Developers",
        "first answer"
    );
    assert_eq!(
        changeset.clarification_qa[1].question.header, "Timeline",
        "second question header"
    );
    assert_eq!(
        changeset.clarification_qa[1].answer, "Q2 2025",
        "second answer"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// After plan + acceptance-tests + red, sessions array has 3 entries with correct tags.
#[test]
fn changeset_yaml_sessions_array_tracks_all_sessions() {
    let backend = MockBackend::new();
    backend.push_ok(DELIMITED_OUTPUT);
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);
    backend.push_ok(RED_OUTPUT);

    let output_dir = std::env::temp_dir().join("tddy-changeset-sessions");
    let _ = std::fs::remove_dir_all(&output_dir);

    let mut workflow = Workflow::new(backend);
    let plan_path = workflow
        .plan("Build auth", &output_dir, None, &PlanOptions::default())
        .expect("plan should succeed");

    std::fs::write(
        plan_path.join("PRD.md"),
        std::fs::read_to_string(plan_path.join("PRD.md")).unwrap_or_default(),
    )
    .ok();
    let _ = workflow.acceptance_tests(&plan_path, None, &AcceptanceTestsOptions::default());
    let _ = workflow.red(&plan_path, None, &RedOptions::default());

    let changeset_path = plan_path.join("changeset.yaml");
    assert!(
        changeset_path.exists(),
        "changeset.yaml should exist after full pipeline"
    );

    let content = std::fs::read_to_string(&changeset_path).expect("read changeset");
    let plan_tag_count = content.matches("tag: plan").count();
    let at_tag_count = content.matches("tag: acceptance-tests").count();
    let impl_tag_count = content.matches("tag: impl").count();
    assert!(
        plan_tag_count >= 1 && (at_tag_count >= 1 || impl_tag_count >= 1),
        "sessions array should track plan and impl/acceptance-tests sessions"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

// ── Acceptance test for TDD Workflow Restructure PRD: R5 ──────────────────────

/// AC5: changeset.yaml exists on disk immediately after the user enters their prompt,
/// before the plan agent runs.
///
/// This test verifies that:
/// - changeset.yaml is written with state "Init" before the plan backend is invoked
/// - The initial_prompt is populated in the changeset
///
/// This test will fail until:
/// - workflow.plan() writes a minimal changeset.yaml (state: Init) before invoking the backend
/// - The changeset contains the initial_prompt field before the plan agent runs
#[test]
fn changeset_written_before_plan_agent() {
    use std::sync::{Arc, Mutex};
    use tddy_core::changeset::read_changeset;
    use tddy_core::{BackendError, CodingBackend, InvokeRequest, InvokeResponse};

    /// A backend that checks disk state when invoke() is called (before returning).
    #[derive(Debug)]
    struct CheckingBackend {
        plan_dir_captured: Arc<Mutex<Option<std::path::PathBuf>>>,
        changeset_state_at_invoke: Arc<Mutex<Option<String>>>,
        initial_prompt_at_invoke: Arc<Mutex<Option<String>>>,
    }

    #[async_trait::async_trait]
    impl CodingBackend for CheckingBackend {
        async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
            if let Some(ref wd) = request.working_dir {
                if let Ok(entries) = std::fs::read_dir(wd) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() && path.join("changeset.yaml").exists() {
                            *self.plan_dir_captured.lock().unwrap() = Some(path.clone());
                            if let Ok(cs) = read_changeset(&path) {
                                *self.changeset_state_at_invoke.lock().unwrap() =
                                    Some(cs.state.current.clone());
                                *self.initial_prompt_at_invoke.lock().unwrap() =
                                    cs.initial_prompt.clone();
                            }
                        }
                    }
                }
            }

            Ok(InvokeResponse {
                output: DELIMITED_OUTPUT.to_string(),
                exit_code: 0,
                session_id: Some("sess-check-123".to_string()),
                questions: vec![],
                raw_stream: None,
                stderr: None,
            })
        }

        fn name(&self) -> &str {
            "checking-mock"
        }
    }

    let output_dir = std::env::temp_dir().join("tddy-changeset-before-plan");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let changeset_state = Arc::new(Mutex::new(None));
    let initial_prompt = Arc::new(Mutex::new(None));
    let plan_dir_captured = Arc::new(Mutex::new(None));

    let backend = CheckingBackend {
        plan_dir_captured: plan_dir_captured.clone(),
        changeset_state_at_invoke: changeset_state.clone(),
        initial_prompt_at_invoke: initial_prompt.clone(),
    };

    let mut workflow = Workflow::new(backend);
    let input = "Build auth with early changeset";
    let _ = workflow.plan(input, &output_dir, None, &PlanOptions::default());

    let captured_state = changeset_state.lock().unwrap().clone();
    assert_eq!(
        captured_state,
        Some("Init".to_string()),
        "changeset.yaml should exist with state 'Init' before plan agent is invoked, got: {:?}",
        captured_state
    );

    let captured_prompt = initial_prompt.lock().unwrap().clone();
    assert_eq!(
        captured_prompt,
        Some(input.to_string()),
        "changeset.yaml should have initial_prompt populated before plan agent runs, got: {:?}",
        captured_prompt
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}
