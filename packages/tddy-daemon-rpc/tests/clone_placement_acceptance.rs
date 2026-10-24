//! Acceptance tests: `clone` placement — a conversation of a clone-placed agent works on a worktree
//! and branch of its own instead of the session's.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-clone-placement.md

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, SessionMetadata};
use tddy_daemon_rpc::test_util::{test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AttachSessionAgentRequest, OpenAgentConversationRequest, SessionAgentService as _,
};
use tddy_service::proto::types::AgentPlacement;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const SESSION_ID: &str = "1780828020298-clone";

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// Records every launch and starts the stub agent for it.
struct RecordingLauncher {
    launches: Mutex<Vec<CodingAgentLaunch>>,
}

#[async_trait::async_trait]
impl CodingAgentLauncher for RecordingLauncher {
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        self.launches.lock().expect("launches").push(launch.clone());
        Ok(AcpAgentCommand {
            program: stub_agent_path(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: Some(launch.worktree.clone()),
        })
    }

    /// These sessions are not managed, so the agent is handed no MCP exec tools.
    async fn mcp_servers_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<Vec<tddy_core::McpServerSpec>, SubagentError> {
        Ok(Vec::new())
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A session whose worktree is a real git checkout, offering a coding agent and a model agent.
struct ACloneSession {
    service: TestDaemon,
    worktree: PathBuf,
    launcher: Arc<RecordingLauncher>,
    _sessions: tempfile::TempDir,
}

fn a_session_with_a_git_worktree() -> ACloneSession {
    let sessions = tempfile::tempdir().expect("sessions");
    let agents = sessions.path().join("agents");
    std::fs::create_dir_all(&agents).expect("agents dir");
    std::fs::write(
        agents.join("codex-fast.yaml"),
        "name: codex-fast\nharness: codex-acp\nmodel: stub-model\n",
    )
    .expect("profile");
    std::fs::write(
        agents.join("explorer.yaml"),
        "name: explorer\nmodel: stub-model\nbase_url: http://127.0.0.1:9\ntools: [READ]\nreplaces: []\n",
    )
    .expect("def");
    let worktree = sessions.path().join("worktree");
    std::fs::create_dir_all(&worktree).expect("worktree");
    git(&worktree, &["init", "-q", "-b", "main"]);
    std::fs::write(worktree.join("lib.rs"), "fn main() {}\n").expect("file");
    git(&worktree, &["add", "."]);
    git(
        &worktree,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    let session_dir = unified_session_dir_path(sessions.path(), SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let mut metadata: SessionMetadata = serde_json::from_value(serde_json::json!({
        "session_id": SESSION_ID,
        "project_id": "project-with-clones",
        "created_at": "2026-10-24T10:00:00Z",
        "updated_at": "2026-10-24T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("write metadata");
    let launcher = Arc::new(RecordingLauncher {
        launches: Mutex::new(Vec::new()),
    });
    let service =
        test_service_with_coding_agent_launcher(sessions.path().to_path_buf(), launcher.clone());
    ACloneSession {
        service,
        worktree,
        launcher,
        _sessions: sessions,
    }
}

impl ACloneSession {
    async fn agent_id(&self, name: &str) -> String {
        self.service
            .list_subagents(Request::direct(ListSubagentsRequest {}))
            .await
            .expect("ListSubagents")
            .into_inner()
            .subagents
            .into_iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("a '{name}' row"))
            .agent_id
    }

    async fn attach(
        &self,
        agent_id: &str,
        placement: AgentPlacement,
    ) -> tddy_service::proto::session_agents_svc::SessionAgentRoster {
        self.service
            .session_agents_service()
            .attach_session_agent(Request::direct(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                placement: placement as i32,
            }))
            .await
            .expect("attach")
            .into_inner()
    }

    async fn open(&self, agent_id: &str, conversation_id: &str) {
        self.service
            .session_agents_service()
            .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                conversation_id: conversation_id.to_string(),
                system_prompt: String::new(),
            }))
            .await
            .expect("open");
    }
}

#[tokio::test]
#[serial]
async fn attaching_with_clone_records_the_placement() {
    // Given
    let session = a_session_with_a_git_worktree();
    let explorer = session.agent_id("explorer").await;

    // When
    let roster = session.attach(&explorer, AgentPlacement::Clone).await;

    // Then
    let entry = roster
        .agents
        .iter()
        .find(|e| e.agent_id == explorer)
        .expect("entry");
    assert_eq!(entry.placement, AgentPlacement::Clone as i32);
}

#[tokio::test]
#[serial]
async fn a_clone_coding_agent_conversation_launches_on_a_worktree_and_branch_of_its_own() {
    // Given
    let session = a_session_with_a_git_worktree();
    let codex = session.agent_id("codex-fast").await;
    session.attach(&codex, AgentPlacement::Clone).await;

    // When
    session.open(&codex, "conv-clone").await;

    // Then
    let launch = session.launcher.launches.lock().expect("launches")[0].clone();
    assert_ne!(
        launch.worktree, session.worktree,
        "not the session's worktree"
    );
    assert_eq!(
        git(&launch.worktree, &["branch", "--show-current"]),
        format!("tddy/subagent/{SESSION_ID}/conv-clone")
    );
    assert!(
        launch.worktree.join("lib.rs").exists(),
        "the clone starts from the caller's files"
    );
}

#[tokio::test]
#[serial]
async fn a_shared_coding_agent_conversation_launches_on_the_session_worktree() {
    // Given
    let session = a_session_with_a_git_worktree();
    let codex = session.agent_id("codex-fast").await;
    session.attach(&codex, AgentPlacement::Shared).await;

    // When
    session.open(&codex, "conv-shared").await;

    // Then
    let launch = session.launcher.launches.lock().expect("launches")[0].clone();
    assert_eq!(launch.worktree, session.worktree);
}
