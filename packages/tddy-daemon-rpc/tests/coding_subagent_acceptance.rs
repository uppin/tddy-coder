//! Acceptance tests: a coding agent as a session subagent, through the daemon's RPCs — listed by
//! `ListSubagents`, attached by `AttachSessionAgent`, launched for `OpenAgentConversation`, prompted
//! by `PromptAgentConversation`, ended by `CancelAgentConversation`.
//!
//! The daemon's `CodingAgentLauncher` is replaced by one that records every launch and starts
//! `tddy-acp-stub`, so what the daemon asked for — the OS user, the worktree, the MCP servers — is
//! observable without root and without a real coding agent.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_core::{AcpAgentCommand, McpServerSpec};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_rpc::test_util::{
    test_service, test_service_with_coding_agent_launcher, TestDaemon,
};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest, SubagentInfo};
use tddy_service::proto::session_agents_svc::{
    AttachSessionAgentRequest, CancelAgentConversationRequest, OpenAgentConversationRequest,
    PromptAgentConversationRequest, SessionAgentService as _,
};
use tddy_service::proto::types::AgentKind;
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const TURN_DEADLINE: Duration = Duration::from_secs(10);

/// `tddy-acp-stub` in the target directory cargo builds into: `CARGO_TARGET_DIR` when set (a
/// relative one read from the workspace root), else the workspace's `target/`, as cargo itself does.
fn stub_agent_path() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|dir| workspace.join(dir))
        .unwrap_or_else(|| workspace.join("target"));
    let stub = target.join("debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

// ---------------------------------------------------------------------------
// The recording launcher
// ---------------------------------------------------------------------------

/// Records every launch and answers it with `tddy-acp-stub`, scripted and recording into `dir`.
///
/// The MCP servers come from the daemon's own launcher (`DaemonCodingAgentLauncher`), whose tool
/// server is pointed at a fixed `tddy-tools` — so what reaches `session/new` is what production
/// hands a managed agent, not a spec this test made up.
struct RecordingLauncher {
    dir: PathBuf,
    launches: Mutex<Vec<CodingAgentLaunch>>,
    served: Mutex<Vec<McpServerSpec>>,
    daemons_own: DaemonCodingAgentLauncher,
}

/// The `tddy-tools` the daemon's launcher resolves a managed agent's MCP server to.
const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";

impl RecordingLauncher {
    fn into(dir: &Path, answer: serde_json::Value) -> Arc<Self> {
        std::fs::write(
            dir.join("scenario.json"),
            serde_json::json!({ "responses": [answer] }).to_string(),
        )
        .expect("write scenario");
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
            dir: dir.to_path_buf(),
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

    fn recorded(&self, event: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.dir.join("record.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON line"))
            .filter(|line| line["event"] == event)
            .collect()
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
            env: vec![
                (
                    "TDDY_ACP_SCENARIO".to_string(),
                    self.dir.join("scenario.json").display().to_string(),
                ),
                (
                    "TDDY_ACP_STUB_RECORD".to_string(),
                    self.dir.join("record.jsonl").display().to_string(),
                ),
            ],
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

// ---------------------------------------------------------------------------
// The session
// ---------------------------------------------------------------------------

/// A daemon serving one session, a harness profile in its `agents/` directory, and the launcher.
struct CodingAgentSession {
    service: TestDaemon,
    session_id: String,
    worktree: PathBuf,
    launcher: Arc<RecordingLauncher>,
    /// Owns the daemon's home, the session and the worktree; dropped with the session.
    _sessions: tempfile::TempDir,
}

struct ASession {
    managed: bool,
    profiles: Vec<(&'static str, &'static str)>,
    model_agents: Vec<&'static str>,
    answer: serde_json::Value,
    with_launcher: bool,
}

fn a_session() -> ASession {
    ASession {
        managed: false,
        profiles: vec![("codex-fast", "codex-acp")],
        model_agents: Vec::new(),
        answer: serde_json::json!({ "chunks": ["done"] }),
        with_launcher: true,
    }
}

impl ASession {
    fn managed(mut self) -> Self {
        self.managed = true;
        self
    }

    fn offering(mut self, name: &'static str, harness: &'static str) -> Self {
        self.profiles = vec![(name, harness)];
        self
    }

    fn whose_agent_answers(mut self, answer: serde_json::Value) -> Self {
        self.answer = answer;
        self
    }

    /// Also offering an OpenAI-loop model agent called `name`.
    fn and_a_model_agent(mut self, name: &'static str) -> Self {
        self.model_agents.push(name);
        self
    }

    /// On a daemon that installed no coding-agent launcher.
    fn on_a_daemon_without_a_launcher(mut self) -> Self {
        self.with_launcher = false;
        self
    }

    fn build(self) -> CodingAgentSession {
        let sessions = tempfile::tempdir().expect("sessions tempdir");
        let agents_dir = sessions.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("agents dir");
        for (name, harness) in &self.profiles {
            std::fs::write(
                agents_dir.join(format!("{name}.yaml")),
                format!("name: {name}\nlabel: \"{name}\"\nharness: {harness}\nmodel: stub-model\n"),
            )
            .expect("write profile");
        }
        for name in &self.model_agents {
            std::fs::write(
                agents_dir.join(format!("{name}-model.yaml")),
                format!(
                    "name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n"
                ),
            )
            .expect("write model agent");
        }
        let worktree = sessions.path().join("worktree");
        std::fs::create_dir_all(&worktree).expect("worktree");
        let session_id = "1780828020298-coding".to_string();
        let session_dir = unified_session_dir_path(sessions.path(), &session_id);
        std::fs::create_dir_all(&session_dir).expect("session dir");
        tddy_core::write_session_metadata(
            &session_dir,
            &a_session_metadata(&session_id, &worktree, self.managed),
        )
        .expect("write session metadata");
        let launcher_dir = sessions.path().join("launcher");
        std::fs::create_dir_all(&launcher_dir).expect("launcher dir");
        let launcher = RecordingLauncher::into(&launcher_dir, self.answer);
        let service = match self.with_launcher {
            true => test_service_with_coding_agent_launcher(
                sessions.path().to_path_buf(),
                launcher.clone(),
            ),
            false => test_service(sessions.path().to_path_buf()),
        };
        CodingAgentSession {
            service,
            session_id,
            worktree,
            launcher,
            _sessions: sessions,
        }
    }
}

impl CodingAgentSession {
    async fn listed(&self, name: &str) -> SubagentInfo {
        self.service
            .list_subagents(Request::direct(ListSubagentsRequest {}))
            .await
            .expect("ListSubagents succeeds")
            .into_inner()
            .subagents
            .into_iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("a '{name}' row"))
    }

    async fn attach(
        &self,
        agent_id: &str,
    ) -> Result<tddy_service::proto::session_agents_svc::SessionAgentRoster, tddy_rpc::Status> {
        self.service
            .session_agents_service()
            .attach_session_agent(Request::direct(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                placement: 0,
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn attached(&self, name: &str) -> String {
        let agent_id = self.listed(name).await.agent_id;
        self.attach(&agent_id).await.expect("attach succeeds");
        agent_id
    }

    async fn open(&self, agent_id: &str) -> String {
        self.try_open(agent_id, "")
            .await
            .expect("opening a coding-agent conversation succeeds")
    }

    /// Open a conversation with `agent_id`, replacing its system prompt unless that is empty.
    async fn try_open(
        &self,
        agent_id: &str,
        system_prompt: &str,
    ) -> Result<String, tddy_rpc::Status> {
        self.service
            .session_agents_service()
            .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                conversation_id: String::new(),
                system_prompt: system_prompt.to_string(),
            }))
            .await
            .map(|response| response.into_inner().conversation_id)
    }

    async fn listed_rows(&self) -> Vec<SubagentInfo> {
        self.service
            .list_subagents(Request::direct(ListSubagentsRequest {}))
            .await
            .expect("ListSubagents succeeds")
            .into_inner()
            .subagents
    }

    /// The concatenated content and the final stop reason of one prompt turn.
    async fn prompt(&self, conversation_id: &str, prompt: &str) -> (String, String) {
        let mut stream = self
            .service
            .session_agents_service()
            .prompt_agent_conversation(Request::direct(PromptAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
                prompt: prompt.to_string(),
                max_turns: None,
                yield_conditions_json: String::new(),
            }))
            .await
            .expect("prompting succeeds")
            .into_inner();
        let (mut text, mut stop_reason) = (String::new(), String::new());
        while let Some(chunk) = tokio::time::timeout(TURN_DEADLINE, stream.next())
            .await
            .expect("a chunk in time")
        {
            let chunk = chunk.expect("a chunk, not an error");
            text.push_str(&chunk.content_chunk);
            if chunk.last {
                stop_reason = chunk.stop_reason;
                break;
            }
        }
        (text, stop_reason)
    }

    async fn cancel(&self, conversation_id: &str) {
        self.service
            .session_agents_service()
            .cancel_agent_conversation(Request::direct(CancelAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
            }))
            .await
            .expect("cancel succeeds");
    }
}

