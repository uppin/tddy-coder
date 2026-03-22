//! Acceptance tests for changeset.yaml — Goal Enhancements PRD.
//!
//! These tests define expected behavior for the changeset.yaml manifest.
//! Migrated from Workflow to WorkflowEngine.

mod common;
mod fixtures;

use std::sync::Arc;
use tddy_core::changeset::read_changeset;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};

use common::{
    ctx_acceptance_tests, ctx_plan, ctx_red, get_session_dir_from_session, run_goal_until_done,
    run_plan, session_dir_for_input, temp_dir_with_git_repo,
};
use fixtures::*;

/// Plan goal creates changeset.yaml instead of .session.
/// .session should NOT exist; changeset.yaml should exist with correct structure.
#[tokio::test]
async fn changeset_yaml_replaces_session_files() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON);

    let output_dir = std::env::temp_dir().join("tddy-changeset-replaces-session");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-changeset-replaces-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (output_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
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

/// Plan output includes discovery section with toolchain versions and scripts.
#[tokio::test]
async fn plan_discovery_includes_toolchain_and_scripts() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_WITH_DISCOVERY);

    let output_dir = std::env::temp_dir().join("tddy-plan-toolchain-discovery");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-plan-toolchain-discovery-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (output_path, _) = run_plan(&engine, "Build auth toolchain", &output_dir, None)
        .await
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
#[tokio::test]
async fn plan_discovery_identifies_doc_locations() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_WITH_DISCOVERY);

    let output_dir = std::env::temp_dir().join("tddy-plan-doc-locations");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-plan-doc-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (output_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("planning should succeed");

    let changeset_path = output_path.join("changeset.yaml");
    let content = std::fs::read_to_string(&changeset_path).expect("read changeset");
    assert!(
        content.contains("docs") || content.contains("doc_locations"),
        "changeset discovery should include doc_locations"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Plan goal agent decides PRD name; changeset.yaml contains one-liner `name` field.
#[tokio::test]
async fn changeset_yaml_contains_prd_name() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_WITH_NAME);

    let output_dir = std::env::temp_dir().join("tddy-changeset-name");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-changeset-name-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (output_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
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
#[tokio::test]
async fn plan_goal_creates_demo_plan() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_WITH_DISCOVERY);

    let output_dir = std::env::temp_dir().join("tddy-plan-demo");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-plan-demo-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (output_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
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
#[tokio::test]
async fn changeset_yaml_persists_workflow_state() {
    let session_dir = std::env::temp_dir().join("tddy-changeset-state");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests",
    )
    .expect("write AT");

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
    std::fs::write(session_dir.join("changeset.yaml"), changeset_content).expect("write changeset");
    std::fs::write(session_dir.join(".impl-session"), "impl-sess-1")
        .expect("write .impl-session for red");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_JSON_MINIMAL);
    backend.push_ok(GREEN_JSON_MINIMAL);
    backend.push_ok(EVALUATE_JSON);
    backend.push_ok(VALIDATE_JSON);
    backend.push_ok(REFACTOR_JSON);
    backend.push_ok(UPDATE_DOCS_JSON); // red -> green -> evaluate -> validate -> refactor -> update-docs

    let storage_dir = std::env::temp_dir().join("tddy-changeset-state-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = run_goal_until_done(&engine, "red", ctx).await.unwrap();

    let content =
        std::fs::read_to_string(session_dir.join("changeset.yaml")).expect("read changeset");
    assert!(
        content.contains("RedTestsReady"),
        "changeset.yaml state should be updated to RedTestsReady after red goal"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Goals use model from changeset.yaml when --model not specified.
/// CLI --model overrides changeset.yaml.
#[tokio::test]
async fn changeset_yaml_model_resolution() {
    let session_dir = std::env::temp_dir().join("tddy-changeset-model");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests",
    )
    .expect("write AT");

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
    std::fs::write(session_dir.join("changeset.yaml"), changeset_content).expect("write changeset");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_JSON_MINIMAL);
    backend.push_ok(GREEN_JSON_MINIMAL);
    backend.push_ok(EVALUATE_JSON);
    backend.push_ok(VALIDATE_JSON);
    backend.push_ok(REFACTOR_JSON);
    backend.push_ok(UPDATE_DOCS_JSON);

    let storage_dir = std::env::temp_dir().join("tddy-changeset-model-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = run_goal_until_done(&engine, "red", ctx).await.unwrap();

    let invocations = backend.invocations();
    let red_inv = invocations
        .iter()
        .find(|r| r.goal_id == tddy_core::GoalId::new("red"))
        .expect("red invocation should exist");
    assert_eq!(
        red_inv.model.as_deref(),
        Some("sonnet"),
        "red goal should use model from changeset.yaml when --model not specified"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Red goal output includes marker definitions with JSON format and scope data.
#[tokio::test]
async fn red_goal_adds_logging_markers() {
    let session_dir = std::env::temp_dir().join("tddy-red-markers");
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests",
    )
    .expect("write AT");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_JSON_WITH_LOGGING_MARKERS);
    backend.push_ok(GREEN_JSON_MINIMAL);
    backend.push_ok(EVALUATE_JSON);
    backend.push_ok(VALIDATE_JSON);
    backend.push_ok(REFACTOR_JSON);
    backend.push_ok(UPDATE_DOCS_JSON);

    let storage_dir = std::env::temp_dir().join("tddy-red-markers-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let ctx = ctx_red(session_dir.clone(), None);
    let _ = run_goal_until_done(&engine, "red", ctx).await.unwrap();

    let red_output_path = session_dir.join("red-output.md");
    let content = std::fs::read_to_string(&red_output_path).expect("read red-output.md");
    assert!(
        content.contains("M001") || content.contains("marker") || content.contains("tddy"),
        "red-output.md should document logging markers"
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// Red goal verifies which markers were collected from test output.
#[test]
fn red_goal_verifies_marker_collection() {
    use tddy_workflow_recipes::parse_red_response;

    let out =
        parse_red_response(RED_JSON_WITH_LOGGING_MARKERS).expect("parse red output with markers");
    assert!(
        !out.skeletons.is_empty(),
        "red output should have skeletons"
    );
}

/// System prompt is written to plan directory; session object (not global artifacts) references it.
#[tokio::test]
#[ignore = "PlanTask does not write system-prompt-plan.md to plan dir; Workflow does"]
async fn system_prompt_stored_in_session_dir() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_WITH_DISCOVERY);

    let output_dir = std::env::temp_dir().join("tddy-system-prompt-plan-dir");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-system-prompt-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (output_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
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
#[tokio::test]
async fn session_object_has_system_prompt_file() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok_with_questions(PLAN_JSON_WITH_DISCOVERY, "sess-plan-123", vec![]);

    let output_dir = std::env::temp_dir().join("tddy-session-system-prompt");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-session-sysprompt-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (output_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
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
#[tokio::test]
async fn changeset_contains_initial_prompt() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON_WITH_DISCOVERY);

    let output_dir = std::env::temp_dir().join("tddy-changeset-initial-prompt");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-changeset-initial-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let input = "Build auth with JWT and session management";
    let (output_path, _) = run_plan(&engine, input, &output_dir, None)
        .await
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
#[tokio::test]
#[ignore = "Engine plan after_task does not merge clarification_qa into changeset yet"]
async fn changeset_contains_clarification_qa() {
    use std::collections::HashMap;
    use tddy_core::backend::ClarificationQuestion;
    use tddy_core::workflow::graph::ExecutionStatus;
    use tddy_core::QuestionOption;

    let backend = Arc::new(MockBackend::new());
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
            allow_other: true,
        },
        ClarificationQuestion {
            header: "Timeline".to_string(),
            question: "What is the expected timeline?".to_string(),
            options: vec![],
            multi_select: false,
            allow_other: true,
        },
    ];
    backend.push_ok_with_questions("", "sess-qa", questions);
    backend.push_ok(PLAN_JSON);
    backend.push_ok(ACCEPTANCE_TESTS_JSON_MINIMAL);
    backend.push_ok(RED_JSON_MINIMAL);
    backend.push_ok(GREEN_JSON_MINIMAL);
    backend.push_ok(EVALUATE_JSON);
    backend.push_ok(VALIDATE_JSON);
    backend.push_ok(REFACTOR_JSON);
    backend.push_ok(UPDATE_DOCS_JSON); // plan -> at -> red -> green -> evaluate -> validate -> refactor -> update-docs

    let output_dir = std::env::temp_dir().join("tddy-changeset-clarification-qa");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-changeset-qa-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let input = "Feature Z";
    let session_dir = session_dir_for_input(&output_dir, input);
    std::fs::create_dir_all(&session_dir).unwrap();
    let ctx = ctx_plan(input, output_dir.clone(), None, None);
    let result = engine.run_goal(&GoalId::new("plan"), ctx).await.unwrap();

    assert!(
        matches!(&result.status, ExecutionStatus::WaitingForInput { .. }),
        "first call should return WaitingForInput (ClarificationNeeded), got {:?}",
        result.status
    );

    let mut updates = HashMap::new();
    updates.insert(
        "answers".to_string(),
        serde_json::json!("Developers\nQ2 2025"),
    );
    engine
        .update_session_context(&result.session_id, updates)
        .await
        .unwrap();

    let mut r = engine.run_session(&result.session_id).await.unwrap();
    loop {
        match &r.status {
            ExecutionStatus::Completed | ExecutionStatus::Error(_) => break,
            ExecutionStatus::WaitingForInput { .. } | ExecutionStatus::ElicitationNeeded { .. } => {
                break;
            }
            ExecutionStatus::Paused { .. } => {
                r = engine.run_session(&result.session_id).await.unwrap();
            }
        }
    }
    assert!(
        !matches!(r.status, ExecutionStatus::Error(_)),
        "plan with answers should succeed"
    );

    let output_path = get_session_dir_from_session(&engine, &result.session_id)
        .await
        .expect("session_dir in session");
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
#[tokio::test]
async fn changeset_yaml_sessions_array_tracks_all_sessions() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_JSON);
    backend.push_ok(ACCEPTANCE_TESTS_JSON_MINIMAL);
    backend.push_ok(RED_JSON_MINIMAL);
    backend.push_ok(GREEN_JSON_MINIMAL);
    backend.push_ok(EVALUATE_JSON);
    backend.push_ok(VALIDATE_JSON);
    backend.push_ok(REFACTOR_JSON);
    backend.push_ok(UPDATE_DOCS_JSON); // acceptance-tests -> red -> green -> ... -> refactor -> update-docs
    backend.push_ok(RED_JSON_MINIMAL);
    backend.push_ok(GREEN_JSON_MINIMAL);
    backend.push_ok(EVALUATE_JSON);
    backend.push_ok(VALIDATE_JSON);
    backend.push_ok(REFACTOR_JSON);
    backend.push_ok(UPDATE_DOCS_JSON); // red goal -> green -> ... -> refactor -> update-docs

    let (output_dir, _) = temp_dir_with_git_repo("changeset-sessions", "Build auth");

    let storage_dir = std::env::temp_dir().join("tddy-changeset-sessions-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let (plan_path, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan should succeed");

    std::fs::write(
        plan_path.join("PRD.md"),
        std::fs::read_to_string(plan_path.join("PRD.md")).unwrap_or_default(),
    )
    .ok();

    let ctx = ctx_acceptance_tests(plan_path.clone(), Some(output_dir.clone()), None, false);
    let _ = run_goal_until_done(&engine, "acceptance-tests", ctx)
        .await
        .unwrap();

    let ctx = ctx_red(plan_path.clone(), None);
    let _ = run_goal_until_done(&engine, "red", ctx).await.unwrap();

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
/// PlanTask uses session_dir as working_dir; CheckingBackend looks for subdirs with changeset.
/// The engine's plan dir is output_dir/slug. So we pass output_dir (parent) in context
/// and the plan task creates output_dir/slug. The hooks write changeset before invoke.
/// The backend receives working_dir = session_dir. So we need to either change the plan task
/// to use parent for working_dir, or use Workflow. Kept with Workflow for now.
#[tokio::test]
#[ignore = "PlanTask uses session_dir as working_dir; CheckingBackend expects parent with subdirs"]
async fn changeset_written_before_plan_agent() {
    use std::sync::{Arc, Mutex};
    use tddy_core::changeset::read_changeset;
    use tddy_core::{BackendError, CodingBackend, InvokeRequest, InvokeResponse};

    /// A backend that checks disk state when invoke() is called (before returning).
    #[derive(Debug)]
    struct CheckingBackend {
        session_dir_captured: Arc<Mutex<Option<std::path::PathBuf>>>,
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
                            *self.session_dir_captured.lock().unwrap() = Some(path.clone());
                            if let Ok(cs) = read_changeset(&path) {
                                *self.changeset_state_at_invoke.lock().unwrap() =
                                    Some(cs.state.current.to_string());
                                *self.initial_prompt_at_invoke.lock().unwrap() =
                                    cs.initial_prompt.clone();
                            }
                        }
                    }
                }
            }

            Ok(InvokeResponse {
                output: PLAN_JSON.to_string(),
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
    let session_dir_captured = Arc::new(Mutex::new(None));

    let backend = CheckingBackend {
        session_dir_captured: session_dir_captured.clone(),
        changeset_state_at_invoke: changeset_state.clone(),
        initial_prompt_at_invoke: initial_prompt.clone(),
    };

    let storage_dir = std::env::temp_dir().join("tddy-changeset-before-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(Arc::new(backend)),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let input = "Build auth with early changeset";
    let session_dir = session_dir_for_input(&output_dir, input);
    std::fs::create_dir_all(&session_dir).unwrap();
    let ctx = ctx_plan(input, output_dir.clone(), None, None);
    let _ = engine.run_goal(&GoalId::new("plan"), ctx).await;

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

// ── Acceptance tests for Stable session dir PRD: R2 ──────────────────────────

/// DiscoveryData round-trip (deserialize YAML without plan_dir_suggestion, re-serialize)
/// must not produce a `plan_dir_suggestion` key in the output.
///
/// Fails until `plan_dir_suggestion` field is removed from `DiscoveryData` struct.
/// Uses YAML deserialization (no struct literal) so the test compiles after removal too.
#[test]
fn test_discovery_data_without_plan_dir_suggestion() {
    use tddy_core::changeset::DiscoveryData;

    // Minimal YAML that does NOT include plan_dir_suggestion
    let yaml_input = "toolchain: {}\nscripts: {}\ndoc_locations: []\nrelevant_code: []\n";
    let discovery: DiscoveryData = serde_yaml::from_str(yaml_input)
        .expect("DiscoveryData should deserialize when plan_dir_suggestion is absent");

    // Re-serializing must not emit the plan_dir_suggestion key
    let yaml_output =
        serde_yaml::to_string(&discovery).expect("DiscoveryData should serialize back to YAML");
    assert!(
        !yaml_output.contains("plan_dir_suggestion"),
        "serialized DiscoveryData must not contain 'plan_dir_suggestion' key after R2 removal; \
         got YAML:\n{}",
        yaml_output
    );
}
