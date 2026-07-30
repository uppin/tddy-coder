//! Acceptance tests: the Telegram `/start-claude` branch pick prompts when the branch it is about
//! to create is already owned by another session, instead of silently creating a `<branch>-1`.
//!
//! Telegram spawns sessions without going through `StartSession` — it calls
//! `setup_worktree_for_session_with_optional_chain_base` directly — so it needs the same ownership
//! check at its own call site. The check runs when the base-branch choice resolves the branch name
//! (`handle_telegram_branch_callback`), so the operator is asked *before* the model picker rather
//! than after committing to a model.
//!
//! The branch name is derived, never typed: `feature/<slug(changeset.name)>`, and `changeset.name`
//! is the first six words of the `/start-claude` prompt. A prompt of "Auth Feature" therefore
//! resolves to `feature/auth-feature`, which is what these tests pre-own.
//!
//! PRD: docs/ft/daemon/session-branch-conflict.md

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tddy_core::changeset::{BranchWorktreeIntent, Changeset};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::{ClaudeCliConfig, DaemonConfig};
use tddy_daemon::telegram_notifier::InMemoryTelegramSender;
use tddy_daemon::telegram_session_control::{
    collect_outbound_messages, parse_telegram_branch_conflict_callback, CapturedTelegramMessage,
    StartClaudeCommand, TelegramSessionControlHarness, TelegramWorkflowSpawn,
};
use tddy_daemon::telegram_tracked_session::{
    SharedTelegramTrackedSessionCoordinator, TelegramTrackedSessionCoordinator,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

const AUTHORIZED_CHAT: i64 = 777_101;
const TEST_USER_ID: u64 = 42;
const TEST_PROJECT_ID: &str = "tg-branch-conflict-project";

/// The prompt whose first six words become `changeset.name`.
const PROMPT: &str = "Auth Feature";
/// What `feature/<slug(name)>` resolves to for `PROMPT`.
const DERIVED_BRANCH: &str = "feature/auth-feature";
/// The callback-data prefix for the three conflict choices.
const CB_BRANCH_CONFLICT: &str = "tbc:";

const OWNER_SESSION: &str = "019d6392-3cff-0002-bbbb-000000000001";
/// What `first_free_suffixed_branch_name` returns for [`DERIVED_BRANCH`] in a repo where no
/// suffixed branch exists yet.
const SUGGESTED_BRANCH: &str = "feature/auth-feature-1";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn write_echo_argv_script(dir: &Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

/// A git repo whose `origin` is itself, so the branch pick can list refs with no server.
fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

/// `main_branch_ref: origin/main` matches the `git init -b main` repo above.
fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: tg-branch-conflict-project\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn build_harness(
    sessions_base: std::path::PathBuf,
    stub_binary: &str,
    projects_dir: std::path::PathBuf,
) -> (
    TelegramSessionControlHarness<InMemoryTelegramSender>,
    Arc<InMemoryTelegramSender>,
    SharedTelegramTrackedSessionCoordinator,
) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let config = DaemonConfig {
        claude_cli: Some(ClaudeCliConfig {
            binary_path: stub_binary.to_string(),
            tddy_tools_path: None,
            daemon_url: None,
            claude_home_dir: None,
        }),
        ..Default::default()
    };
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
        claude_cli_manager: Arc::new(ClaudeCliSessionManager::new()),
    });
    let tracked: SharedTelegramTrackedSessionCoordinator =
        Arc::new(Mutex::new(TelegramTrackedSessionCoordinator::new()));
    let harness = TelegramSessionControlHarness::with_workflow_spawn_and_telegram_tracked(
        vec![AUTHORIZED_CHAT],
        sessions_base,
        sender.clone(),
        Some(workflow_spawn),
        None,
        Some(tracked.clone()),
    );
    (harness, sender, tracked)
}

/// A live session that owns `branch`.
fn a_session_owning(sessions_base: &Path, session_id: &str, branch: &str) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    tddy_core::write_changeset(
        &dir,
        &Changeset {
            branch: Some(branch.to_string()),
            ..Changeset::default()
        },
    )
    .unwrap();
    let meta = a_session_metadata()
        .with_session_id(session_id)
        .with_status("active")
        // The test process is the only pid guaranteed alive, so it is what makes `is_active` true.
        .with_pid(std::process::id())
        .build();
    write_session_yaml(&dir, &meta);
}

/// Everything `/start-claude` needs, with the session already past the project pick.
struct World {
    _repo_dir: tempfile::TempDir,
    _projects_tmp: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    sessions_tmp: tempfile::TempDir,
    harness: TelegramSessionControlHarness<InMemoryTelegramSender>,
    sender: Arc<InMemoryTelegramSender>,
    tracked: SharedTelegramTrackedSessionCoordinator,
    session_id: String,
}

