//! Acceptance tests: `exploration.md` session artifact (exploration-artifact changeset).
//!
//! Planning returns code-discovery knowledge in the plan submit `exploration` field; the engine
//! writes `session_dir/artifacts/exploration.md`, and post-interview steps advertise it via the
//! `<context-reminder>` header so downstream agents reuse the knowledge instead of re-exploring.
//! These tests encode intended behavior and fail until the implementation lands
//! (PRD: docs/ft/coder/exploration-artifact.md).

mod common;

use std::sync::Arc;

use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};

use common::{ctx_acceptance_tests, ctx_green, ctx_red, run_plan, temp_dir_with_git_repo};

/// Exploration document body as the plan agent would submit it: code map with
/// file:line:col references, a mermaid diagram, and documentation pointers.
const EXPLORATION_BODY: &str = "# Exploration\n\n## Code Map\n\n- `src/auth/mod.rs:42:5` — login entry point\n\n## Diagrams\n\n```mermaid\nflowchart TD\n  Login --> SessionStore\n```\n\n## Documentation\n\n- docs/ft/auth.md\n";

const PRD_BODY: &str = "# PRD\n## Summary\nAuth.\n\n## TODO\n\n- [ ] Task 1";

const ACCEPTANCE_TESTS_JSON: &str = r#"{"goal":"acceptance-tests","summary":"Created 1 test. Failing.","tests":[{"name":"test_foo","file":"src/foo.rs","line":1,"status":"failing"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test test_foo"}"#;

const RED_OUTPUT: &str = r#"{"goal":"red","summary":"Created 1 skeleton and 1 failing test.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"}]}"#;

const GREEN_OUTPUT_ALL_PASS: &str = r#"{"goal":"green","summary":"Implemented 1 method. All tests passing.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

fn plan_json_with_exploration() -> String {
    serde_json::json!({
        "goal": "plan",
        "name": "Auth Feature",
        "prd": PRD_BODY,
        "exploration": EXPLORATION_BODY,
        "branch_suggestion": "feature/auth",
        "worktree_suggestion": "feature-auth",
    })
    .to_string()
}

fn plan_json_without_exploration() -> String {
    serde_json::json!({
        "goal": "plan",
        "name": "Auth Feature",
        "prd": PRD_BODY,
        "branch_suggestion": "feature/auth",
        "worktree_suggestion": "feature-auth",
    })
    .to_string()
}

fn engine_with_backend(backend: Arc<MockBackend>, label: &str) -> WorkflowEngine {
    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-exploration-engine-{}-{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    )
}

fn write_exploration_artifact(session_dir: &std::path::Path) {
    std::fs::create_dir_all(session_dir.join("artifacts")).expect("create artifacts dir");
    std::fs::write(
        session_dir.join("artifacts").join("exploration.md"),
        EXPLORATION_BODY,
    )
    .expect("write exploration.md");
}

