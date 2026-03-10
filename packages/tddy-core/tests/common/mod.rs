//! Shared helpers for WorkflowEngine integration tests.

use std::collections::HashMap;
use std::path::PathBuf;
use tddy_core::output::slugify_directory_name;
use tddy_core::workflow::graph::{ExecutionResult, ExecutionStatus};
use tddy_core::WorkflowEngine;

/// Plan dir path for a given input (matches Workflow::plan behavior).
pub fn plan_dir_for_input(parent: &std::path::Path, input: &str) -> PathBuf {
    parent.join(slugify_directory_name(input))
}

/// Build context for plan goal.
/// output_dir is the repo root (parent of plan_dir); agent runs in output_dir to discover Cargo.toml, etc.
/// plan_dir is output_dir/slug; defaults to output_dir.join(slugify(feature_input)) if not provided.
pub fn ctx_plan(
    feature_input: &str,
    output_dir: PathBuf,
    answers: Option<&str>,
    conversation_output_path: Option<PathBuf>,
) -> HashMap<String, serde_json::Value> {
    let plan_dir = plan_dir_for_input(&output_dir, feature_input);
    let mut m = HashMap::new();
    m.insert(
        "feature_input".to_string(),
        serde_json::json!(feature_input),
    );
    m.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir).unwrap(),
    );
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    if let Some(p) = conversation_output_path {
        m.insert(
            "conversation_output_path".to_string(),
            serde_json::to_value(p).unwrap(),
        );
    }
    m
}

/// Build context for acceptance-tests goal.
/// run_demo: when true, green will transition to demo (used when running full chain).
pub fn ctx_acceptance_tests(
    plan_dir: PathBuf,
    answers: Option<&str>,
    run_demo: bool,
) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    m.insert("run_demo".to_string(), serde_json::json!(run_demo));
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    m
}

/// Build context for red goal.
pub fn ctx_red(plan_dir: PathBuf, answers: Option<&str>) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    m
}

/// Build context for green goal.
pub fn ctx_green(
    plan_dir: PathBuf,
    answers: Option<&str>,
    run_demo: bool,
) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    m.insert("run_demo".to_string(), serde_json::json!(run_demo));
    if let Some(a) = answers {
        m.insert("answers".to_string(), serde_json::json!(a));
    }
    m
}

/// Build context for evaluate goal.
/// output_dir is the project root (for Cargo.toml discovery); defaults to plan_dir if None.
pub fn ctx_evaluate(
    plan_dir: PathBuf,
    output_dir: Option<PathBuf>,
) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir.clone()).unwrap(),
    );
    m.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir.unwrap_or(plan_dir)).unwrap(),
    );
    m
}

/// Build context for validate goal.
pub fn ctx_validate(plan_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    m
}

/// Build context for refactor goal.
pub fn ctx_refactor(plan_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    m
}

/// Build context for update-docs goal.
pub fn ctx_update_docs(plan_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    m
}

/// Build context for demo goal.
pub fn ctx_demo(plan_dir: PathBuf) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir).unwrap(),
    );
    m
}

/// Get plan_dir from session context after a run. Re-exported for test use.
pub async fn get_plan_dir_from_session(
    engine: &WorkflowEngine,
    session_id: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let session = engine
        .get_session(session_id)
        .await?
        .ok_or("Session not found")?;
    session
        .context
        .get_sync("plan_dir")
        .or_else(|| session.context.get_sync("output_dir"))
        .ok_or("plan_dir not in session context".into())
}

/// Run plan and return (plan_dir, session_id). Returns Err on WaitingForInput or Error.
pub async fn run_plan(
    engine: &WorkflowEngine,
    input: &str,
    output_dir: &std::path::Path,
    answers: Option<&str>,
) -> Result<(PathBuf, String), Box<dyn std::error::Error + Send + Sync>> {
    run_plan_with_conversation_output(engine, input, output_dir, answers, None).await
}

