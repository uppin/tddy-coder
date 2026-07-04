//! Acceptance tests: Telegram `/start-claude` → project → branch → model → PTY spawn.
//!
//! Validates the full Telegram flow for launching a Claude Code CLI session:
//! `/start-claude <prompt>` → project keyboard → branch keyboard → model keyboard → PTY launch.
//!
//! These tests use:
//! - [`InMemoryTelegramSender`] to capture Telegram replies without a live bot
//! - An echo-argv stub script as the `claude` binary
//! - A real git repo with a self-hosted origin (so branch callbacks can list branches)
//! - [`serial_test::serial`] for tests that mutate global env vars (TDDY_PROJECTS_DIR)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tddy_core::changeset::read_changeset;
use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::claude_cli_session::{ClaudeCliSessionManager, PtyHandle};
use tddy_daemon::config::{ClaudeCliConfig, DaemonConfig};
use tddy_daemon::telegram_notifier::InMemoryTelegramSender;
use tddy_daemon::telegram_session_control::{
    collect_outbound_messages, read_changeset_routing_snapshot, StartClaudeCommand,
    TelegramSessionControlHarness, TelegramWorkflowSpawn, CLAUDE_CLI_MODELS,
};

const AUTHORIZED_CHAT: i64 = 777_001;
const TEST_USER_ID: u64 = 42;
const TEST_PROJECT_ID: &str = "tg-test-project";

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

/// Write an executable shell script that echoes all CLI arguments.
fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

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

/// Write `projects.yaml` registering the repo under TEST_PROJECT_ID.
///
/// Sets `main_branch_ref: origin/main` to match the test repo created with `git init -b main`.
/// Without this, `effective_integration_base_ref_for_project` would return `origin/master` (the
/// hardcoded default) which would cause `git fetch origin master` to fail on a `main`-only repo.
fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: tg-test-project\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// Build a `DaemonConfig` whose `claude_cli.binary_path` points at `stub_binary`.
fn daemon_config_with_stub_binary(stub_binary: &str) -> DaemonConfig {
    DaemonConfig {
        claude_cli: Some(ClaudeCliConfig {
            binary_path: stub_binary.to_string(),
            tddy_tools_path: None,
            daemon_url: None,
            claude_home_dir: None,
        }),
        ..Default::default()
    }
}

/// Build the full harness for `/start-claude` tests:
/// - Shared `ClaudeCliSessionManager` exposed to the caller for attachability assertions
/// - Echo-argv stub binary
/// - Real git repo registered as `TEST_PROJECT_ID`
///
/// Returns the harness and the sender so tests can inspect captured Telegram messages.
fn build_harness(
    sessions_base: std::path::PathBuf,
    stub_binary: &str,
    projects_dir: std::path::PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
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

/// Poll `capture` until it contains `needle` within `timeout_ms`.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// **start_claude_creates_session_with_initial_prompt_and_marker**: `/start-claude <prompt>`
/// creates a session_dir, persists `initial_prompt` and `session_type = claude-cli` in
/// `changeset.yaml`, and the first outgoing message is the **project** keyboard (not recipe).
#[tokio::test]
async fn start_claude_creates_session_with_initial_prompt_and_marker() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let manager = Arc::new(ClaudeCliSessionManager::new());

    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // Given
    // (harness and sender already set up above)

    // When
    let outcome = harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "build a hello world CLI tool".to_string(),
        })
        .await
        .expect("handle_start_claude must succeed");

    // Then
    assert!(
        !outcome.session_id.is_empty(),
        "handle_start_claude must assign a session_id"
    );

    let session_dir = unified_session_dir_path(sessions_tmp.path(), &outcome.session_id);
    assert!(
        session_dir.exists(),
        "session directory must be created under sessions_base"
    );

    // changeset.yaml must contain initial_prompt and session_type = claude-cli.
    let snap =
        read_changeset_routing_snapshot(&session_dir).expect("changeset.yaml must be readable");
    assert_eq!(
        snap.initial_prompt.as_deref(),
        Some("build a hello world CLI tool"),
        "initial_prompt must be persisted in changeset.yaml"
    );
    assert_eq!(
        snap.session_type.as_deref(),
        Some("claude-cli"),
        "session_type must be 'claude-cli' in changeset.yaml"
    );

    // The project keyboard must be the outgoing message (not recipe keyboard).
    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    assert!(
        !sent.is_empty(),
        "handle_start_claude must send at least one Telegram message"
    );
    let first = &sent[0];
    assert_eq!(
        first.chat_id, AUTHORIZED_CHAT,
        "message must target the authorised chat"
    );
    // The project keyboard has rows with buttons.
    let kb_flat: Vec<&String> = first
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(l, _)| l)
        .collect();
    assert!(
        !kb_flat.is_empty(),
        "first message must have an inline keyboard (project picker); sent: {:?}",
        sent
    );
    // No recipe button — claude-cli flow skips recipe selection entirely.
    let has_recipe_button = kb_flat.iter().any(|l| {
        let lower = l.to_lowercase();
        lower.contains("tdd") || lower.contains("recipe") || lower.contains("bugfix")
    });
    assert!(
        !has_recipe_button,
        "project keyboard must not contain recipe buttons; keyboard: {:?}",
        kb_flat
    );
}

