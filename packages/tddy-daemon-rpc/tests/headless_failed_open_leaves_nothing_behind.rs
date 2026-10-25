//! A headless conversation whose open fails after its launch was planned leaves nothing behind: no
//! approval route holding its agent's credential, and no `headless/<conversation>/` directory.
//!
//! The plan is the launcher's: it binds the conversation's approvals to the agent's credential and
//! writes the conversation's files. The production launcher does that only for a session run as the
//! daemon's own OS user, which the test host's session is not, so the launcher here is a double that
//! does the same two things the production plan does, through the same `ApprovalRoutes` and the same
//! directory. What fails afterwards is real: the conversation's transcript cannot be opened.
//!
//! Backlog: docs/dev/todo/2026-10-25-headless-approval-route-residuals.md

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serial_test::serial;

use tddy_coding_subagent::launcher::headless_conversation_dir;
use tddy_coding_subagent::{ApprovalRoutes, CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, HeadlessCommand, McpServerSpec, SessionMetadata};
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::Request;
use tddy_service::acp_replay::ACP_TRANSCRIPT_FILENAME;
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AgentPlacement, AttachSessionAgentRequest, OpenAgentConversationRequest,
    SessionAgentService as _,
};
use tddy_service::subagent_conversation_replay::conversation_transcript_dir;
use tddy_session_lifecycle::test_util::{test_host, TEST_TOKEN};

const SESSION_ID: &str = "1780828020298-headless-open";
const CONVERSATION: &str = "conv-failed-open";
/// The credential the plan binds the conversation's approvals to.
const AGENT_TOKEN: &str = "minted-agent-token";

/// Plans a headless conversation as the production launcher does — binds its approvals, writes its
/// files — and hands back a command that is never run.
struct PlanningLauncher {
    routes: Arc<ApprovalRoutes>,
    tddy_data_dir: PathBuf,
}

#[async_trait::async_trait]
impl CodingAgentLauncher for PlanningLauncher {
    async fn command_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        Err(SubagentError::from(
            "this launcher plans headless conversations only",
        ))
    }

    async fn mcp_servers_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        Err(SubagentError::from(
            "this launcher plans headless conversations only",
        ))
    }

    async fn headless_command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<HeadlessCommand, SubagentError> {
        let conversation = launch
            .conversation_id
            .as_deref()
            .expect("a clone-placed launch names its conversation");
        self.routes
            .register_launch(conversation, &launch.session_id, AGENT_TOKEN);
        let dir = headless_conversation_dir(&self.tddy_data_dir, conversation);
        std::fs::create_dir_all(&dir).expect("the conversation's headless dir");
        let config = dir.join("mcp.json");
        std::fs::write(&config, "{}").expect("the conversation's MCP config");
        Ok(HeadlessCommand {
            program: PathBuf::from("/nonexistent/claude-is-never-run"),
            cwd: Some(launch.worktree.clone()),
            mcp_config: Some(config),
            ..HeadlessCommand::default()
        })
    }
}

fn git(dir: &Path, args: &[&str]) {
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
}

/// A daemon serving one session on a git checkout, offering a headless `claude` coding agent planned
/// by [`PlanningLauncher`], and the routes that launcher binds approvals in.
struct AHeadlessSession {
    service: TestDaemon,
    routes: Arc<ApprovalRoutes>,
    tddy_data_dir: PathBuf,
    session_dir: PathBuf,
    _sessions: tempfile::TempDir,
}

fn a_session_offering_a_headless_claude() -> AHeadlessSession {
    let sessions = tempfile::tempdir().expect("sessions");
    let agents = sessions.path().join("agents");
    std::fs::create_dir_all(&agents).expect("agents dir");
    std::fs::write(
        agents.join("plain-claude.yaml"),
        "name: plain-claude\nharness: claude\nmodel: sonnet\n",
    )
    .expect("profile");
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
        "project_id": "project-with-a-headless-agent",
        "created_at": "2026-10-25T10:00:00Z",
        "updated_at": "2026-10-25T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("write metadata");

    // The test host keeps its data in the sessions base.
    let tddy_data_dir = sessions.path().to_path_buf();
    let routes = Arc::new(ApprovalRoutes::default());
    let launcher = Arc::new(PlanningLauncher {
        routes: Arc::clone(&routes),
        tddy_data_dir: tddy_data_dir.clone(),
    });
    let service = TestDaemon::from_host(
        test_host(sessions.path().to_path_buf())
            .with_approval_routes(Arc::clone(&routes))
            .with_coding_agent_launcher(launcher),
    );
    service.as_arc().install_sandbox_rpc_bridge();
    AHeadlessSession {
        service,
        routes,
        tddy_data_dir,
        session_dir,
        _sessions: sessions,
    }
}

impl AHeadlessSession {
    async fn attached_clone_placed(&self, name: &str) -> String {
        let agent_id = self
            .service
            .list_subagents(Request::direct(ListSubagentsRequest {}))
            .await
            .expect("ListSubagents")
            .into_inner()
            .subagents
            .into_iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("a '{name}' row"))
            .agent_id;
        self.service
            .session_agents_service()
            .attach_session_agent(Request::direct(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.clone(),
                placement: AgentPlacement::Clone as i32,
            }))
            .await
            .expect("attach");
        agent_id
    }

    /// Make `conversation`'s transcript unreadable: its file is a directory.
    fn with_an_unreadable_transcript_for(&self, conversation: &str) {
        let transcript = conversation_transcript_dir(&self.session_dir, conversation)
            .join(ACP_TRANSCRIPT_FILENAME);
        std::fs::create_dir_all(transcript).expect("a directory where the transcript file goes");
    }

    async fn open(&self, agent_id: &str, conversation: &str) -> Result<String, tddy_rpc::Status> {
        self.service
            .session_agents_service()
            .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                conversation_id: conversation.to_string(),
                system_prompt: String::new(),
            }))
            .await
            .map(|response| response.into_inner().conversation_id)
    }

    /// Why an approval for `conversation` is refused, presented with the agent's own credential.
    async fn approval_refusal_for(&self, conversation: &str) -> String {
        self.routes
            .request(
                conversation,
                SESSION_ID,
                AGENT_TOKEN,
                "Bash",
                &serde_json::json!({ "command": "cargo test" }),
            )
            .await
            .expect_err("no approval is answered for a conversation that never opened")
    }
}

#[tokio::test]
#[serial]
async fn an_open_that_fails_after_the_plan_leaves_no_approval_route_and_no_headless_files() {
    // Given a headless agent whose conversation's transcript cannot be opened
    let session = a_session_offering_a_headless_claude();
    let agent_id = session.attached_clone_placed("plain-claude").await;
    session.with_an_unreadable_transcript_for(CONVERSATION);

    // When the conversation is opened
    let opened = session.open(&agent_id, CONVERSATION).await;

    // Then the open failed, and neither the route nor the conversation's files outlived it
    let error = opened.expect_err("an unreadable transcript fails the open");
    assert!(
        error.message().contains("transcript"),
        "the open failed where the test made it fail: {}",
        error.message()
    );
    let dir = headless_conversation_dir(&session.tddy_data_dir, CONVERSATION);
    assert!(
        !dir.exists(),
        "the conversation's headless files were removed: {}",
        dir.display()
    );
    let refusal = session.approval_refusal_for(CONVERSATION).await;
    assert!(
        refusal.contains("is not open on this daemon"),
        "the approval route was removed, not left inert: {refusal}"
    );
}
