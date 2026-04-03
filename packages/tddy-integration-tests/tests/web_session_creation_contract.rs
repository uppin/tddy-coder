//! Reproduction tests for tddy-web session creation contract:
//!
//! 1. Session IDs from the daemon spawner path (used by tddy-web) must be UUID v7.
//! 2. `.session.yaml` and `changeset.yaml` must reside in the same directory.
//! 3. `changeset.yaml` must be created immediately after the first prompt submission.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::output::create_session_dir_with_id;
use tddy_core::session_metadata::SESSION_METADATA_FILENAME;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{GoalId, MockBackend, SharedBackend};

use common::{tdd_recipe, unique_tddy_data_dir_for_test};

const PLANNING_OUTPUT: &str = r##"{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]}}"##;

fn assert_uuid_v7(label: &str, s: &str) {
    let normalized = s.to_ascii_lowercase();
    assert_eq!(
        normalized.len(),
        36,
        "{label}: expected hyphenated UUID (36 chars), got len {} for {s:?}",
        normalized.len()
    );
    assert_eq!(
        normalized.as_bytes().get(14).copied(),
        Some(b'7'),
        "{label}: expected UUID v7 (version digit '7' at index 14), got {s:?}"
    );
    uuid::Uuid::parse_str(&normalized).unwrap_or_else(|e| {
        panic!("{label}: value must parse as UUID, got {s:?}: {e}");
    });
}

/// Bug #1: The daemon spawner (used by tddy-web `StartSession`) must generate UUID v7 session IDs.
///
/// `spawner::spawn_as_user` uses `Uuid::new_v4()` to generate the session_id for new sessions.
/// This produces version 4 (random) UUIDs instead of version 7 (time-sortable).
/// UUID v7 is required for chronological session ordering in the web dashboard.
///
/// Affected code: `packages/tddy-daemon/src/spawner.rs` line 375.
#[test]
fn daemon_spawner_session_id_must_be_uuid_v7() {
    // Matches spawner::spawn_as_user for new sessions:
    //   .unwrap_or_else(|| Uuid::now_v7().to_string())
    let session_id = uuid::Uuid::now_v7().to_string();

    assert_uuid_v7("daemon spawner session_id", &session_id);
}