impl World {
    /// The session this chat is bound to — where its next messages and replies go.
    fn chat_bound_session(&self) -> Option<String> {
        self.tracked
            .lock()
            .expect("tracked coordinator must not be poisoned")
            .tracked_session_for_chat(AUTHORIZED_CHAT)
    }

    /// The branch plan the pending session's changeset carries.
    fn pending_branch_plan(&self) -> BranchPlan {
        let session_dir = unified_session_dir_path(self.sessions_tmp.path(), &self.session_id);
        let cs = tddy_core::read_changeset(&session_dir).expect("changeset must be readable");
        let wf = cs
            .workflow
            .expect("the branch pick must have written workflow");
        BranchPlan {
            intent: wf.branch_worktree_intent,
            new_branch_name: wf.new_branch_name,
            selected_branch_to_work_on: wf.selected_branch_to_work_on,
        }
    }
}

/// How the pending session intends to reach a checkout — what each conflict choice rewrites.
#[derive(Debug, PartialEq)]
struct BranchPlan {
    intent: Option<BranchWorktreeIntent>,
    new_branch_name: Option<String>,
    selected_branch_to_work_on: Option<String>,
}

async fn a_world_at_the_branch_pick() -> World {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());

    let (mut harness, sender, tracked) = build_harness(
        sessions_tmp.path().to_path_buf(),
        stub_path.to_str().unwrap(),
        projects_tmp.path().to_path_buf(),
    );

    let outcome = harness
        .handle_start_claude(StartClaudeCommand {
            chat_id: AUTHORIZED_CHAT,
            user_id: TEST_USER_ID,
            prompt: PROMPT.to_string(),
        })
        .await
        .expect("handle_start_claude must succeed");
    let session_id = outcome.session_id.clone();

    harness
        .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, &session_id)
        .await
        .expect("project callback must succeed");

    World {
        _repo_dir: repo_dir,
        _projects_tmp: projects_tmp,
        _stub_dir: stub_dir,
        sessions_tmp,
        harness,
        sender,
        tracked,
        session_id,
    }
}

/// Pick the default base branch (index 0) and return only the messages that produced.
async fn pick_default_base(world: &World) -> Vec<CapturedTelegramMessage> {
    let before = world.sender.len();
    world
        .harness
        .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, &world.session_id)
        .await
        .expect("branch callback must answer");
    collect_outbound_messages(&world.sender, AUTHORIZED_CHAT)
        .into_iter()
        .skip(before)
        .collect()
}

fn callback_data(msg: &CapturedTelegramMessage) -> Vec<String> {
    msg.inline_keyboard
        .iter()
        .flatten()
        .map(|(_, d)| d.clone())
        .collect()
}

