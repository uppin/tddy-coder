//! Acceptance tests: Telegram `/start-cursor` → project → branch → model keyboards
//! (`#agent-tools 18/19`). The launch itself — argv, hooks, the minted chat — is
//! `telegram_start_agent_from_spec.rs`'s.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_extensions::{OperatorSessionStart, OperatorStartOutcome, SessionStarter};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_telegram_control::telegram_notifier::InMemoryTelegramSender;
use tddy_telegram_control::telegram_session_control::{
    collect_outbound_messages, read_changeset_routing_snapshot, telegram_model_choices,
    StartCursorCommand, TelegramSessionControlHarness, TelegramWorkflowSpawn,
};

const AUTHORIZED_CHAT: i64 = 777_002;
const TEST_USER_ID: u64 = 43;
const TEST_PROJECT_ID: &str = "tg-cursor-project";
const STARTED_SESSION: &str = "019d6392-3cff-7018-8000-000000000002";

/// Create a git repo with a self-hosted origin remote.
fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(dir);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("git command failed");
    };
    let author_env = &[
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "t@t.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "t@t.com"),
    ];
    run(&["init", "-b", "main"], &[]);
    run(&["config", "user.email", "t@t.com"], &[]);
    run(&["config", "user.name", "Test"], &[]);
    run(&["commit", "--allow-empty", "-m", "init"], author_env);
    run(&["remote", "add", "origin", dir.to_str().unwrap()], &[]);
    run(&["push", "-u", "origin", "main"], &[]);
}

/// Write `projects.yaml` registering the repo under `TEST_PROJECT_ID`, on `origin/main`.
fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: {}\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n",
        TEST_PROJECT_ID,
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// The built-in coding agents, as a host without `<tddyhome>/tools` has them.
fn the_builtin_agents() -> Arc<CodingAgentRegistry> {
    Arc::new(CodingAgentRegistry::builtin().expect("the built-in agents load"))
}

/// The daemon's generic start, answering every start as session `started_id`.
struct AStarterAnswering {
    started_id: String,
}

#[async_trait::async_trait]
impl SessionStarter for AStarterAnswering {
    async fn start_for_operator(
        &self,
        _start: OperatorSessionStart,
    ) -> Result<OperatorStartOutcome, tddy_rpc::Status> {
        Ok(OperatorStartOutcome::Started {
            session_id: self.started_id.clone(),
        })
    }
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
    .with_coding_agents(the_builtin_agents())
    .with_session_starter(Arc::new(AStarterAnswering {
        started_id: STARTED_SESSION.to_string(),
    }));
    (harness, sender)
}

#[tokio::test]
async fn start_cursor_creates_session_with_initial_prompt_and_marker() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        projects_tmp.path().to_path_buf(),
    );

    // When
    let outcome = harness
        .handle_start_cursor(StartCursorCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "fix the flaky test".to_string(),
        })
        .await
        .expect("handle_start_cursor must succeed");

    // Then
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &outcome.session_id);
    let snap = read_changeset_routing_snapshot(&session_dir).unwrap();
    assert_eq!(
        (snap.initial_prompt.as_deref(), snap.session_type.as_deref()),
        (Some("fix the flaky test"), Some("cursor-cli"))
    );
    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    assert!(
        !sent[0].inline_keyboard.is_empty(),
        "project keyboard must be sent"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn start_cursor_project_then_branch_routes_to_the_cursor_specs_model_keyboard() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        projects_tmp.path().to_path_buf(),
    );
    let session_id = harness
        .handle_start_cursor(StartCursorCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "add logging".to_string(),
        })
        .await
        .unwrap()
        .session_id;
    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .unwrap();
    let msg_count_before_branch = sender.len();

    // When
    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &session_id)
        .await
        .unwrap();

    // Then
    let sent_after_branch: Vec<_> = collect_outbound_messages(&sender, AUTHORIZED_CHAT)
        .into_iter()
        .skip(msg_count_before_branch)
        .collect();
    let kb_data: Vec<&String> = sent_after_branch[0]
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(_, d)| d)
        .collect();
    let registry = CodingAgentRegistry::builtin().unwrap();
    let offered = telegram_model_choices(registry.get("cursor").unwrap())
        .unwrap()
        .len();
    assert_eq!(
        (kb_data.iter().all(|d| d.starts_with("tcm:")), kb_data.len()),
        (true, offered)
    );
}
