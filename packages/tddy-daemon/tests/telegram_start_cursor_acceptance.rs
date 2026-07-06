//! Acceptance tests: Telegram `/start-cursor` → project → branch → model → PTY spawn.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::claude_cli_session::{CliSessionManager, PtyHandle};
use tddy_daemon::config::{CursorCliConfig, DaemonConfig};
use tddy_daemon::telegram_notifier::InMemoryTelegramSender;
use tddy_daemon::telegram_session_control::{
    collect_outbound_messages, read_changeset_routing_snapshot, StartCursorCommand,
    TelegramSessionControlHarness, TelegramWorkflowSpawn, CURSOR_CLI_MODELS,
};

const AUTHORIZED_CHAT: i64 = 777_002;
const TEST_USER_ID: u64 = 43;
const TEST_PROJECT_ID: &str = "tg-cursor-project";

fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_agent.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

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

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: tg-cursor-project\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn daemon_config_with_stub_binary(stub_binary: &str) -> DaemonConfig {
    DaemonConfig {
        cursor_cli: Some(CursorCliConfig {
            binary_path: stub_binary.to_string(),
            tddy_tools_path: None,
            daemon_url: None,
            cursor_home_dir: None,
        }),
        ..Default::default()
    }
}

fn build_harness(
    sessions_base: std::path::PathBuf,
    stub_binary: &str,
    projects_dir: std::path::PathBuf,
    manager: Arc<CliSessionManager>,
) -> (
    TelegramSessionControlHarness<InMemoryTelegramSender>,
    Arc<InMemoryTelegramSender>,
) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let config = daemon_config_with_stub_binary(stub_binary);
    let workflow_spawn = Arc::new(TelegramWorkflowSpawn {
        config: Arc::new(config),
        spawn_client: None,
        os_user: "testuser".to_string(),
        tddy_data_dir: sessions_base.clone(),
        projects_dir_override: Some(projects_dir),
        telegram_hooks: None,
        child_grpc_by_session: Arc::new(Mutex::new(HashMap::new())),
        elicitation_select_options: Arc::new(Mutex::new(HashMap::new())),
        elicitation_multi_select_meta: Arc::new(Mutex::new(HashMap::new())),
        pending_elicitation_other: Arc::new(Mutex::new(HashMap::new())),
        claude_cli_manager: manager,
    });
    let harness = TelegramSessionControlHarness::with_workflow_spawn(
        vec![AUTHORIZED_CHAT],
        sessions_base,
        sender.clone(),
        Some(workflow_spawn),
        None,
    );
    (harness, sender)
}

async fn wait_for_capture_contains(handle: &Arc<PtyHandle>, needle: &str, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        {
            let cap = handle.capture.lock().unwrap();
            if String::from_utf8_lossy(&cap).contains(needle) {
                return true;
            }
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn start_cursor_creates_session_with_initial_prompt_and_marker() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let manager = Arc::new(CliSessionManager::new());
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
        Arc::clone(&manager),
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
    assert!(!outcome.session_id.is_empty());
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &outcome.session_id);
    let snap = read_changeset_routing_snapshot(&session_dir).unwrap();
    assert_eq!(snap.initial_prompt.as_deref(), Some("fix the flaky test"));
    assert_eq!(snap.session_type.as_deref(), Some("cursor-cli"));

    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    assert!(!sent.is_empty());
    let kb_flat: Vec<&String> = sent[0]
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(l, _)| l)
        .collect();
    assert!(!kb_flat.is_empty(), "project keyboard must be sent");
}

#[tokio::test]
#[serial_test::serial]
async fn start_cursor_project_then_branch_routes_to_cursor_model_keyboard() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let manager = Arc::new(CliSessionManager::new());
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    let outcome = harness
        .handle_start_cursor(StartCursorCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "add logging".to_string(),
        })
        .await
        .unwrap();
    let session_id = outcome.session_id.clone();

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
    let all_msgs = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let sent_after_branch: Vec<_> = all_msgs.into_iter().skip(msg_count_before_branch).collect();
    assert!(!sent_after_branch.is_empty());
    let kb_data: Vec<&String> = sent_after_branch[0]
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(_, d)| d)
        .collect();
    assert!(kb_data.iter().all(|d| d.starts_with("tcur:")));
    assert_eq!(kb_data.len(), CURSOR_CLI_MODELS.len());
}

#[tokio::test]
#[serial_test::serial]
async fn start_cursor_model_callback_launches_cursor_cli_with_hooks() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let manager = Arc::new(CliSessionManager::new());
    let (mut harness, _sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    let outcome = harness
        .handle_start_cursor(StartCursorCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "ship feature".to_string(),
        })
        .await
        .unwrap();
    let session_id = outcome.session_id.clone();

    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .unwrap();
    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &session_id)
        .await
        .unwrap();

    // When
    harness
        .handle_telegram_cursor_model_callback(AUTHORIZED_CHAT, 0, 0, &session_id)
        .await
        .expect("model callback must spawn cursor-cli");

    // Then
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &session_id);
    let meta = read_session_metadata(&session_dir).unwrap();
    assert_eq!(meta.session_type.as_deref(), Some("cursor-cli"));
    assert!(meta.pid.is_some());
    assert!(meta.hook_token.is_some());

    let worktree_path = std::path::PathBuf::from(meta.repo_path.unwrap());
    let hooks_path = worktree_path.join(".cursor/hooks.json");
    assert!(
        hooks_path.exists(),
        "Telegram spawn must write .cursor/hooks.json"
    );

    let (expected_model_id, _) = CURSOR_CLI_MODELS[0];
    assert_eq!(meta.model.as_deref(), Some(expected_model_id));

    let handle = manager
        .get(&session_id)
        .await
        .expect("session must be registered in CliSessionManager");
    assert!(
        wait_for_capture_contains(&handle, "ARGV:", 3000).await,
        "stub binary output must appear in PTY capture"
    );
}
