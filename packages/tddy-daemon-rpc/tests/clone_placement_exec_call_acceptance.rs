//! Acceptance: a clone-placed coding agent's exec calls run in its conversation worktree, and a
//! shared coding agent's run in the session worktree.
//!
//! A managed coding agent reaches the daemon's exec tools through `tddy-tools --mcp`, which dispatches
//! each call for the conversation its environment names (`CONVERSATION_ID_ENV`). These tests pin the
//! two halves the daemon owns: the launch names the conversation, and the MCP server it is handed
//! carries that name; and an exec call the daemon receives for that conversation runs in its worktree.
//! The `tddy-tools` process itself is not run here.
//!
//! Feature doc: docs/ft/coder/managed-codebase-subagents.md § A daemon-run conversation of a `clone`-placed agent

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher, CONVERSATION_ID_ENV};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, McpServerSpec, SessionMetadata};
use tddy_daemon_rpc::test_util::{test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session_agents_svc::AgentPlacement;
use tddy_service::proto::session_agents_svc::{
    AttachSessionAgentRequest, OpenAgentConversationRequest, SessionAgentService as _,
};
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const SESSION_ID: &str = "1780828020298-exec-calls";
const CLAUDE: &str = "claude-coder";
/// The `tddy-tools` the daemon's launcher resolves a managed agent's MCP server to.
const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// Records every launch and the MCP servers each one was handed, which are the daemon's own: the
/// production launcher builds them, from a tool server pointed at a fixed `tddy-tools`.
struct RecordingLauncher {
    launches: Mutex<Vec<CodingAgentLaunch>>,
    served: Mutex<Vec<McpServerSpec>>,
    daemons_own: DaemonCodingAgentLauncher,
}

impl RecordingLauncher {
    fn new() -> Arc<Self> {
        let daemons_own = DaemonCodingAgentLauncher::new(
            Arc::new(|_| Ok(None)),
            Arc::new(|launch| {
                Ok((
                    PathBuf::from(TDDY_TOOLS_PATH),
                    vec![(
                        "TDDY_REMOTE_SESSION_ID".to_string(),
                        launch.session_id.clone(),
                    )],
                ))
            }),
        );
        Arc::new(Self {
            launches: Mutex::new(Vec::new()),
            served: Mutex::new(Vec::new()),
            daemons_own,
        })
    }

    fn launches(&self) -> Vec<CodingAgentLaunch> {
        self.launches.lock().expect("launches").clone()
    }

    fn served(&self) -> Vec<McpServerSpec> {
        self.served.lock().expect("served").clone()
    }
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

    async fn mcp_servers_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        let servers = self.daemons_own.mcp_servers_for(launch).await?;
        self.served.lock().expect("served").extend(servers.clone());
        Ok(servers)
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

/// A managed session whose worktree is a real git checkout, offering one claude-acp coding agent.
struct AManagedSession {
    service: TestDaemon,
    worktree: PathBuf,
    launcher: Arc<RecordingLauncher>,
    _sessions: tempfile::TempDir,
}

fn a_managed_session() -> AManagedSession {
    let sessions = tempfile::tempdir().expect("sessions");
    let agents = sessions.path().join("agents");
    std::fs::create_dir_all(&agents).expect("agents dir");
    std::fs::write(
        agents.join(format!("{CLAUDE}.yaml")),
        format!("name: {CLAUDE}\nlabel: \"{CLAUDE}\"\nharness: claude-acp\nmodel: stub-model\n"),
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
        "project_id": "project-with-exec-calls",
        "created_at": "2026-10-24T10:00:00Z",
        "updated_at": "2026-10-24T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    metadata.managed_codebase = true;
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("write metadata");
    let launcher = RecordingLauncher::new();
    let service =
        test_service_with_coding_agent_launcher(sessions.path().to_path_buf(), launcher.clone());
    AManagedSession {
        service,
        worktree,
        launcher,
        _sessions: sessions,
    }
}

impl AManagedSession {
    async fn agent_id(&self) -> String {
        self.service
            .list_subagents(Request::direct(ListSubagentsRequest {}))
            .await
            .expect("ListSubagents")
            .into_inner()
            .subagents
            .into_iter()
            .find(|row| row.name == CLAUDE)
            .unwrap_or_else(|| panic!("a '{CLAUDE}' row"))
            .agent_id
    }

    async fn attach(&self, agent_id: &str, placement: AgentPlacement) {
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
            .expect("attach");
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

    /// The launch the daemon made for the one conversation this session opened.
    fn the_launch(&self) -> CodingAgentLaunch {
        self.launcher.launches()[0].clone()
    }

    /// The `tddy-tools` server the agent was handed, with its environment.
    fn the_tool_server(&self) -> McpServerSpec {
        self.launcher.served()[0].clone()
    }

    /// An exec call the daemon receives for `conversation_id`, as `tddy-tools` sends it: an empty
    /// conversation id runs it in the session worktree.
    async fn exec(
        &self,
        conversation_id: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> ExecuteToolResponse {
        self.service
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
            }))
            .await
            .expect("ExecuteTool answers")
            .into_inner()
    }
}

#[tokio::test]
#[serial]
async fn a_clone_coding_agent_is_launched_naming_the_conversation_its_exec_calls_run_in() {
    // Given
    let session = a_managed_session();
    let agent = session.agent_id().await;
    session.attach(&agent, AgentPlacement::Clone).await;

    // When
    session.open(&agent, "conv-clone").await;

    // Then
    assert_eq!(
        session.the_launch().conversation_id.as_deref(),
        Some("conv-clone")
    );
}

#[tokio::test]
#[serial]
async fn a_clone_coding_agents_tool_server_is_handed_the_conversation_its_exec_calls_run_in() {
    // Given
    let session = a_managed_session();
    let agent = session.agent_id().await;
    session.attach(&agent, AgentPlacement::Clone).await;

    // When
    session.open(&agent, "conv-clone").await;

    // Then
    assert!(session
        .the_tool_server()
        .env
        .contains(&(CONVERSATION_ID_ENV.to_string(), "conv-clone".to_string())));
}

#[tokio::test]
#[serial]
async fn a_shared_coding_agents_tool_server_is_handed_no_conversation() {
    // Given
    let session = a_managed_session();
    let agent = session.agent_id().await;
    session.attach(&agent, AgentPlacement::Shared).await;

    // When
    session.open(&agent, "conv-shared").await;

    // Then
    assert_eq!(
        (
            session.the_launch().conversation_id,
            session
                .the_tool_server()
                .env
                .iter()
                .any(|(name, _)| name == CONVERSATION_ID_ENV),
        ),
        (None, false)
    );
}

#[tokio::test]
#[serial]
async fn an_exec_call_of_a_clone_conversation_lands_in_its_conversation_worktree_not_the_session_worktree(
) {
    // Given
    let session = a_managed_session();
    let agent = session.agent_id().await;
    session.attach(&agent, AgentPlacement::Clone).await;
    session.open(&agent, "conv-clone").await;
    let conversation_root = session.the_launch().worktree;

    // When
    let answer = session
        .exec(
            "conv-clone",
            "Write",
            serde_json::json!({ "path": "clone-only.txt", "contents": "written by the clone\n" }),
        )
        .await;

    // Then
    assert_eq!(
        (
            answer.is_error,
            conversation_root.join("clone-only.txt").exists(),
            session.worktree.join("clone-only.txt").exists(),
        ),
        (false, true, false)
    );
}

#[tokio::test]
#[serial]
async fn an_exec_call_of_a_shared_conversation_lands_in_the_session_worktree() {
    // Given
    let session = a_managed_session();
    let agent = session.agent_id().await;
    session.attach(&agent, AgentPlacement::Shared).await;
    session.open(&agent, "conv-shared").await;

    // When
    let answer = session
        .exec(
            "",
            "Write",
            serde_json::json!({ "path": "shared-file.txt", "contents": "written in the session\n" }),
        )
        .await;

    // Then
    assert_eq!(
        (
            answer.is_error,
            session.worktree.join("shared-file.txt").exists(),
            git(&session.worktree, &["branch", "--list", "tddy/subagent/*"]).is_empty(),
        ),
        (false, true, true)
    );
}
