//! An operator's agent pick starts the workflow session the pickers chose — or says why it cannot,
//! and starts nothing.
//!
//! Driven through `handle_telegram_agent_callback`, the last tap of the picker chain. Every case
//! here is refused before a process is spawned, so no `tddy-coder` binary, LiveKit server or git
//! remote is needed: a daemon without LiveKit is refused by the spawn itself, and a chain child
//! whose parent session is gone is refused while its integration base is being merged.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::config::{AllowedAgent, DaemonConfig};
use tddy_projects::project_storage::{self, ProjectData};
use tddy_telegram_control::telegram_notifier::InMemoryTelegramSender;
use tddy_telegram_control::telegram_session_control::{
    TelegramSessionControlHarness, TelegramWorkflowSpawn,
};

const OPERATOR_CHAT: i64 = 424_242;
const SESSION_ID: &str = "019d6392-3cff-0001-aaaa-0000000000a1";
const GONE_PARENT_SESSION_ID: &str = "019d6392-3cff-0001-aaaa-0000000000ff";
const THE_ONLY_PROJECT: usize = 0;
const THE_ONLY_AGENT: usize = 0;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A daemon's data root with one registered project and an empty session directory for
/// [`SESSION_ID`].
struct ADaemonWithOneProject {
    _root: tempfile::TempDir,
    sessions_base: PathBuf,
    projects_dir: PathBuf,
}

impl ADaemonWithOneProject {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let sessions_base = root.path().join("sessions");
        let projects_dir = root.path().join("projects");
        let repo = root.path().join("repo");
        for dir in [&sessions_base, &projects_dir, &repo] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(unified_session_dir_path(&sessions_base, SESSION_ID)).unwrap();
        project_storage::write_projects(&projects_dir, &[a_project_at(&repo)]).unwrap();
        Self {
            _root: root,
            sessions_base,
            projects_dir,
        }
    }

    /// The session is a chain child of a parent session that no longer exists.
    fn whose_session_chains_onto_a_gone_parent(self) -> Self {
        let session_dir = unified_session_dir_path(&self.sessions_base, SESSION_ID);
        std::fs::write(
            session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
            format!(
                "session_id: {SESSION_ID}\n\
                 project_id: proj-a\n\
                 created_at: '2026-10-22T10:00:00Z'\n\
                 updated_at: '2026-10-22T10:00:00Z'\n\
                 status: active\n\
                 previous_session_id: {GONE_PARENT_SESSION_ID}\n"
            ),
        )
        .unwrap();
        self
    }

    /// The picker harness over this daemon, with Claude as the one allowed agent and no LiveKit.
    fn harness(
        &self,
    ) -> (
        TelegramSessionControlHarness<InMemoryTelegramSender>,
        Arc<InMemoryTelegramSender>,
    ) {
        let sender = Arc::new(InMemoryTelegramSender::new());
        let config = DaemonConfig {
            allowed_agents: vec![AllowedAgent {
                id: "claude".to_string(),
                label: Some("Claude".to_string()),
            }],
            ..Default::default()
        };
        let workflow_spawn = Arc::new(TelegramWorkflowSpawn {
            config: Arc::new(config),
            spawn_client: None,
            os_user: "n/a".to_string(),
            tddy_data_dir: self.sessions_base.clone(),
            projects_dir_override: Some(self.projects_dir.clone()),
            telegram_hooks: None,
            child_grpc_by_session: Arc::new(Mutex::new(HashMap::new())),
            elicitation_select_options: Arc::new(Mutex::new(HashMap::new())),
            elicitation_multi_select_meta: Arc::new(Mutex::new(HashMap::new())),
            pending_elicitation_other: Arc::new(Mutex::new(HashMap::new())),
            claude_cli_manager: Arc::new(
                tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new(),
            ),
        });
        let harness = TelegramSessionControlHarness::with_workflow_spawn(
            vec![OPERATOR_CHAT],
            self.sessions_base.clone(),
            sender.clone(),
            Some(workflow_spawn),
            None,
        );
        (harness, sender)
    }
}

fn a_project_at(repo: &Path) -> ProjectData {
    ProjectData {
        project_id: "proj-a".to_string(),
        name: "Project A".to_string(),
        git_url: "https://example.invalid/a.git".to_string(),
        main_repo_path: repo.to_string_lossy().to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: HashMap::new(),
        accounts: Vec::new(),
        local_gh: false,
        home_agent: None,
    }
}

async fn the_operator_picks_the_agent(
    harness: &TelegramSessionControlHarness<InMemoryTelegramSender>,
) -> Result<(), String> {
    harness
        .handle_telegram_agent_callback(OPERATOR_CHAT, THE_ONLY_AGENT, THE_ONLY_PROJECT, SESSION_ID)
        .await
        .map_err(|e| format!("{e:#}"))
}

// ---------------------------------------------------------------------------
// A daemon that cannot host the session's room
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_pick_on_a_daemon_without_livekit_is_refused_with_the_reason() {
    // Given
    let daemon = ADaemonWithOneProject::new();
    let (harness, _) = daemon.harness();

    // When
    let outcome = the_operator_picks_the_agent(&harness).await;

    // Then
    assert_eq!(outcome, Err("LiveKit not configured".to_string()));
}

#[tokio::test]
async fn an_agent_pick_on_a_daemon_without_livekit_announces_no_workflow() {
    // Given
    let daemon = ADaemonWithOneProject::new();
    let (harness, sender) = daemon.harness();

    // When
    let _ = the_operator_picks_the_agent(&harness).await;

    // Then — "Workflow started" is only ever sent for a session that was spawned
    assert_eq!(sender.recorded(), Vec::<(i64, String)>::new());
}

// ---------------------------------------------------------------------------
// A chain child
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_chain_child_whose_parent_session_is_gone_is_refused_before_it_spawns() {
    // Given
    let daemon = ADaemonWithOneProject::new().whose_session_chains_onto_a_gone_parent();
    let (harness, _) = daemon.harness();

    // When
    let outcome = the_operator_picks_the_agent(&harness).await;

    // Then — refused at the base merge, not by the spawn's own LiveKit check
    let error = outcome.expect_err("a chain child of a gone parent has no base to start from");
    assert!(
        error.contains("parent session not found"),
        "the refusal names the missing parent: {error:?}"
    );
}
