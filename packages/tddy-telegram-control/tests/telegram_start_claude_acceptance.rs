//! Acceptance tests: Telegram `/start-claude` → project → branch → model keyboards, and the reply
//! when the daemon's generic start answers (`#agent-tools 18/19`). The launch itself — argv, hooks,
//! the start guard — is `telegram_start_agent_from_spec.rs`'s.
//!
//! These tests use:
//! - [`InMemoryTelegramSender`] to capture Telegram replies without a live bot
//! - The built-in coding agents, and a starter that answers every start as one session
//! - A real git repo with a self-hosted origin (so branch callbacks can list branches)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::changeset::read_changeset;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_extensions::{OperatorSessionStart, OperatorStartOutcome, SessionStarter};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_telegram_control::telegram_notifier::InMemoryTelegramSender;
use tddy_telegram_control::telegram_session_control::{
    collect_outbound_messages, read_changeset_routing_snapshot, telegram_model_choices,
    StartClaudeCommand, TelegramSessionControlHarness, TelegramWorkflowSpawn,
};

const AUTHORIZED_CHAT: i64 = 777_001;
const TEST_USER_ID: u64 = 42;
const TEST_PROJECT_ID: &str = "tg-test-project";
const STARTED_SESSION: &str = "019d6392-3cff-7018-8000-000000000001";

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

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
        claude_cli_manager: Arc::new(ClaudeCliSessionManager::new()),
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
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        projects_tmp.path().to_path_buf(),
    );

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
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &outcome.session_id);
    assert!(
        session_dir.exists(),
        "session directory must be created under sessions_base"
    );
    let snap =
        read_changeset_routing_snapshot(&session_dir).expect("changeset.yaml must be readable");
    assert_eq!(
        (snap.initial_prompt.as_deref(), snap.session_type.as_deref()),
        (Some("build a hello world CLI tool"), Some("claude-cli"))
    );

    // The project keyboard is the first message, with no recipe buttons.
    let sent = collect_outbound_messages(&sender, AUTHORIZED_CHAT);
    let kb_flat: Vec<&String> = sent[0]
        .inline_keyboard
        .iter()
        .flatten()
        .map(|(l, _)| l)
        .collect();
    assert!(
        !kb_flat.is_empty(),
        "first message must have an inline keyboard (project picker); sent: {sent:?}"
    );
    let has_recipe_button = kb_flat.iter().any(|l| {
        let lower = l.to_lowercase();
        lower.contains("tdd") || lower.contains("recipe") || lower.contains("bugfix")
    });
    assert!(
        !has_recipe_button,
        "project keyboard must not contain recipe buttons; keyboard: {kb_flat:?}"
    );
}

/// **start_claude_project_then_branch_routes_to_model_keyboard**: after project callback →
/// branch callback, the reply is the **model** keyboard — one `tcm:` button per model the claude
/// spec offers — not the agent/tddy-coder keyboard.
#[tokio::test]
#[serial_test::serial]
async fn start_claude_project_then_branch_routes_to_model_keyboard() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());

    // Given
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        projects_tmp.path().to_path_buf(),
    );
    let session_id = harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "implement search feature".to_string(),
        })
        .await
        .expect("handle_start_claude must succeed")
        .session_id;
    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .expect("project callback must succeed");
    let msg_count_before_branch = sender.len();

    // When — select branch
    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &session_id)
        .await
        .expect("branch callback must succeed");

    // Then — the model keyboard: tcm: buttons, one per model the claude spec offers
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
    let offered = telegram_model_choices(registry.get("claude").unwrap())
        .unwrap()
        .len();
    assert_eq!(
        (kb_data.iter().all(|d| d.starts_with("tcm:")), kb_data.len()),
        (true, offered),
        "model keyboard must have one tcm: button per model of the claude spec: {kb_data:?}"
    );

    // Then — the changeset keeps session_type=claude-cli and has the branch intent written.
    let session_dir = unified_session_dir_path(sessions_tmp.path(), &session_id);
    let snap = read_changeset_routing_snapshot(&session_dir).unwrap();
    let cs = read_changeset(&session_dir).unwrap();
    assert_eq!(
        (
            snap.session_type.as_deref(),
            cs.workflow
                .as_ref()
                .and_then(|w| w.branch_worktree_intent)
                .is_some()
        ),
        (Some("claude-cli"), true)
    );
}

// ---------------------------------------------------------------------------
// A project that acts as a GitHub account: Telegram cannot open its vault
// ---------------------------------------------------------------------------

/// Like [`register_project`], with the project assigned a GitHub account.
fn register_project_assigned_to(
    projects_dir: &std::path::Path,
    repo_path: &std::path::Path,
    account: &str,
) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: tg-test-project\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n    accounts:\n      - provider: github\n        account_id: {account}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// Run `/start-claude` through the model callback and return everything the chat was told.
async fn texts_sent_while_starting_claude(
    register: impl FnOnce(&std::path::Path, &std::path::Path),
) -> Vec<String> {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register(projects_tmp.path(), repo_dir.path());
    let (mut harness, sender) = build_harness(
        sessions_tmp.path().to_path_buf(),
        projects_tmp.path().to_path_buf(),
    );
    let session_id = harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: "add feature X".to_string(),
        })
        .await
        .expect("handle_start_claude must succeed")
        .session_id;
    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .expect("project callback must succeed");
    harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &session_id)
        .await
        .expect("branch callback must succeed");
    harness
        .handle_telegram_model_pick_callback(AUTHORIZED_CHAT, 0, 0, &session_id)
        .await
        .expect("model callback must succeed");
    collect_outbound_messages(&sender, AUTHORIZED_CHAT)
        .into_iter()
        .map(|message| message.text)
        .collect()
}

#[tokio::test]
#[serial_test::serial]
async fn a_telegram_started_session_on_a_project_that_acts_as_an_account_says_the_account_is_unavailable(
) {
    // Given a project assigned a GitHub account
    // When a session is started from Telegram on it
    let texts = texts_sent_while_starting_claude(|projects, repo| {
        register_project_assigned_to(projects, repo, "acct-ada")
    })
    .await;

    // Then the chat is told, once, that the account's identity and tools are unavailable from Telegram and why
    let notices: Vec<&String> = texts
        .iter()
        .filter(|text| text.contains("GitHub account") && text.contains("Telegram"))
        .collect();
    assert_eq!(notices.len(), 1, "{texts:?}");
    assert!(
        notices[0].contains("signed-in") && notices[0].contains("web"),
        "the notice must say why and where to start instead: {}",
        notices[0]
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_telegram_started_session_on_a_project_with_no_account_says_nothing_about_accounts() {
    // Given a project that assigns no account
    // When a session is started from Telegram on it
    let texts = texts_sent_while_starting_claude(register_project).await;

    // Then nothing was lost, so nothing is said
    assert!(
        texts.iter().all(|text| !text.contains("GitHub account")),
        "{texts:?}"
    );
}