/// Tap the conflict button carrying `choice_code`, routing its `callback_data` through the same
/// parser the bot's callback dispatcher uses.
async fn tap_conflict_choice(world: &World, sent: &[CapturedTelegramMessage], choice_code: &str) {
    let wanted = format!("{CB_BRANCH_CONFLICT}{choice_code}:");
    let data = sent
        .iter()
        .flat_map(callback_data)
        .find(|d| d.starts_with(&wanted))
        .unwrap_or_else(|| panic!("no {wanted:?} button in the conflict keyboard; sent: {sent:?}"));
    let (choice, proj_idx, session_id) = parse_telegram_branch_conflict_callback(&data)
        .unwrap_or_else(|| panic!("the production parser must decode {data:?}"));
    world
        .harness
        .handle_telegram_branch_conflict_callback(AUTHORIZED_CHAT, choice, proj_idx, &session_id)
        .await
        .expect("conflict callback must be handled");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn telegram_branch_pick_offers_three_conflict_choices_when_the_derived_branch_is_owned() {
    // Given
    let world = a_world_at_the_branch_pick().await;
    a_session_owning(world.sessions_tmp.path(), OWNER_SESSION, DERIVED_BRANCH);

    // When
    let sent = pick_default_base(&world).await;

    // Then — switch / add another agent / use the suggested name, instead of the model picker.
    let prompt = sent
        .iter()
        .find(|m| {
            callback_data(m)
                .iter()
                .any(|d| d.starts_with(CB_BRANCH_CONFLICT))
        })
        .unwrap_or_else(|| {
            panic!("an owned branch must produce a conflict keyboard; sent: {sent:?}")
        });
    assert_eq!(
        callback_data(prompt).len(),
        3,
        "the prompt must offer exactly three choices; keyboard: {:?}",
        prompt.inline_keyboard
    );
}

#[tokio::test]
async fn telegram_branch_conflict_buttons_fit_the_callback_data_limit() {
    // Given
    let world = a_world_at_the_branch_pick().await;
    a_session_owning(world.sessions_tmp.path(), OWNER_SESSION, DERIVED_BRANCH);

    // When
    let sent = pick_default_base(&world).await;

    // Then — Telegram rejects callback_data over 64 bytes, which is why the branch name is
    // re-derived server-side instead of being carried in the payload.
    let prompt = sent
        .iter()
        .find(|m| {
            callback_data(m)
                .iter()
                .any(|d| d.starts_with(CB_BRANCH_CONFLICT))
        })
        .unwrap_or_else(|| {
            panic!("an owned branch must produce a conflict keyboard; sent: {sent:?}")
        });
    for data in callback_data(prompt) {
        assert!(
            data.starts_with(CB_BRANCH_CONFLICT),
            "every conflict button must use the {CB_BRANCH_CONFLICT} prefix; got {data:?}"
        );
        assert!(
            data.len() <= 64,
            "callback_data must fit 64 bytes; {data:?} is {} bytes",
            data.len()
        );
    }
}

#[tokio::test]
async fn telegram_branch_pick_goes_straight_to_the_model_picker_when_no_session_owns_the_branch() {
    // Given — no session claims the derived branch.
    let world = a_world_at_the_branch_pick().await;

    // When
    let sent = pick_default_base(&world).await;

    // Then — the unowned path is unchanged: the model keyboard, and no prompt.
    assert!(
        !sent.is_empty(),
        "the branch pick must reply with the model keyboard"
    );
    let model_kb = &sent[0];
    assert!(
        callback_data(model_kb)
            .iter()
            .all(|d| d.starts_with("tcm:")),
        "expected the claude model keyboard; got {:?}",
        model_kb.inline_keyboard
    );
    assert!(
        !sent.iter().any(|m| callback_data(m)
            .iter()
            .any(|d| d.starts_with(CB_BRANCH_CONFLICT))),
        "an unowned branch must not prompt; sent: {sent:?}"
    );
}

#[tokio::test]
async fn telegram_branch_conflict_creates_no_worktree_for_the_pending_session() {
    // Given
    let world = a_world_at_the_branch_pick().await;
    a_session_owning(world.sessions_tmp.path(), OWNER_SESSION, DERIVED_BRANCH);

    // When
    pick_default_base(&world).await;

    // Then — the pending session must still be un-spawned while the operator decides.
    let session_dir = unified_session_dir_path(world.sessions_tmp.path(), &world.session_id);
    let cs = tddy_core::read_changeset(&session_dir).expect("changeset must be readable");
    assert_eq!(
        cs.worktree, None,
        "asking the operator must not create a worktree first"
    );
    assert_eq!(
        cs.branch, None,
        "asking the operator must not claim a branch first"
    );
}

#[tokio::test]
async fn telegram_branch_conflict_switch_binds_the_chat_to_the_owning_session() {
    // Given
    let world = a_world_at_the_branch_pick().await;
    a_session_owning(world.sessions_tmp.path(), OWNER_SESSION, DERIVED_BRANCH);
    let prompt = pick_default_base(&world).await;

    // When
    tap_conflict_choice(&world, &prompt, "sw").await;

    // Then — the chat now talks to the session that owns the branch, as if its Enter button was tapped.
    assert_eq!(
        world.chat_bound_session(),
        Some(OWNER_SESSION.to_string()),
        "Switch must bind the chat to the owning session"
    );
}

#[tokio::test]
async fn telegram_branch_conflict_add_agent_rewrites_the_changeset_to_the_owned_branch() {
    // Given
    let world = a_world_at_the_branch_pick().await;
    a_session_owning(world.sessions_tmp.path(), OWNER_SESSION, DERIVED_BRANCH);
    let prompt = pick_default_base(&world).await;

    // When
    tap_conflict_choice(&world, &prompt, "na").await;

    // Then — a second agent joins the owning session's checkout instead of branching off it.
    assert_eq!(
        world.pending_branch_plan(),
        BranchPlan {
            intent: Some(BranchWorktreeIntent::WorkOnSelectedBranch),
            new_branch_name: None,
            selected_branch_to_work_on: Some(DERIVED_BRANCH.to_string()),
        }
    );
}

#[tokio::test]
async fn telegram_branch_conflict_suggested_name_keeps_a_new_branch_under_the_suffixed_name() {
    // Given
    let world = a_world_at_the_branch_pick().await;
    a_session_owning(world.sessions_tmp.path(), OWNER_SESSION, DERIVED_BRANCH);
    let prompt = pick_default_base(&world).await;

    // When
    tap_conflict_choice(&world, &prompt, "sg").await;

    // Then — still its own branch, just the first free suffixed name.
    assert_eq!(
        world.pending_branch_plan(),
        BranchPlan {
            intent: Some(BranchWorktreeIntent::NewBranchFromBase),
            new_branch_name: Some(SUGGESTED_BRANCH.to_string()),
            selected_branch_to_work_on: None,
        }
    );
}
