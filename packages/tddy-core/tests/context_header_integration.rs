//! Integration tests verifying that every agent prompt is prepended with a
//! context header wrapped in `<context-reminder>` when .md artifacts exist
//! in the plan directory.
//!
//! Migrated from Workflow to WorkflowEngine.

mod common;

use std::sync::Arc;
use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
use tddy_core::{MockBackend, SharedBackend, WorkflowEngine};

use common::{ctx_acceptance_tests, ctx_red, run_plan, write_changeset_for_plan_session};

const ACCEPTANCE_TESTS_OUTPUT: &str = r#"<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Created 1 test. Failing.","tests":[{"name":"test_foo","file":"src/foo.rs","line":1,"status":"failing"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test test_foo"}
</structured-response>"#;

const RED_OUTPUT: &str = r#"<structured-response content-type="application-json">
{"goal":"red","summary":"Created 1 skeleton.","tests":[{"name":"test_foo","file":"src/foo.rs","line":1,"status":"failing"}],"skeletons":[{"name":"Foo","file":"src/foo.rs","line":1,"kind":"struct"}]}
</structured-response>"#;

const PLAN_OUTPUT: &str = r#"---PRD_START---
# PRD

## Summary
A feature.

## Testing Plan
### Acceptance Tests
- test_foo
---PRD_END---

---TODO_START---
- [ ] Task
---TODO_END---"#;

// ── AC1: prompt starts with marker when artifacts exist ───────────────────────

/// Acceptance-tests prompt must start with the context header when PRD.md exists.
#[tokio::test]
async fn acceptance_tests_prompt_includes_context_header_when_prd_exists() {
    let plan_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-prd");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-ctx-hdr-1");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let _ = engine.run_goal("acceptance-tests", ctx).await.unwrap();

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let prompt = &invocations[0].prompt;

    assert!(
        prompt.starts_with("<context-reminder>"),
        "acceptance-tests prompt must start with context-reminder tag, got:\n{}",
        &prompt[..prompt.len().min(300)]
    );
    assert!(
        prompt.contains("**CRITICAL FOR CONTEXT AND SUMMARY**"),
        "acceptance-tests prompt must contain marker inside context-reminder"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

// ── AC4: paths in header are absolute ─────────────────────────────────────────

/// Context header must list PRD.md with an absolute path.
#[tokio::test]
async fn acceptance_tests_prompt_header_lists_prd_md_with_absolute_path() {
    let plan_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-abs");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-ctx-hdr-2");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-abs-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let _ = engine.run_goal("acceptance-tests", ctx).await.unwrap();

    let invocations = backend.invocations();
    let prompt = &invocations[0].prompt;

    assert!(
        prompt.contains("PRD.md:"),
        "header must contain a PRD.md: entry"
    );

    let prd_line = prompt
        .lines()
        .find(|l| l.starts_with("PRD.md:"))
        .expect("header must contain a PRD.md line");
    let path_str = prd_line.trim_start_matches("PRD.md:").trim();

    assert!(
        std::path::Path::new(path_str).is_absolute(),
        "PRD.md path in header must be absolute, got: {}",
        path_str
    );
    assert!(
        std::path::Path::new(path_str).exists(),
        "path listed in header must exist on disk: {}",
        path_str
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

// ── AC3: missing artifacts not listed ─────────────────────────────────────────

/// Context header must NOT mention artifacts that don't exist yet.
#[tokio::test]
async fn acceptance_tests_prompt_header_omits_missing_artifacts() {
    let plan_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-omit");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-ctx-hdr-3");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-omit-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let _ = engine.run_goal("acceptance-tests", ctx).await.unwrap();

    let invocations = backend.invocations();
    let prompt = &invocations[0].prompt;

    assert!(
        !prompt.contains("TODO.md:"),
        "header must NOT list TODO.md (not a known artifact)"
    );
    assert!(
        !prompt.contains("acceptance-tests.md:"),
        "header must NOT list acceptance-tests.md when it does not exist"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

// ── header format: blank line separates header from prompt body ───────────────

/// Header block must be followed by a blank line before the original prompt content.
#[tokio::test]
async fn acceptance_tests_prompt_header_separated_from_body_by_blank_line() {
    let plan_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-blank");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    write_changeset_for_plan_session(&plan_dir, "sess-ctx-hdr-blank");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-ctx-hdr-at-blank-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_acceptance_tests(plan_dir.clone(), None, false);
    let _ = engine.run_goal("acceptance-tests", ctx).await.unwrap();

    let invocations = backend.invocations();
    let prompt = &invocations[0].prompt;

    assert!(
        prompt.contains("</context-reminder>"),
        "prompt must contain closing context-reminder tag"
    );

    let close_pos = prompt.find("</context-reminder>").unwrap();
    let after_tag = &prompt[close_pos + "</context-reminder>".len()..];
    assert!(
        after_tag.starts_with('\n') || after_tag.starts_with("\n\n"),
        "closing tag must be followed by newline before prompt body"
    );

    let header_section = &prompt[..close_pos];
    assert!(
        header_section.starts_with("<context-reminder>"),
        "header section must start with context-reminder tag"
    );
    assert!(
        header_section.contains("**CRITICAL FOR CONTEXT AND SUMMARY**"),
        "header section must contain marker inside tags"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

// ── AC1: red phase prompt includes header with multiple artifacts ─────────────

/// Red phase prompt must include context header listing PRD.md and acceptance-tests.md.
#[tokio::test]
async fn red_prompt_includes_context_header_with_multiple_artifacts() {
    let plan_dir = std::env::temp_dir().join("tddy-ctx-hdr-red");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(plan_dir.join("acceptance-tests.md"), "# Acceptance Tests")
        .expect("write acceptance-tests.md");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(RED_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-ctx-hdr-red-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let ctx = ctx_red(plan_dir.clone(), None);
    let _ = engine.run_goal("red", ctx).await.unwrap();

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let prompt = &invocations[0].prompt;

    assert!(
        prompt.starts_with("<context-reminder>"),
        "red prompt must start with context-reminder tag"
    );
    assert!(
        prompt.contains("**CRITICAL FOR CONTEXT AND SUMMARY**"),
        "red prompt must contain marker inside context-reminder"
    );
    assert!(
        prompt.contains("PRD.md:"),
        "red prompt header must list PRD.md"
    );
    assert!(
        prompt.contains("acceptance-tests.md:"),
        "red prompt header must list acceptance-tests.md"
    );

    let _ = std::fs::remove_dir_all(&plan_dir);
}

// ── AC2: plan goal with fresh empty dir has no header ─────────────────────────

/// Plan prompt with a fresh (empty) output dir must NOT have a context header,
/// since no .md artifacts exist at prompt-construction time.
#[tokio::test]
async fn plan_prompt_has_no_context_header_for_empty_output_dir() {
    let output_dir = std::env::temp_dir().join("tddy-ctx-hdr-plan-empty");
    let _ = std::fs::remove_dir_all(&output_dir);

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLAN_OUTPUT);

    let storage_dir = std::env::temp_dir().join("tddy-ctx-hdr-plan-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let _ = run_plan(&engine, "A feature", &output_dir, None)
        .await
        .expect("plan should succeed");

    let invocations = backend.invocations();
    assert!(!invocations.is_empty(), "backend should have been invoked");
    let prompt = &invocations[0].prompt;

    assert!(
        !prompt.starts_with("<context-reminder>"),
        "plan prompt with empty output dir must NOT have context-reminder block"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}