/// The plan submit `exploration` field is persisted by the engine as
/// `session_dir/artifacts/exploration.md`, preserving code refs and diagrams.
#[tokio::test]
async fn plan_submit_with_exploration_writes_exploration_md_under_artifacts() {
    // Given
    let output_dir =
        std::env::temp_dir().join(format!("tddy-exploration-plan-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(&plan_json_with_exploration());
    let engine = engine_with_backend(backend, "plan-writes");

    // When
    let (session_dir, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan should succeed");

    // Then
    let exploration_path = session_dir.join("artifacts").join("exploration.md");
    assert!(
        exploration_path.is_file(),
        "plan submit with an exploration field must write {}",
        exploration_path.display()
    );
    let content = std::fs::read_to_string(&exploration_path).expect("read exploration.md");
    assert!(
        content.contains("`src/auth/mod.rs:42:5` — login entry point"),
        "exploration.md must round-trip the submitted code map entry; got:\n{}",
        content
    );
    assert!(
        content.contains("```mermaid"),
        "exploration.md must round-trip the submitted mermaid diagram; got:\n{}",
        content
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Guard: a plan submit without the optional `exploration` field still succeeds,
/// writes the PRD, and does not fabricate an exploration.md.
#[tokio::test]
async fn plan_submit_without_exploration_writes_no_exploration_md() {
    // Given
    let output_dir = std::env::temp_dir().join(format!(
        "tddy-exploration-plan-none-out-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&output_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(&plan_json_without_exploration());
    let engine = engine_with_backend(backend, "plan-skips");

    // When
    let (session_dir, _) = run_plan(&engine, "Build auth", &output_dir, None)
        .await
        .expect("plan should succeed");

    // Then
    assert!(
        session_dir.join("artifacts").join("PRD.md").is_file(),
        "PRD.md must still be written under artifacts/"
    );
    assert!(
        !session_dir
            .join("artifacts")
            .join("exploration.md")
            .exists(),
        "no exploration.md must be written when the plan submit omits the field"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// The acceptance-tests prompt's `<context-reminder>` header advertises an existing
/// `artifacts/exploration.md` with its absolute path.
#[tokio::test]
async fn acceptance_tests_prompt_header_lists_exploration_md_when_present() {
    // Given
    let (output_dir, session_dir) = temp_dir_with_git_repo("exploration-hdr-at");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_exploration_artifact(&session_dir);
    common::write_changeset_for_session(&session_dir, "sess-exploration-at");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_JSON);
    let engine = engine_with_backend(backend.clone(), "hdr-at");

    // When
    let ctx = ctx_acceptance_tests(session_dir.clone(), Some(output_dir), None, false);
    let _ = engine
        .run_goal(&GoalId::new("acceptance-tests"), ctx)
        .await
        .unwrap();

    // Then
    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let prompt = &invocations[0].prompt;

    let exploration_line = prompt
        .lines()
        .find(|l| l.starts_with("exploration.md:"))
        .unwrap_or_else(|| {
            panic!(
                "acceptance-tests context header must list exploration.md when present; got:\n{}",
                prompt
            )
        });
    let path_str = exploration_line
        .trim_start_matches("exploration.md:")
        .trim();
    assert!(
        std::path::Path::new(path_str).is_absolute(),
        "exploration.md path in header must be absolute, got: {}",
        path_str
    );
    assert!(
        std::path::Path::new(path_str).exists(),
        "exploration.md path listed in header must exist on disk: {}",
        path_str
    );

    let _ = std::fs::remove_dir_all(session_dir.parent().unwrap());
}

/// The red prompt's `<context-reminder>` header advertises an existing
/// `artifacts/exploration.md`.
#[tokio::test]
async fn red_prompt_header_lists_exploration_md_when_present() {
    // Given
    let session_dir =
        std::env::temp_dir().join(format!("tddy-exploration-hdr-red-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests",
    )
    .expect("write acceptance-tests.md");
    write_exploration_artifact(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    let engine = engine_with_backend(backend.clone(), "hdr-red");

    // When
    let ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), ctx).await.unwrap();

    // Then
    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let prompt = &invocations[0].prompt;

    assert!(
        prompt.contains("exploration.md:"),
        "red context header must list exploration.md when present; got:\n{}",
        prompt
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}

/// The green prompt gets a `<context-reminder>` header (it has none today) and the
/// header advertises an existing `artifacts/exploration.md`.
#[tokio::test]
async fn green_prompt_starts_with_context_header_listing_exploration_md() {
    // Given
    let session_dir =
        std::env::temp_dir().join(format!("tddy-exploration-hdr-green-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    std::fs::write(session_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- auth_service_validates_email",
    )
    .expect("write acceptance-tests.md");
    write_exploration_artifact(&session_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    backend.push_ok(GREEN_OUTPUT_ALL_PASS);
    let engine = engine_with_backend(backend.clone(), "hdr-green");

    let red_ctx = ctx_red(session_dir.clone(), None);
    let _ = engine.run_goal(&GoalId::new("red"), red_ctx).await.unwrap();

    // When
    let green_ctx = ctx_green(session_dir.clone(), None, false);
    let _ = engine
        .run_goal(&GoalId::new("green"), green_ctx)
        .await
        .unwrap();

    // Then
    let invocations = backend.invocations();
    assert!(
        invocations.len() >= 2,
        "red then green should both invoke the backend; got {}",
        invocations.len()
    );
    let green_prompt = &invocations[1].prompt;

    assert!(
        green_prompt.starts_with("<context-reminder>"),
        "green prompt must start with a context-reminder header; got:\n{}",
        &green_prompt[..green_prompt.floor_char_boundary(300)]
    );
    assert!(
        green_prompt.contains("exploration.md:"),
        "green context header must list exploration.md when present; got:\n{}",
        green_prompt
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}
