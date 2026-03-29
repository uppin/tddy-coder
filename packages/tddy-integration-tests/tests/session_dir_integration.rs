//! Acceptance tests for stable $HOME/.tddy/sessions/{uuid} directory — PRD: Stable session dir.
//!
//! Tests 1–3 of the acceptance test plan:
//!   1. create_session_dir_in(base) creates base/sessions/{uuid}/ with valid UUID dirname
//!   2. Two calls produce different UUIDs
//!   3. Plan goal creates artifacts under a sessions/{uuid}/ path, not YYYY-MM-DD-slug/

mod common;

use std::sync::Arc;
use tddy_core::output::create_session_dir_in;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{GoalId, MockBackend, SharedBackend, WorkflowEngine};

/// Plan output as JSON (tddy-tools submit format).
const PLANNING_OUTPUT: &str = r##"{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]}}"##;

/// create_session_dir_in(base) creates base/sessions/{uuid}/ — directory exists and dirname is a
/// 36-char UUID with exactly 4 hyphens.
#[test]
fn test_create_session_dir_creates_under_base() {
    let tmp = std::env::temp_dir().join("tddy-session-dir-creates");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let session_dir = create_session_dir_in(&tmp).expect("create_session_dir_in should succeed");

    let sessions_parent = tmp.join("sessions");
    assert!(
        session_dir.starts_with(&sessions_parent),
        "session dir should be under {}/sessions/{{uuid}}/, got: {}",
        tmp.display(),
        session_dir.display()
    );

    let uuid_part = session_dir.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        uuid_part.len(),
        36,
        "session dirname should be a 36-char UUID, got: {}",
        uuid_part
    );
    // UUID v7 format: xxxxxxxx-xxxx-7xxx-xxxx-xxxxxxxxxxxx — 4 hyphens, lexicographic order ≈ time order
    assert_eq!(
        uuid_part.chars().filter(|&c| c == '-').count(),
        4,
        "UUID should contain exactly 4 hyphens, got: {}",
        uuid_part
    );
    assert!(
        session_dir.is_dir(),
        "session dir should be created on disk: {}",
        session_dir.display()
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Two calls to create_session_dir_in produce different paths (unique UUIDs).
#[test]
fn test_create_session_dir_returns_unique_ids() {
    let tmp = std::env::temp_dir().join("tddy-session-dir-unique");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let dir1 = create_session_dir_in(&tmp).expect("first create_session_dir_in call");
    let dir2 = create_session_dir_in(&tmp).expect("second create_session_dir_in call");

    assert_ne!(
        dir1, dir2,
        "each call should produce a unique session directory"
    );
    assert!(dir1.is_dir(), "first session dir should exist on disk");
    assert!(dir2.is_dir(), "second session dir should exist on disk");

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Running plan goal creates artifacts in a sessions/{uuid}/ path, not output_dir/YYYY-MM-DD-slug/.
#[tokio::test]
async fn test_plan_goal_uses_session_dir() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLANNING_OUTPUT);

    // Use a custom base dir so we don't pollute real $HOME/.tddy
    let base = std::env::temp_dir().join("tddy-plan-session-dir-test");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let storage_dir = std::env::temp_dir().join("tddy-plan-session-dir-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        common::tdd_recipe(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(common::tdd_recipe().create_hooks(None)),
    );

    // Entry layer creates `{session_base}/sessions/{uuid}/` before plan; workflow must not allocate.
    let session_dir = create_session_dir_in(&base).expect("pre-create session dir");
    let session_id = session_dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("session dirname")
        .to_string();
    let mut ctx = std::collections::HashMap::new();
    ctx.insert("feature_input".to_string(), serde_json::json!("Build auth"));
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(base.clone()).unwrap(),
    );
    ctx.insert("session_id".to_string(), serde_json::json!(session_id));
    ctx.insert(
        "session_dir".to_string(),
        serde_json::to_value(&session_dir).unwrap(),
    );

    let result = engine
        .run_goal(&GoalId::new("plan"), ctx)
        .await
        .expect("plan goal should not panic");

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "plan goal should succeed (not error) when session_base is provided: {:?}",
        result.status
    );

    // Get session_dir from session context after plan completes
    let session = engine
        .get_session(&result.session_id)
        .await
        .expect("get_session should work")
        .expect("session should exist");
    let ctx_session_dir: std::path::PathBuf = session
        .context
        .get_sync("session_dir")
        .expect("session_dir should be set in session context after plan goal");
    assert_eq!(
        ctx_session_dir, session_dir,
        "plan should keep the pre-created session_dir"
    );
    let session_dir = ctx_session_dir;

    // session_dir must be under base/sessions/{uuid}/
    let sessions_dir = base.join("sessions");
    assert!(
        session_dir.starts_with(&sessions_dir),
        "plan dir should be under {}/sessions/{{uuid}}/, got: {}",
        base.display(),
        session_dir.display()
    );

    // UUID dirname: 36 chars with 4 hyphens
    let uuid_part = session_dir.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        uuid_part.len(),
        36,
        "plan session dirname should be a 36-char UUID, got: {}",
        uuid_part
    );

    // changeset.yaml must exist in the session dir
    assert!(
        session_dir.join("changeset.yaml").exists(),
        "changeset.yaml should exist in session dir: {}",
        session_dir.display()
    );

    // Must NOT use old YYYY-MM-DD-slug format
    let today_prefix = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert!(
        !session_dir.to_string_lossy().contains(&today_prefix),
        "plan dir must not use old YYYY-MM-DD-slug format, got: {}",
        session_dir.display()
    );

    let _ = std::fs::remove_dir_all(&base);
}