fn a_session_metadata(session_id: &str, worktree: &Path, managed: bool) -> SessionMetadata {
    let mut metadata: SessionMetadata = serde_json::from_value(serde_json::json!({
        "session_id": session_id,
        "project_id": "project-with-a-coding-agent",
        "created_at": "2026-10-24T10:00:00Z",
        "updated_at": "2026-10-24T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("session metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    metadata.managed_codebase = managed;
    metadata
}

fn process_is_running(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn an_acp_harness_profile_is_listed_as_a_coding_agent_with_its_harness() {
    // Given
    let session = a_session().offering("codex-fast", "codex-acp").build();

    // When
    let row = session.listed("codex-fast").await;

    // Then
    assert_eq!(row.kind, AgentKind::CodingAgent as i32);
    assert_eq!(row.harness, "codex-acp");
    assert_eq!(row.model, "stub-model");
    assert!(row.agent_id.starts_with("codex-fast@"), "{}", row.agent_id);
}

#[tokio::test]
#[serial]
async fn an_acp_allowed_agent_is_listed_as_a_coding_agent_with_its_default_model() {
    // Given
    let config_dir = tempfile::tempdir().expect("config dir");
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        "users:\n  - github_user: \"u\"\n    os_user: \"u\"\nallowed_agents:\n  - id: claude-acp\n    label: \"Claude (ACP)\"\n  - id: cursor\n",
    )
    .expect("write config");
    let config = DaemonConfig::load(&config_path).expect("load config");
    let tddy_home = tempfile::tempdir().expect("tddy home");
    let home = tddy_home.path().to_path_buf();
    let service = TestDaemon::from_host(
        tddy_session_lifecycle::connection_service::DaemonSessionHost::new(
            config.clone(),
            Arc::new(move |_| Some(home.clone())),
            tddy_home.path().to_path_buf(),
            Arc::new(|_| Some("u".to_string())),
            None,
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
            tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
            Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
            Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
        ),
    );

    // When
    let rows = service
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents succeeds")
        .into_inner()
        .subagents;

    // Then
    let coding: Vec<&SubagentInfo> = rows
        .iter()
        .filter(|row| row.kind == AgentKind::CodingAgent as i32)
        .collect();
    assert_eq!(coding.len(), 1, "only the ACP id: {rows:?}");
    assert_eq!(coding[0].name, "claude-acp");
    assert_eq!(coding[0].label, "Claude (ACP)");
    assert_eq!(
        coding[0].model,
        tddy_core::default_model_for_agent("claude-acp")
    );
}

#[tokio::test]
#[serial]
async fn a_headless_harness_profile_is_not_listed_yet() {
    // Given
    let session = a_session().offering("plain-claude", "claude").build();

    // When
    let rows = session.listed_rows().await;

    // Then
    assert!(
        rows.iter().all(|row| row.name != "plain-claude"),
        "a headless harness is not offered yet: {rows:?}"
    );
}

#[tokio::test]
#[serial]
async fn a_coding_agent_named_like_a_model_agent_is_not_advertised() {
    // Given
    let session = a_session()
        .offering("reviewer", "claude-acp")
        .and_a_model_agent("reviewer")
        .build();

    // When
    let rows = session.listed_rows().await;

    // Then
    let kinds: Vec<i32> = rows
        .iter()
        .filter(|row| row.name == "reviewer")
        .map(|row| row.kind)
        .collect();
    assert_eq!(kinds, [AgentKind::Model as i32], "{rows:?}");
}

// ---------------------------------------------------------------------------
// Attaching
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn attaching_a_coding_agent_streams_a_coding_agent_entry() {
    // Given
    let session = a_session().offering("codex-fast", "codex-acp").build();
    let agent_id = session.listed("codex-fast").await.agent_id;

    // When
    let roster = session.attach(&agent_id).await.expect("attach succeeds");

    // Then
    let entry = roster
        .agents
        .iter()
        .find(|entry| entry.agent_id == agent_id)
        .expect("the entry");
    assert_eq!(entry.kind, AgentKind::CodingAgent as i32);
    assert_eq!(entry.harness, "codex-acp");
}

#[tokio::test]
#[serial]
async fn a_managed_session_refuses_a_harness_that_cannot_withdraw_its_native_tools() {
    // Given
    let session = a_session()
        .managed()
        .offering("codex-fast", "codex-acp")
        .build();
    let agent_id = session.listed("codex-fast").await.agent_id;

    // When
    let refusal = session
        .attach(&agent_id)
        .await
        .expect_err("codex-acp cannot be confined");

    // Then
    assert_eq!(refusal.code(), tddy_rpc::Code::FailedPrecondition);
    assert!(
        refusal.message().contains("codex-acp"),
        "names the harness: {}",
        refusal.message()
    );
}

// ---------------------------------------------------------------------------
// Conversations
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn opening_a_coding_agent_conversation_launches_it_as_the_session_user_on_the_session_worktree(
) {
    // Given
    let session = a_session().offering("codex-fast", "codex-acp").build();
    let agent_id = session.attached("codex-fast").await;

    // When
    session.open(&agent_id).await;

    // Then
    let launches = session.launcher.launches();
    assert_eq!(launches.len(), 1, "one launch: {launches:?}");
    assert_eq!(launches[0].session_os_user, "testdev");
    assert_eq!(launches[0].worktree, session.worktree);
    assert_eq!(launches[0].def.harness, "codex-acp");
    assert!(!launches[0].managed);
}

/// A managed session with a `claude-acp` coding agent, attached and opened.
async fn a_managed_claude_conversation() -> CodingAgentSession {
    let session = a_session()
        .managed()
        .offering("claude-coder", "claude-acp")
        .build();
    let agent_id = session.attached("claude-coder").await;
    session.open(&agent_id).await;
    session
}

#[tokio::test]
#[serial]
async fn a_managed_session_launches_the_coding_agent_as_managed() {
    // When
    let session = a_managed_claude_conversation().await;

    // Then
    assert!(session.launcher.launches()[0].managed);
}

#[tokio::test]
#[serial]
async fn a_managed_coding_agent_is_handed_the_launchers_tddy_tools_mcp_server() {
    // When
    let session = a_managed_claude_conversation().await;

    // Then
    let served: Vec<(String, PathBuf, Vec<String>)> = session
        .launcher
        .served()
        .into_iter()
        .map(|server| (server.name, server.command, server.args))
        .collect();
    assert_eq!(
        served,
        [(
            "tddy-tools".to_string(),
            PathBuf::from(TDDY_TOOLS_PATH),
            vec!["--mcp".to_string()]
        )]
    );
}

#[tokio::test]
#[serial]
async fn the_launchers_mcp_servers_reach_session_new() {
    // When
    let session = a_managed_claude_conversation().await;

    // Then
    let recorded = session.launcher.recorded("new_session");
    assert_eq!(
        recorded[0]["mcp_servers"],
        serde_json::json!(["tddy-tools"])
    );
}

#[tokio::test]
#[serial]
async fn a_managed_claude_agent_has_its_native_tools_withdrawn_at_session_new() {
    // When
    let session = a_managed_claude_conversation().await;

    // Then
    let recorded = session.launcher.recorded("new_session");
    let withdrawn = recorded[0]["meta"]["claudeCode"]["options"]["disallowedTools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        withdrawn.iter().any(|tool| tool == "Bash"),
        "Bash is withdrawn: {}",
        recorded[0]
    );
}

#[tokio::test]
#[serial]
async fn opening_a_coding_agent_with_a_system_prompt_is_refused() {
    // Given
    let session = a_session().build();
    let agent_id = session.attached("codex-fast").await;

    // When
    let refusal = session
        .try_open(&agent_id, "You are a careful reviewer.")
        .await
        .expect_err("a coding agent keeps its own system prompt");

    // Then
    assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        session.launcher.launches().is_empty(),
        "nothing was launched"
    );
}

#[tokio::test]
#[serial]
async fn a_daemon_without_a_launcher_refuses_to_open_a_coding_agent() {
    // Given
    let session = a_session().on_a_daemon_without_a_launcher().build();
    let agent_id = session.attached("codex-fast").await;

    // When
    let refusal = session
        .try_open(&agent_id, "")
        .await
        .expect_err("no launcher, no coding agent");

    // Then
    assert_eq!(refusal.code(), tddy_rpc::Code::FailedPrecondition);
    assert!(
        refusal.message().contains("no coding-agent launcher"),
        "{}",
        refusal.message()
    );
}

#[tokio::test]
#[serial]
async fn a_coding_agent_turn_answers_through_prompt_agent_conversation() {
    // Given
    let session = a_session()
        .whose_agent_answers(serde_json::json!({ "chunks": ["fixed ", "it"] }))
        .build();
    let agent_id = session.attached("codex-fast").await;
    let conversation_id = session.open(&agent_id).await;

    // When
    let (text, stop_reason) = session.prompt(&conversation_id, "Fix the bug").await;

    // Then
    assert_eq!(text, "fixed it");
    assert_eq!(
        stop_reason, "EndTurn",
        "the wire spelling `agent_stop_reason` writes"
    );
}

#[tokio::test]
#[serial]
async fn ending_a_coding_agent_conversation_stops_its_process() {
    // Given
    let session = a_session().build();
    let agent_id = session.attached("codex-fast").await;
    let conversation_id = session.open(&agent_id).await;
    let pid = session.launcher.recorded("started")[0]["pid"]
        .as_u64()
        .expect("the agent recorded its pid") as u32;

    // When
    session.cancel(&conversation_id).await;

    // Then
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while process_is_running(pid) && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!process_is_running(pid), "the agent process {pid} is gone");
}
