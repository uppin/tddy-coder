//! Acceptance: Telegram's project picker leaves out a soft-deleted project — deleting a project
//! hides it from every project picker, and Telegram's is one.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Renaming, deleting and restoring a project.
//! Changeset: docs/dev/changesets/2026-11-15-project-update-and-soft-delete.md

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_telegram_control::telegram_notifier::InMemoryTelegramSender;
use tddy_telegram_control::telegram_session_control::{
    collect_outbound_messages, StartClaudeCommand, TelegramSessionControlHarness,
    TelegramWorkflowSpawn,
};

const AUTHORIZED_CHAT: i64 = 777_002;
const TEST_USER_ID: u64 = 43;

/// A registry with one live project and one deleted one.
fn a_registry_with_a_deleted_project(projects_dir: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        "projects:\n  - project_id: live-project\n    name: still-here\n    git_url: \"\"\n    main_repo_path: /repos/live\n  - project_id: gone-project\n    name: deleted-one\n    git_url: \"\"\n    main_repo_path: /repos/gone\n    deleted_at: '2026-11-15T10:00:00.000Z'\n    deleted_by: octocat\n",
    )
    .unwrap();
}

fn build_harness(
    sessions_base: std::path::PathBuf,
    projects_dir: std::path::PathBuf,
) -> (
    TelegramSessionControlHarness<InMemoryTelegramSender>,
    Arc<InMemoryTelegramSender>,
) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let workflow_spawn = Arc::new(TelegramWorkflowSpawn {
        config: Arc::new(DaemonConfig::default()),
        spawn_client: None,
        spawn_backend: tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        os_user: "testuser".to_string(),
        tddy_data_dir: sessions_base.clone(),
        projects_dir_override: Some(projects_dir),
        telegram_hooks: None,
        child_grpc_by_session: Arc::new(Mutex::new(HashMap::new())),
        elicitation_select_options: Arc::new(Mutex::new(HashMap::new())),
        elicitation_multi_select_meta: Arc::new(Mutex::new(HashMap::new())),
        pending_elicitation_other: Arc::new(Mutex::new(HashMap::new())),
        cli_manager: Arc::new(CliSessionManager::new()),
    });
    let harness = TelegramSessionControlHarness::with_workflow_spawn(
        vec![AUTHORIZED_CHAT],
        sessions_base,
        sender.clone(),
        Some(workflow_spawn),
        None,
    )
    .with_coding_agents(Arc::new(
        CodingAgentRegistry::builtin().expect("the built-in agents load"),
    ));
    (harness, sender)
}

#[tokio::test]
async fn the_project_picker_leaves_out_a_deleted_project() {
    // Given a live project and a deleted one
    let sessions = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    a_registry_with_a_deleted_project(projects.path());
    let (mut harness, sender) =
        build_harness(sessions.path().to_path_buf(), projects.path().to_path_buf());

    // When a session is started, which opens the project picker
    harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "anything".to_string(),
        })
        .await
        .expect("handle_start_claude must succeed");

    // Then the picker offers the live project only
    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let labels: Vec<&String> = sent[0]
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(label, _)| label)
        .collect();
    assert!(
        labels.iter().any(|l| l.contains("still-here")),
        "the live project is offered: {labels:?}"
    );
    assert!(
        !labels.iter().any(|l| l.contains("deleted-one")),
        "the deleted project is not offered: {labels:?}"
    );
}