/// Run plan with optional conversation_output_path for raw agent output.
pub async fn run_plan_with_conversation_output(
    engine: &WorkflowEngine,
    input: &str,
    output_dir: &std::path::Path,
    answers: Option<&str>,
    conversation_output_path: Option<PathBuf>,
) -> Result<(PathBuf, String), Box<dyn std::error::Error + Send + Sync>> {
    let plan_dir = plan_dir_for_input(output_dir, input);
    std::fs::create_dir_all(&plan_dir)?;

    let context = ctx_plan(
        input,
        output_dir.to_path_buf(),
        answers,
        conversation_output_path,
    );
    let result = engine.run_goal("plan", context).await?;

    match &result.status {
        ExecutionStatus::WaitingForInput { .. } => return Err("ClarificationNeeded".into()),
        ExecutionStatus::Error(e) => return Err(e.clone().into()),
        _ => {}
    }

    let plan_dir = get_plan_dir_from_session(engine, &result.session_id).await?;
    Ok((plan_dir, result.session_id))
}

/// Write a minimal changeset.yaml with Planned state for a plan session.
pub fn write_changeset_for_plan_session(plan_dir: &std::path::Path, session_id: &str) {
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

/// Write a minimal changeset.yaml with custom state.
pub fn write_changeset_with_state(plan_dir: &std::path::Path, state: &str, session_id: &str) {
    let changeset = format!(
        r#"version: 1
models: {{}}
sessions:
  - id: "{}"
    agent: claude
    tag: plan
    created_at: "2026-03-07T10:00:00Z"
state:
  current: {}
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {{}}
"#,
        session_id, state
    );
    std::fs::write(plan_dir.join("changeset.yaml"), changeset).expect("write changeset");
}

/// Write a minimal evaluation-report.md to plan_dir (for validate goal prerequisite).
pub fn write_evaluation_report_to_plan_dir(plan_dir: &std::path::Path) {
    let content = r#"# Evaluation Report

## Summary

Evaluated 3 changed files. Risk level: medium.

## Risk Level

medium

## Changed Files

- src/main.rs (modified, +15/-3)
- src/lib.rs (modified, +5/-0)
- tests/main_test.rs (added, +40/-0)

## Affected Tests

- tests/main_test.rs: created
- tests/integration_test.rs: updated

## Validity Assessment

The change is valid for the intended use-case.
"#;
    std::fs::write(plan_dir.join("evaluation-report.md"), content)
        .expect("write evaluation-report.md");
}

/// Write refactoring-plan.md to plan_dir (for refactor goal prerequisite).
pub fn write_refactoring_plan(plan_dir: &std::path::Path) {
    let content = r#"# Refactoring Plan

## Priority: Critical

1. **Rename Goal::ValidateRefactor to Goal::Validate**
   - Scope: backend/mod.rs, workflow/mod.rs
   - Estimated effort: small

## Priority: High

2. **Rename internal types**
   - Scope: workflow/mod.rs, lib.rs
   - Estimated effort: medium
"#;
    std::fs::write(plan_dir.join("refactoring-plan.md"), content)
        .expect("write refactoring-plan.md");
}

/// Run a goal and return ExecutionResult. Loop on Paused until Completed/Error/WaitingForInput.
pub async fn run_goal_until_done(
    engine: &WorkflowEngine,
    goal: &str,
    context: HashMap<String, serde_json::Value>,
) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut result = engine.run_goal(goal, context.clone()).await?;
    loop {
        match &result.status {
            ExecutionStatus::Completed | ExecutionStatus::Error(_) => return Ok(result),
            ExecutionStatus::WaitingForInput { .. } => return Ok(result),
            ExecutionStatus::ElicitationNeeded { .. } => return Ok(result),
            ExecutionStatus::Paused { .. } => {
                result = engine.run_session(&result.session_id).await?;
            }
        }
    }
}
