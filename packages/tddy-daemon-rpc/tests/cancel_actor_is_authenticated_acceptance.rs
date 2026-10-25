//! A cancel records the request it ends as answered by the caller the daemon authenticated, never by a login the
//! request carries: the daemon reads the actor from the session token, so a caller cannot name another login.
//!
//! The coding agent is `tddy-acp-stub` under a scenario that asks one permission. The setup is the one
//! `defer_permission_wire_acceptance.rs` uses, copied rather than shared so each file reads alone.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, McpServerSpec, SessionMetadata};
use tddy_daemon_rpc::test_util::{test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::{Request, Status};
use tddy_service::proto::acp::{acp_agent_message, session_update, AcpAgentMessage};
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AttachSessionAgentRequest, CancelAgentConversationRequest,
    CancelAgentConversationResponse, OpenAgentConversationRequest, PromptAgentConversationRequest,
    ResumeAgentConversationRequest, SessionAgentService as _,
};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_session_lifecycle::test_util::{TEST_TOKEN, TEST_USER};

/// `tddy-acp-stub` in the target directory cargo builds into.
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

/// The `tddy-tools` the daemon's launcher resolves a managed agent's MCP server to.
const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";

/// Launches every coding agent as `tddy-acp-stub`, scripted by `scenario.json` and recording into
/// `record.jsonl`, both in `dir`.
struct ScenarioLauncher {
    dir: PathBuf,
    daemons_own: DaemonCodingAgentLauncher,
}

impl ScenarioLauncher {
    fn new(dir: &Path) -> Arc<Self> {
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
            daemons_own,
        })
    }
}

#[async_trait::async_trait]
impl CodingAgentLauncher for ScenarioLauncher {
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
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
        self.daemons_own.mcp_servers_for(launch).await
    }
}

const SESSION_ID: &str = "1780828020298-cancel-actor";

/// A daemon serving one session with a coding agent attached and a conversation open on it.
struct OpenConversation {
    service: TestDaemon,
    conversation_id: String,
    session_dir: PathBuf,
    /// Owns the daemon's home, the session and the worktree; dropped with the session.
    _sessions: tempfile::TempDir,
}

/// An open conversation whose coding agent runs under `scenario`.
async fn an_open_conversation(scenario: serde_json::Value) -> OpenConversation {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    let agents_dir = sessions.path().join("agents");
    std::fs::create_dir_all(&agents_dir).expect("agents dir");
    std::fs::write(
        agents_dir.join("codex-fast.yaml"),
        "name: codex-fast\nlabel: \"codex-fast\"\nharness: codex-acp\nmodel: stub-model\n",
    )
    .expect("write profile");
    let worktree = sessions.path().join("worktree");
    std::fs::create_dir_all(&worktree).expect("worktree");
    let session_dir = unified_session_dir_path(sessions.path(), SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    tddy_core::write_session_metadata(&session_dir, &a_session_metadata(&worktree))
        .expect("write session metadata");
    let launcher_dir = sessions.path().join("launcher");
    std::fs::create_dir_all(&launcher_dir).expect("launcher dir");
    std::fs::write(launcher_dir.join("scenario.json"), scenario.to_string())
        .expect("write scenario");
    let service = test_service_with_coding_agent_launcher(
        sessions.path().to_path_buf(),
        ScenarioLauncher::new(&launcher_dir),
    );

    let agent_id = service
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents succeeds")
        .into_inner()
        .subagents
        .into_iter()
        .find(|row| row.name == "codex-fast")
        .expect("a codex-fast row")
        .agent_id;
    service
        .session_agents_service()
        .attach_session_agent(Request::direct(AttachSessionAgentRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            agent_id: agent_id.clone(),
            placement: 0,
        }))
        .await
        .expect("attach succeeds");
    let conversation_id = service
        .session_agents_service()
        .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            agent_id,
            conversation_id: String::new(),
            system_prompt: String::new(),
        }))
        .await
        .expect("open succeeds")
        .into_inner()
        .conversation_id;

    OpenConversation {
        service,
        conversation_id,
        session_dir,
        _sessions: sessions,
    }
}

/// A conversation with a coding agent that asks one permission, paused on it.
struct PausedConversation {
    open: OpenConversation,
    request_id: String,
}

async fn a_conversation_paused_on_a_permission_request() -> PausedConversation {
    let open = an_open_conversation(serde_json::json!({ "responses": [{
        "chunks": ["tests pass"], "tool_calls": [],
        "permission_requests": [{ "title": "Run cargo test" }],
    }] }))
    .await;
    let mut prompt = open
        .service
        .session_agents_service()
        .prompt_agent_conversation(Request::direct(PromptAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: open.conversation_id.clone(),
            prompt: "Run the tests".to_string(),
            max_turns: None,
            yield_conditions_json: String::new(),
        }))
        .await
        .expect("prompting succeeds")
        .into_inner();
    let frames = collect_frames(&mut prompt).await;
    let last = frames.last().expect("a prompt produces a final frame");
    assert_eq!(last.stop_reason, "PermissionRequested");
    let request: serde_json::Value =
        serde_json::from_str(&last.permission_request_json).expect("the request is JSON");
    let request_id = request["requestId"]
        .as_str()
        .expect("the request is named")
        .to_string();

    PausedConversation { open, request_id }
}

