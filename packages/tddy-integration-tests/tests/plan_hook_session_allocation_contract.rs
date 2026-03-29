//! Contract: session trees under `TDDY_SESSIONS_DIR/sessions/` are allocated only by CLI / daemon /
//! RPC (or test harnesses mimicking them). TDD `before_plan` must not add a second UUID directory
//! when common code has already created exactly one session folder.

mod common;

use std::sync::Arc;

use serial_test::serial;
use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::output::{create_session_dir_in, TDDY_SESSIONS_DIR_ENV};
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::session::workflow_engine_storage_dir;
use tddy_core::{GoalId, MockBackend, SharedBackend};

const PLANNING_OUTPUT: &str = r##"{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]}}"##;

fn count_dirs(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).count())
        .unwrap_or(0)
}

#[tokio::test]
#[serial]
async fn plan_goal_does_not_allocate_second_session_dir_under_sessions_root() {
    let base = common::unique_tddy_data_dir_for_test();
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let sessions_root = base.join("sessions");
    std::fs::create_dir_all(&sessions_root).unwrap();

    let pre_created = create_session_dir_in(&base).expect("common code creates session dir");
    assert_eq!(
        count_dirs(&sessions_root),
        1,
        "setup: exactly one session directory under sessions/"
    );

    let init_cs = Changeset {
        initial_prompt: Some("preloaded".to_string()),
        ..Changeset::default()
    };
    write_changeset(&pre_created, &init_cs).expect("write changeset");

    let repo = std::env::temp_dir().join(format!(
        "tddy-plan-hook-contract-repo-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();

    let prev_env = std::env::var(TDDY_SESSIONS_DIR_ENV).ok();
    std::env::set_var(TDDY_SESSIONS_DIR_ENV, base.to_str().unwrap());

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLANNING_OUTPUT);

    let storage_dir = workflow_engine_storage_dir(&pre_created);
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("Build auth from hook contract test"),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(&repo).unwrap(),
    );
    ctx.insert(
        "session_dir".to_string(),
        serde_json::to_value(&pre_created).unwrap(),
    );

    let result = engine
        .run_goal(&GoalId::new("plan"), ctx)
        .await
        .expect("plan goal run");

    let after_count = count_dirs(&sessions_root);

    match &prev_env {
        Some(v) => std::env::set_var(TDDY_SESSIONS_DIR_ENV, v),
        None => std::env::remove_var(TDDY_SESSIONS_DIR_ENV),
    }

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "plan should complete for this contract scenario: {:?}",
        result.status
    );

    assert_eq!(
        after_count, 1,
        "before_plan (and related hooks) must not create another directory under sessions/; \
         only the path pre-created by common entry code may exist"
    );

    let _ = std::fs::remove_dir_all(&repo);
    let _ = std::fs::remove_dir_all(&base);
}
