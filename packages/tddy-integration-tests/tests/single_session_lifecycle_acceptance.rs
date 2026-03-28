//! Single-session lifecycle acceptance tests (PRD: unified session id + one directory).
//!
//! These encode the target contract; they are expected to fail until production code
//! stops allocating extra session directories and preserves the startup `session_id`
//! in engine context (including when the backend returns a different agent session id).

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serial_test::serial;
use tddy_core::backend::{GoalId, InvokeResponse};
use tddy_core::output::{
    create_session_dir_under, create_session_dir_with_id, TDDY_SESSIONS_DIR_ENV,
};
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{MockBackend, SharedBackend, WorkflowEngine};

/// Plan output as JSON (tddy-tools submit format).
const PLANNING_OUTPUT: &str = r##"{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]}}"##;

/// Startup session id (UUID v7-shaped) bound before plan — directory name must match.
const STARTUP_SESSION_ID: &str = "019d357e-48ee-7c11-bd44-a967873f58b2";

/// Agent/backend thread id — must not replace the process session id in context (PRD).
const BACKEND_AGENT_SESSION_ID: &str = "00000000-0000-7000-8000-000000000099";

fn count_dir_children(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .ok()
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

/// **plan_task_reuses_session_dir_when_present**: process `session_id` stays the filesystem
/// session id when `session_dir` is already bound; backend agent id must not replace it.
#[tokio::test]
async fn plan_task_reuses_session_dir_when_present() {
    let backend = Arc::new(MockBackend::new());
    backend.push_response(Ok(InvokeResponse {
        output: PLANNING_OUTPUT.to_string(),
        exit_code: 0,
        session_id: Some(BACKEND_AGENT_SESSION_ID.to_string()),
        questions: vec![],
        raw_stream: None,
        stderr: None,
    }));

    let base = std::env::temp_dir().join(format!("tddy-plan-task-reuse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let session_dir = create_session_dir_with_id(&base, STARTUP_SESSION_ID).expect("session dir");
    let init_cs = tddy_core::changeset::Changeset {
        initial_prompt: Some("feature".to_string()),
        ..Default::default()
    };
    let _ = tddy_core::changeset::write_changeset(&session_dir, &init_cs);

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-plan-task-reuse-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let mut ctx = HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("Build auth single-session"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir.clone()).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_id".to_string(),
        serde_json::json!(STARTUP_SESSION_ID),
    );

    let result = engine
        .run_goal(&GoalId::new("plan"), ctx)
        .await
        .expect("plan goal");

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "plan should succeed: {:?}",
        result.status
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .expect("get_session")
        .expect("session exists");
    let ctx_sid: String = session
        .context
        .get_sync("session_id")
        .expect("session_id must remain in engine context after plan");

    assert_eq!(
        ctx_sid, STARTUP_SESSION_ID,
        "engine context session_id must remain the startup / filesystem session id, \
         not the backend agent id ({BACKEND_AGENT_SESSION_ID})"
    );
    let sd: PathBuf = session
        .context
        .get_sync("session_dir")
        .expect("session_dir in context");
    assert_eq!(
        sd.file_name().and_then(|n| n.to_str()),
        Some(STARTUP_SESSION_ID),
        "session_dir basename must equal startup session id"
    );
    assert_eq!(
        count_dir_children(&base.join("sessions")),
        1,
        "exactly one session directory under session_base/sessions/"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// **before_plan_does_not_allocate_second_dir_when_session_id_bound**: hooks + PlanTask when
/// `session_id` and `session_base` are set must not lose the startup id when the backend returns a
/// different agent thread id. Context omits `session_dir` so `before_plan` and `PlanTask` resolve it.
#[tokio::test]
#[serial]
async fn before_plan_does_not_allocate_second_dir_when_session_id_bound() {
    let isolated_home =
        std::env::temp_dir().join(format!("tddy-hooks-no-second-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&isolated_home);
    std::fs::create_dir_all(&isolated_home).unwrap();

    let backend = Arc::new(MockBackend::new());
    backend.push_response(Ok(InvokeResponse {
        output: PLANNING_OUTPUT.to_string(),
        exit_code: 0,
        session_id: Some(BACKEND_AGENT_SESSION_ID.to_string()),
        questions: vec![],
        raw_stream: None,
        stderr: None,
    }));

    let base = isolated_home.join("workflow-base");
    std::fs::create_dir_all(&base).unwrap();

    let storage_dir =
        std::env::temp_dir().join(format!("tddy-hooks-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let mut ctx = HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("Bound session id feature"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_id".to_string(),
        serde_json::json!(STARTUP_SESSION_ID),
    );

    let before_global = count_dir_children(&isolated_home.join("sessions"));

    std::env::set_var(TDDY_SESSIONS_DIR_ENV, isolated_home.as_os_str());
    let result = engine
        .run_goal(&GoalId::new("plan"), ctx)
        .await
        .expect("plan");
    let after_global = count_dir_children(&isolated_home.join("sessions"));

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "plan should succeed: {:?}",
        result.status
    );

    assert_eq!(
        before_global, after_global,
        "before_plan / plan path must not call new_session_dir() into TDDY_SESSIONS_DIR \
         when session_id was bound (would create an extra directory)"
    );
    assert_eq!(
        count_dir_children(&base.join("sessions")),
        1,
        "exactly one directory under workflow session_base/sessions/"
    );

    let session = engine
        .get_session(&result.session_id)
        .await
        .expect("get_session")
        .expect("session exists");
    let ctx_sid: String = session
        .context
        .get_sync("session_id")
        .expect("session_id in context");
    assert_eq!(
        ctx_sid, STARTUP_SESSION_ID,
        "startup session_id must not be replaced by backend agent id after before_plan + PlanTask"
    );

    std::env::remove_var(TDDY_SESSIONS_DIR_ENV);
    let _ = std::fs::remove_dir_all(&isolated_home);
}

/// **workflow_runner_avoids_new_session_dir_fallback_after_successful_plan**: after a full
/// workflow run establishes a session directory, the sessions tree must not gain an extra
/// anonymous UUID folder (no silent `new_session_dir` fallback for the same run). Process
/// `session_id` must stay the canonical id even when the backend reports another.
#[tokio::test]
async fn workflow_runner_avoids_new_session_dir_fallback_after_successful_plan() {
    let backend = Arc::new(MockBackend::new());
    backend.push_response(Ok(InvokeResponse {
        output: PLANNING_OUTPUT.to_string(),
        exit_code: 0,
        session_id: Some(BACKEND_AGENT_SESSION_ID.to_string()),
        questions: vec![],
        raw_stream: None,
        stderr: None,
    }));

    let base = std::env::temp_dir().join(format!("tddy-wf-runner-fallback-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let mut ctx = HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("SKIP_QUESTIONS full workflow session contract"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_id".to_string(),
        serde_json::json!(STARTUP_SESSION_ID),
    );

    let storage_dir = std::env::temp_dir().join(format!(
        "tddy-wf-runner-fallback-engine-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    let before = count_dir_children(&base.join("sessions"));
    let result = engine
        .run_full_workflow(ctx)
        .await
        .expect("run_full_workflow");
    let after = count_dir_children(&base.join("sessions"));

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "workflow should not error: {:?}",
        result.status
    );

    assert_eq!(
        after,
        before + 1,
        "full workflow must create exactly one new directory under session_base/sessions/ \
         (not an extra fallback directory)"
    );
    assert_eq!(
        after, 1,
        "there must be exactly one session subdirectory for this run (no duplicate UUID dirs)"
    );

    let session = engine.get_session(&result.session_id).await.ok().flatten();
    let has_dir = session
        .as_ref()
        .and_then(|s| s.context.get_sync::<PathBuf>("session_dir"))
        .is_some();
    assert!(
        has_dir,
        "engine session context must carry session_dir after full workflow (so presenters never need new_session_dir fallback)"
    );
    let ctx_sid: String = session
        .expect("session after full workflow")
        .context
        .get_sync("session_id")
        .expect("session_id in context");
    assert_eq!(
        ctx_sid, STARTUP_SESSION_ID,
        "full workflow must preserve startup session_id (not backend's agent session id)"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// **cli_invocation_creates_single_sessions_subtree**: the CLI session directory for a run must be
/// `{TDDY_SESSIONS_DIR}/sessions/<session_id>/` (same as [`create_session_dir_with_id`]), never a bare
/// `{TDDY_SESSIONS_DIR}/<session_id>/` path.
#[test]
fn cli_invocation_creates_single_sessions_subtree() {
    let base = std::env::temp_dir().join(format!(
        "tddy-cli-single-session-contract-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let sid = uuid::Uuid::now_v7().to_string();

    let wrong_layout = create_session_dir_under(&base, &sid).unwrap();
    let cli_layout = create_session_dir_with_id(&base, &sid).unwrap();

    assert_eq!(
        wrong_layout, cli_layout,
        "CLI must resolve session_dir to the same path as create_session_dir_with_id (sessions/{{id}})"
    );

    let _ = std::fs::remove_dir_all(&base);
}