impl PausedConversation {
    /// Resume the paused turn, handing `request_id` to the user, as the main agent does.
    fn resume_deferring(
        &self,
    ) -> impl futures_util::Future<Output = Vec<AgentConversationChunk>> + '_ {
        let request = ResumeAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: self.open.conversation_id.clone(),
            from_message_id: None,
            correction: None,
            max_turns: None,
            yield_conditions_json: String::new(),
            replacement_json: String::new(),
            permission_answer_json: String::new(),
            defer_permission_to_user: Some(self.request_id.clone()),
        };
        async move {
            let mut stream = self
                .open
                .service
                .session_agents_service()
                .resume_agent_conversation(Request::direct(request))
                .await
                .expect("resuming an open conversation succeeds")
                .into_inner();
            collect_frames(&mut stream).await
        }
    }
}

impl OpenConversation {
    /// Cancel the conversation as the signed-in caller: the request names no actor, the daemon records one from the token.
    async fn cancel_as_the_caller(&self) -> Result<CancelAgentConversationResponse, Status> {
        self.service
            .session_agents_service()
            .cancel_agent_conversation(Request::direct(CancelAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: self.conversation_id.clone(),
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// The conversation's transcript, empty when nothing has been recorded.
    fn transcript(&self) -> Vec<AcpAgentMessage> {
        read_conversation_transcript(&self.session_dir, &self.conversation_id)
            .expect("the transcript reads")
            .unwrap_or_default()
    }
}

/// Every frame of a turn's stream, up to and including the final one.
async fn collect_frames<S>(stream: &mut S) -> Vec<AgentConversationChunk>
where
    S: futures_util::Stream<Item = Result<AgentConversationChunk, Status>> + Unpin,
{
    let mut frames = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(frame) = stream.next().await {
            let frame = frame.expect("the conversation stream must not error");
            let last = frame.last;
            frames.push(frame);
            if last {
                return;
            }
        }
    })
    .await
    .expect("the turn ends within the deadline");
    frames
}

fn a_session_metadata(worktree: &Path) -> SessionMetadata {
    let mut metadata: SessionMetadata = serde_json::from_value(serde_json::json!({
        "session_id": SESSION_ID,
        "project_id": "project-with-a-coding-agent",
        "created_at": "2026-10-25T10:00:00Z",
        "updated_at": "2026-10-25T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("session metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    metadata
}

/// The latest tool-call frame the transcript holds under `tool_call_id`.
fn latest_call_named(
    frames: &[AcpAgentMessage],
    tool_call_id: &str,
) -> Option<session_update::Update> {
    frames.iter().rev().find_map(|frame| match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(note)) => note
            .update
            .as_ref()
            .and_then(|update| update.update.clone())
            .filter(|update| match update {
                session_update::Update::ToolCall(call) => {
                    call.tool_call_id.as_ref().map(|id| id.value.as_str()) == Some(tool_call_id)
                }
                _ => false,
            }),
        _ => None,
    })
}

/// The parsed raw output of the permission record `request_id` holds in `frames`.
fn permission_output(frames: &[AcpAgentMessage], request_id: &str) -> serde_json::Value {
    let Some(session_update::Update::ToolCall(call)) =
        latest_call_named(frames, &format!("permission:{request_id}"))
    else {
        panic!("the request is recorded under permission:<requestId>: {frames:?}");
    };
    serde_json::from_str(call.raw_output.as_deref().expect("the answer's output"))
        .expect("the output is JSON")
}

/// The ids of the permission records `frames` holds.
fn permission_record_ids(frames: &[AcpAgentMessage]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(note)) => {
                match note.update.as_ref()?.update.as_ref()? {
                    session_update::Update::ToolCall(call) => call
                        .tool_call_id
                        .as_ref()
                        .map(|id| id.value.clone())
                        .filter(|id| id.starts_with("permission:")),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
#[serial]
async fn a_cancel_records_the_signed_in_caller_as_the_actor() {
    // Given a deferred resume waiting on the user
    let paused = a_conversation_paused_on_a_permission_request().await;
    let request_id = paused.request_id.clone();
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;

    // When the signed-in caller cancels the conversation
    paused
        .open
        .cancel_as_the_caller()
        .await
        .expect("cancelling a conversation with a pending request succeeds");

    // Then the request's record names the authenticated user
    let frames = paused.open.transcript();
    assert_eq!(
        permission_output(&frames, &request_id),
        serde_json::json!({ "answeredBy": TEST_USER, "cancelled": true })
    );
}

#[tokio::test]
#[serial]
async fn a_cancel_of_a_deferred_request_that_names_no_actor_succeeds_and_records_the_signed_in_login(
) {
    // Given a deferred resume waiting on the user
    let paused = a_conversation_paused_on_a_permission_request().await;
    let request_id = paused.request_id.clone();
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;

    // When the signed-in caller cancels, naming no actor
    paused
        .open
        .cancel_as_the_caller()
        .await
        .expect("a cancel that names no actor succeeds: the daemon records the signed-in caller");

    // Then the request's record names the signed-in login and a cancellation
    let frames = paused.open.transcript();
    assert_eq!(
        permission_output(&frames, &request_id),
        serde_json::json!({ "answeredBy": TEST_USER, "cancelled": true })
    );
}

#[tokio::test]
#[serial]
async fn a_cancel_with_no_pending_request_succeeds_and_records_no_permission() {
    // Given an open conversation that holds no permission request
    let open = an_open_conversation(serde_json::json!({ "responses": [{
        "chunks": ["ready"], "tool_calls": [], "permission_requests": [],
    }] }))
    .await;

    // When the conversation is cancelled
    open.cancel_as_the_caller()
        .await
        .expect("a cancel with no pending request succeeds");

    // Then no permission record is written
    assert_eq!(
        permission_record_ids(&open.transcript()),
        Vec::<String>::new()
    );
}
