//! A workflow session's spawn records the usage context the daemon later accounts it from: which
//! agent it runs, the OS user's home its child runs with, and its working tree.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-usage-binding-daemon-stream.md

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use tddy_spawn::spawner::{self, LiveKitCreds, SpawnOptions};
use tddy_usage::{read_usage_context, UsageContextFile};

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to resolve the target account")
}

fn a_livekit() -> LiveKitCreds {
    LiveKitCreds {
        url: "ws://127.0.0.1:7880".to_string(),
        api_key: "test-key".to_string(),
        api_secret: "test-secret".to_string(),
        common_room: None,
        daemon_instance_id: None,
    }
}

fn a_tool_that_stays_alive(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("fake-tddy-coder.sh");
    std::fs::write(&path, "#!/bin/sh\nsleep 5\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Plan a session child for `opts` and return the usage context its session directory holds.
fn recorded_context(opts: SpawnOptions<'_>, session_id: &str) -> Option<UsageContextFile> {
    let repo = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = a_tool_that_stays_alive(tools.path());
    let plan = spawner::plan_session_child(
        &current_username(),
        tool.to_str().unwrap(),
        data_dir.path(),
        repo.path(),
        &a_livekit(),
        opts,
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
    )
    .expect("plan the session child");
    assert_eq!(plan.session_id, session_id);
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(data_dir.path(), session_id);
    read_usage_context(&session_dir)
}

#[test]
fn a_workflow_spawn_records_its_agent_and_the_os_users_home() {
    // When
    let context = recorded_context(
        SpawnOptions {
            new_session_id: Some("workflow-a"),
            agent: Some("acme"),
            model: Some("acme-model"),
            ..SpawnOptions::default()
        },
        "workflow-a",
    );

    // Then
    let context = context.expect("the spawn recorded a usage context");
    assert_eq!(
        (context.spec_id, context.agent_home, context.model),
        (
            "acme".to_string(),
            tddy_daemon_kernel::user_paths::home_dir_for_user(&current_username()),
            Some("acme-model".to_string())
        )
    );
}

#[test]
fn a_workflow_spawn_without_an_agent_records_no_usage_context() {
    // When
    let context = recorded_context(
        SpawnOptions {
            new_session_id: Some("workflow-b"),
            ..SpawnOptions::default()
        },
        "workflow-b",
    );

    // Then
    assert_eq!(context, None);
}
