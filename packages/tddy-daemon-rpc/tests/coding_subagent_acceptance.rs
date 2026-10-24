//! Acceptance tests: a coding agent as a session subagent, through the daemon's RPCs — listed by
//! `ListSubagents`, attached by `AttachSessionAgent`, launched for `OpenAgentConversation`, prompted
//! by `PromptAgentConversation`, ended by `CancelAgentConversation`.
//!
//! The daemon's `CodingAgentLauncher` is replaced by one that records every launch and starts
//! `tddy-acp-stub`, so what the daemon asked for — the OS user, the worktree, the MCP servers — is
//! observable without root and without a real coding agent.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::AcpAgentCommand;
use tddy_core::SessionMetadata;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_rpc::test_util::{test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest, SubagentInfo};
use tddy_service::proto::session_agents_svc::{
    AttachSessionAgentRequest, CancelAgentConversationRequest, OpenAgentConversationRequest,
    PromptAgentConversationRequest, SessionAgentService as _,
};
use tddy_service::proto::types::AgentKind;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const TURN_DEADLINE: Duration = Duration::from_secs(10);

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
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
struct RecordingLauncher {
    dir: PathBuf,
    launches: Mutex<Vec<CodingAgentLaunch>>,
}

impl RecordingLauncher {
    fn into(dir: &Path, answer: serde_json::Value) -> Arc<Self> {
        std::fs::write(
            dir.join("scenario.json"),
            serde_json::json!({ "responses": [answer] }).to_string(),
        )
        .expect("write scenario");
        Arc::new(Self {
            dir: dir.to_path_buf(),
            launches: Mutex::new(Vec::new()),
        })
    }

    fn launches(&self) -> Vec<CodingAgentLaunch> {
        self.launches.lock().expect("launches").clone()
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
    sessions: tempfile::TempDir,
}

struct ASession {
    managed: bool,
    profiles: Vec<(&'static str, &'static str)>,
    answer: serde_json::Value,
}

fn a_session() -> ASession {
    ASession {
        managed: false,
        profiles: vec![("codex-fast", "codex-acp")],
        answer: serde_json::json!({ "chunks": ["done"] }),
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
        let service = test_service_with_coding_agent_launcher(
            sessions.path().to_path_buf(),
            launcher.clone(),
        );
        CodingAgentSession {
            service,
            session_id,
            worktree,
            launcher,
            sessions,
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
        self.service
            .session_agents_service()
            .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                conversation_id: String::new(),
                system_prompt: String::new(),
            }))
            .await
            .expect("opening a coding-agent conversation succeeds")
            .into_inner()
            .conversation_id
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
    let rows = session
        .service
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents succeeds")
        .into_inner()
        .subagents;

    // Then
    assert!(
        rows.iter().all(|row| row.name != "plain-claude"),
        "a headless harness is not offered yet: {rows:?}"
    );
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

#[tokio::test]
#[serial]
async fn a_managed_session_launches_the_coding_agent_with_only_tddy_tools_exec_tools() {
    // Given
    let session = a_session()
        .managed()
        .offering("claude-coder", "claude-acp")
        .build();
    let agent_id = session.attached("claude-coder").await;

    // When
    session.open(&agent_id).await;

    // Then
    let launch = &session.launcher.launches()[0];
    assert!(launch.managed);
    let servers: Vec<&str> = launch.mcp_servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(servers, ["tddy-tools"]);
    let recorded = session.launcher.recorded("new_session");
    assert_eq!(
        recorded[0]["mcp_servers"],
        serde_json::json!(["tddy-tools"])
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
    assert_eq!(stop_reason, "EndTurn", "the wire spelling `agent_stop_reason` writes");
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
    let _ = &session.sessions;
}