/// **start_claude_project_then_branch_routes_to_model_keyboard**: after project callback →
/// branch callback, the reply is the **model** keyboard (3 claude-cli model buttons), not the
/// agent/tddy-coder keyboard.
#[tokio::test]
#[serial_test::serial]
async fn start_claude_project_then_branch_routes_to_model_keyboard() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());

    // projects_dir is passed explicitly via TelegramWorkflowSpawn.projects_dir_override

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let manager = Arc::new(ClaudeCliSessionManager::new());

    // Given
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    let outcome = harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "implement search feature".to_string(),
        })
        .await
        .expect("handle_start_claude must succeed");

    let session_id = outcome.session_id.clone();

    // When — select project
    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .expect("project callback must succeed");

    // Record how many messages have been sent so far, so we can slice to only the new ones.
    let msg_count_before_branch = sender.len();

    // When — select branch
    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &session_id)
        .await
        .expect("branch callback must succeed");

    // Then — after branch callback with session_type=claude-cli, the reply must be the model keyboard.
    // Skip messages that were sent before the branch callback.
    let all_msgs = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let sent_after_branch: Vec<_> = all_msgs.into_iter().skip(msg_count_before_branch).collect();
    assert!(
        !sent_after_branch.is_empty(),
        "branch callback must send the model keyboard; nothing was sent"
    );
    let model_kb = &sent_after_branch[0];
    let kb_data: Vec<&String> = model_kb
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(_, d)| d)
        .collect();
    // All model keyboard callback_data must start with the tcm: prefix.
    assert!(
        kb_data.iter().all(|d| d.starts_with("tcm:")),
        "model keyboard buttons must use 'tcm:' callback_data prefix; got: {:?}",
        kb_data
    );
    // Must have exactly CLAUDE_CLI_MODELS.len() buttons.
    assert_eq!(
        kb_data.len(),
        CLAUDE_CLI_MODELS.len(),
        "model keyboard must have one button per CLAUDE_CLI_MODELS entry"
    );

    // Then — changeset must still have session_type=claude-cli and the branch intent written.
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &session_id);
    let snap = read_changeset_routing_snapshot(&session_dir)
        .expect("changeset.yaml must be readable after branch callback");
    assert_eq!(
        snap.session_type.as_deref(),
        Some("claude-cli"),
        "session_type must still be claude-cli in changeset after branch callback"
    );
    let cs = read_changeset(&session_dir)
        .expect("changeset.yaml must be readable as Changeset after branch callback");
    assert!(
        cs.workflow
            .as_ref()
            .and_then(|w| w.branch_worktree_intent)
            .is_some(),
        "changeset.workflow.branch_worktree_intent must be set after branch callback"
    );
}