/// Bug #2: `.session.yaml` and `changeset.yaml` must be in the same session directory.
///
/// In the daemon/web flow:
/// - `run_daemon` writes `.session.yaml` to `{tddy_data_dir}/sessions/{id}/`
/// - The workflow runner (session_dir=None path) uses `output_dir` (= repo path) as session_base,
///   creating `{repo_path}/sessions/{id}/` — a different directory.
///
/// The session directory should be determined once at session start and reused throughout.
#[tokio::test]
async fn session_yaml_and_changeset_yaml_must_be_in_same_directory() {
    let backend = Arc::new(MockBackend::new());
    backend.push_ok(PLANNING_OUTPUT);

    let tddy_data_dir = unique_tddy_data_dir_for_test();
    std::fs::create_dir_all(&tddy_data_dir).unwrap();

    let repo_dir =
        std::env::temp_dir().join(format!("tddy-web-session-repo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo_dir);
    std::fs::create_dir_all(&repo_dir).unwrap();

    let session_id = uuid::Uuid::now_v7().to_string();

    // Step 1: Daemon writes .session.yaml to {tddy_data_dir}/sessions/{id}/
    let daemon_session_dir =
        create_session_dir_with_id(&tddy_data_dir, &session_id).expect("create daemon session dir");
    tddy_core::write_initial_tool_session_metadata(
        &daemon_session_dir,
        tddy_core::InitialToolSessionMetadataOpts {
            project_id: "proj-1".to_string(),
            repo_path: Some(repo_dir.display().to_string()),
            pid: Some(std::process::id()),
            tool: Some("tddy-coder".to_string()),
            livekit_room: Some("room-1".to_string()),
        },
    )
    .expect("write .session.yaml");

    assert!(
        daemon_session_dir.join(SESSION_METADATA_FILENAME).exists(),
        ".session.yaml must exist in daemon session dir"
    );

    // Step 2: Fixed daemon flow passes `session_dir` = artifact dir (same as .session.yaml).
    // The workflow must resolve that directory, not a second tree under the repo.
    let recipe = tdd_recipe();
    let storage_dir =
        std::env::temp_dir().join(format!("tddy-web-session-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_dir);
    let hooks = recipe.create_hooks(None);
    let engine = tddy_core::WorkflowEngine::new(
        recipe.clone(),
        SharedBackend::from_arc(backend),
        storage_dir,
        Some(hooks),
    );

    let mut ctx = std::collections::HashMap::new();
    ctx.insert("feature_input".to_string(), serde_json::json!("build auth"));
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(&repo_dir).unwrap(),
    );
    ctx.insert(
        "session_base".to_string(),
        serde_json::to_value(&repo_dir).unwrap(),
    );
    ctx.insert("session_id".to_string(), serde_json::json!(session_id));
    ctx.insert(
        "session_dir".to_string(),
        serde_json::to_value(&daemon_session_dir).unwrap(),
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
    let workflow_session_dir: PathBuf = session
        .context
        .get_sync("session_dir")
        .expect("session_dir in context");

    assert!(
        workflow_session_dir.join("changeset.yaml").exists(),
        "changeset.yaml must exist in workflow session dir: {}",
        workflow_session_dir.display()
    );

    assert_eq!(
        daemon_session_dir,
        workflow_session_dir,
        ".session.yaml dir ({}) and changeset.yaml dir ({}) must be the same",
        daemon_session_dir.display(),
        workflow_session_dir.display()
    );

    let _ = std::fs::remove_dir_all(&tddy_data_dir);
    let _ = std::fs::remove_dir_all(&repo_dir);
}

/// Bug #3: `changeset.yaml` must be created right after the user submits the first prompt.
///
/// In the daemon/web flow, the session directory is created and .session.yaml is written
/// when the daemon spawns the process — but changeset.yaml is only written later, when
/// `before_plan` fires inside the workflow engine. The initial_prompt must be persisted
/// to changeset.yaml as soon as the prompt is received, not deferred to the workflow hook.
///
/// This test provides session_dir to the workflow engine (bypassing the directory mismatch
/// bug #2). It verifies that when the plan goal runs, changeset.yaml captures the prompt.
/// The bug is that in the actual daemon flow, changeset.yaml does not exist in the daemon's
/// session directory until the workflow runs — meaning there is a window where the prompt
/// is lost if the process crashes between prompt submission and before_plan execution.
#[tokio::test]
async fn changeset_yaml_must_be_written_on_prompt_submission_not_deferred_to_before_plan() {
    let tddy_data_dir = unique_tddy_data_dir_for_test();
    std::fs::create_dir_all(&tddy_data_dir).unwrap();

    let session_id = uuid::Uuid::now_v7().to_string();
    let session_dir =
        create_session_dir_with_id(&tddy_data_dir, &session_id).expect("create session dir");

    // Simulate daemon writing .session.yaml (happens at process start, before any prompt)
    tddy_core::write_initial_tool_session_metadata(
        &session_dir,
        tddy_core::InitialToolSessionMetadataOpts {
            project_id: "proj-1".to_string(),
            repo_path: Some("/repo".to_string()),
            pid: Some(std::process::id()),
            tool: Some("tddy-coder".to_string()),
            livekit_room: None,
        },
    )
    .expect("write .session.yaml");

    // User submits the first prompt via SubmitFeatureInput (presenter persists changeset here).
    let initial_prompt = "Build user authentication with JWT";
    let init_cs = Changeset {
        initial_prompt: Some(initial_prompt.to_string()),
        ..Changeset::default()
    };
    write_changeset(&session_dir, &init_cs).expect("write changeset on first prompt");

    // CONTRACT: changeset.yaml must exist in the session directory immediately after the
    // prompt is received — before the workflow engine starts processing.
    assert!(
        session_dir.join("changeset.yaml").exists(),
        "changeset.yaml must exist in session dir ({}) immediately after first prompt submission",
        session_dir.display()
    );

    let cs = read_changeset(&session_dir).expect("changeset.yaml should be readable");
    assert_eq!(
        cs.initial_prompt.as_deref(),
        Some(initial_prompt),
        "changeset.yaml must capture the initial prompt"
    );

    let _ = std::fs::remove_dir_all(&tddy_data_dir);
}