/// **start_claude_model_callback_launches_claude_cli**: the model callback runs
/// `spawn_telegram_claude_cli`; `.session.yaml` is written with `session_type=claude-cli`,
/// `model`, `pid`, and a real-worktree `repo_path`; the stub script's ARGV output is visible
/// in the PTY capture.
#[tokio::test]
#[serial_test::serial]
async fn start_claude_model_callback_launches_claude_cli() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());

    // projects_dir is passed explicitly via TelegramWorkflowSpawn.projects_dir_override

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let manager = Arc::new(ClaudeCliSessionManager::new());

    // Given
    let (mut harness, _sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When — /start-claude → project → branch → model
    let outcome = harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "add feature X".to_string(),
        })
        .await
        .expect("handle_start_claude must succeed");
    let session_id = outcome.session_id.clone();

    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .expect("project callback must succeed");

    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &session_id)
        .await
        .expect("branch callback must succeed");

    // Pick model index 0 (Opus 4).
    harness
        .handle_telegram_claude_model_callback(AUTHORIZED_CHAT, 0, 0, &session_id)
        .await
        .expect("model callback must succeed: spawns claude-cli");

    // Then
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &session_id);

    // .session.yaml must be written with claude-cli metadata.
    let meta = read_session_metadata(&session_dir)
        .expect(".session.yaml must be written after model callback");

    assert_eq!(
        meta.session_type.as_deref(),
        Some("claude-cli"),
        "session_type must be 'claude-cli' in .session.yaml"
    );
    assert!(
        meta.pid.is_some(),
        "pid must be written to .session.yaml after PTY spawn"
    );
    assert!(
        meta.repo_path.is_some(),
        "repo_path must be written to .session.yaml"
    );

    let (expected_model_id, _label) = CLAUDE_CLI_MODELS[0];
    assert_eq!(
        meta.model.as_deref(),
        Some(expected_model_id),
        "model must match the chosen claude-cli model"
    );

    let worktree_path = std::path::PathBuf::from(meta.repo_path.unwrap());
    assert!(
        worktree_path.exists(),
        "worktree must exist at repo_path: {}",
        worktree_path.display()
    );

    // Verify it is a real git worktree.
    let wt_list = std::process::Command::new("git")
        .args(["worktree", "list"])
        .current_dir(repo_dir.path())
        .output()
        .expect("git worktree list must run");
    let wt_stdout = String::from_utf8_lossy(&wt_list.stdout);
    assert!(
        wt_stdout
            .lines()
            .any(|l| l.starts_with(worktree_path.to_str().unwrap())),
        "worktree must appear in 'git worktree list';\n\
         worktree_path={}\ngit worktree list:\n{}",
        worktree_path.display(),
        wt_stdout
    );
}

/// **start_claude_uses_shared_manager**: after `spawn_telegram_claude_cli`, the injected
/// `Arc<ClaudeCliSessionManager>` must contain the session — proving it is attachable via the
/// terminal-stream RPCs (the same registry the daemon's `ConnectionServiceImpl` uses).
#[tokio::test]
#[serial_test::serial]
async fn start_claude_uses_shared_manager() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());

    // projects_dir is passed explicitly via TelegramWorkflowSpawn.projects_dir_override

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let manager = Arc::new(ClaudeCliSessionManager::new());

    // Given
    let (mut harness, _sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When — /start-claude → project → branch → model
    let outcome = harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "build a search feature".to_string(),
        })
        .await
        .expect("handle_start_claude must succeed");
    let session_id = outcome.session_id.clone();

    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .expect("project callback must succeed");

    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &session_id)
        .await
        .expect("branch callback must succeed");

    harness
        .handle_telegram_claude_model_callback(AUTHORIZED_CHAT, 0, 0, &session_id)
        .await
        .expect("model callback must succeed");

    // Then — the shared manager must now contain the session (proves attachability via terminal RPCs).
    let handle = manager
        .get(&session_id)
        .await
        .expect("session must be in ClaudeCliSessionManager after Telegram spawn");

    // And the PTY process is running (stub wrote ARGV output).
    let found = wait_for_capture_contains(&handle, "ARGV:", 2000).await;
    assert!(
        found,
        "stub script must write ARGV: to PTY capture within 2s; session_id={}",
        session_id
    );

    let cap = handle.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    assert!(
        output.contains("build a search feature"),
        "initial_prompt must be passed as positional arg via Telegram flow; got: {:?}",
        output
    );
}
