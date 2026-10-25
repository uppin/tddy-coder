//! A cancelled deferred request is recorded as answered by whoever cancelled it: the conversation's transcript
//! holds the request's record completed, its raw output naming the cancelling actor and a cancellation. Once the
//! conversation is cancelled it is no longer open, so an answer to the request is refused as not found.
//!
//! The coding agent is `tddy-acp-stub` under a scenario that asks one permission. The setup is the one
//! `defer_permission_wire_acceptance.rs` uses, copied rather than shared so each file reads alone.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::agent_activity::{AgentActivityRecord, AGENT_ACTIVITY_FILENAME, STATUS_COMPLETED};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, McpServerSpec, SessionMetadata};
use tddy_daemon_rpc::test_util::{signing_test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::{Code, Request, Status};
use tddy_service::acp_replay::tool_call_detail_in;
use tddy_service::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, ToolCallStatus,
};
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AnswerAgentPermissionRequest, AnswerAgentPermissionResponse,
    AttachSessionAgentRequest, CancelAgentConversationRequest, CancelAgentConversationResponse,
    OpenAgentConversationRequest, PromptAgentConversationRequest, ResumeAgentConversationRequest,
    SessionAgentService as _,
};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_session_lifecycle::test_util::{SignedTestCredentials, TEST_USER};

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

const SESSION_ID: &str = "1780828020298-waiting";

/// A daemon serving one session with a coding agent attached and a conversation open on it, the
/// conversation paused on one permission request.
struct PausedConversation {
    service: TestDaemon,
    credentials: SignedTestCredentials,
    conversation_id: String,
    request_id: String,
    session_dir: PathBuf,
    /// Owns the daemon's home, the session and the worktree; dropped with the session.
    _sessions: tempfile::TempDir,
}

/// A conversation with a coding agent that asks one permission, paused on it.
async fn a_conversation_paused_on_a_permission_request() -> PausedConversation {
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
    std::fs::write(
        launcher_dir.join("scenario.json"),
        serde_json::json!({ "responses": [{
            "chunks": ["tests pass"], "tool_calls": [],
            "permission_requests": [{ "title": "Run cargo test" }],
        }] })
        .to_string(),
    )
    .expect("write scenario");
    let (service, credentials) = signing_test_service_with_coding_agent_launcher(
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
            session_token: credentials.user_token.clone(),
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
            session_token: credentials.user_token.clone(),
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

    let mut prompt = service
        .session_agents_service()
        .prompt_agent_conversation(Request::direct(PromptAgentConversationRequest {
            session_token: credentials.user_token.clone(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: conversation_id.clone(),
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

    PausedConversation {
        service,
        credentials,
        conversation_id,
        request_id,
        session_dir,
        _sessions: sessions,
    }
}

impl PausedConversation {
    /// Resume the paused turn, handing `request_id` to the user, as the main agent does.
    fn resume_deferring(
        &self,
    ) -> impl futures_util::Future<Output = Vec<AgentConversationChunk>> + '_ {
        let request = ResumeAgentConversationRequest {
            session_token: self.credentials.user_token.clone(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: self.conversation_id.clone(),
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
                .service
                .session_agents_service()
                .resume_agent_conversation(Request::direct(request))
                .await
                .expect("resuming an open conversation succeeds")
                .into_inner();
            collect_frames(&mut stream).await
        }
    }

    /// Answer `request_id` as the signed-in user, choosing `option_id`.
    async fn answer_as_the_user(
        &self,
        request_id: &str,
        option_id: &str,
    ) -> Result<AnswerAgentPermissionResponse, Status> {
        self.service
            .session_agents_service()
            .answer_agent_permission(Request::direct(AnswerAgentPermissionRequest {
                session_token: self.credentials.user_token.clone(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: self.conversation_id.clone(),
                request_id: request_id.to_string(),
                option_id: option_id.to_string(),
                cancelled: false,
            }))
            .await
            .map(|response| response.into_inner())
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
        "created_at": "2026-10-24T10:00:00Z",
        "updated_at": "2026-10-24T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("session metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    metadata
}

impl PausedConversation {
    /// Cancel the conversation over `session_token`: the daemon derives the actor from it.
    async fn cancel_over(
        &self,
        session_token: &str,
    ) -> Result<CancelAgentConversationResponse, Status> {
        self.service
            .session_agents_service()
            .cancel_agent_conversation(Request::direct(CancelAgentConversationRequest {
                session_token: session_token.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: self.conversation_id.clone(),
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// The conversation's transcript, once it has been recorded.
    fn transcript(&self) -> Vec<AcpAgentMessage> {
        read_conversation_transcript(&self.session_dir, &self.conversation_id)
            .expect("the transcript reads")
            .expect("the conversation recorded its turns")
    }
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

#[tokio::test]
#[serial]
async fn cancelling_a_deferred_request_as_the_main_agent_records_it_answered_by_the_main_agent() {
    // Given a deferred resume waiting on the user, and the main agent cancelling the conversation
    let paused = a_conversation_paused_on_a_permission_request().await;
    let request_id = paused.request_id.clone();
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;
    paused
        .cancel_over(&paused.credentials.agent_token)
        .await
        .expect("cancelling a conversation with a pending request succeeds");

    // Then the request's record is completed, naming the main agent and a cancellation
    let frames = paused.transcript();
    let call = latest_call_named(&frames, &format!("permission:{request_id}"))
        .expect("the request is recorded under permission:<requestId>");
    let session_update::Update::ToolCall(call) = call else {
        panic!("a tool call: {call:?}");
    };
    assert_eq!(
        ToolCallStatus::try_from(call.status).expect("a status"),
        ToolCallStatus::Completed
    );
    let output: serde_json::Value =
        serde_json::from_str(call.raw_output.as_deref().expect("the answer's output"))
            .expect("the output is JSON");
    assert_eq!(
        output,
        serde_json::json!({ "answeredBy": "main-agent", "cancelled": true })
    );
    let input: serde_json::Value =
        serde_json::from_str(call.raw_input.as_deref().expect("the request's input"))
            .expect("the input is JSON");
    assert_eq!(input["requestId"], request_id.as_str());
    assert!(
        tool_call_detail_in(&frames, &format!("permission:{request_id}"))
            .expect("the request has a detail")
            .raw_output
            .is_some(),
        "the web's detail for the request carries the answer"
    );
}

#[tokio::test]
#[serial]
async fn cancelling_a_deferred_request_as_a_user_records_it_answered_by_that_login() {
    // Given a deferred resume waiting on the user, and the user cancelling the conversation
    let paused = a_conversation_paused_on_a_permission_request().await;
    let request_id = paused.request_id.clone();
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;
    paused
        .cancel_over(&paused.credentials.user_token)
        .await
        .expect("cancelling a conversation with a pending request succeeds");

    // Then the request's record is completed, naming the user's login and a cancellation
    let frames = paused.transcript();
    let Some(session_update::Update::ToolCall(call)) =
        latest_call_named(&frames, &format!("permission:{request_id}"))
    else {
        panic!("the request is recorded under permission:<requestId>: {frames:?}");
    };
    let output: serde_json::Value =
        serde_json::from_str(call.raw_output.as_deref().expect("the answer's output"))
            .expect("the output is JSON");
    assert_eq!(
        output,
        serde_json::json!({ "answeredBy": TEST_USER, "cancelled": true })
    );
}

#[tokio::test]
#[serial]
async fn an_answer_to_a_cancelled_request_is_refused_naming_the_conversation() {
    // Given a deferred request whose conversation the main agent cancelled
    let paused = a_conversation_paused_on_a_permission_request().await;
    let request_id = paused.request_id.clone();
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;
    paused
        .cancel_over(&paused.credentials.agent_token)
        .await
        .expect("cancelling a conversation with a pending request succeeds");

    // When the user answers the request
    let refusal = paused
        .answer_as_the_user(&request_id, "allow-once")
        .await
        .expect_err("an answer to a cancelled request is refused");

    // Then it is refused as not found, naming the conversation
    assert_eq!(refusal.code, Code::NotFound);
    assert!(
        refusal.message.contains(&paused.conversation_id),
        "{}",
        refusal.message
    );
}

/// Cancelling a deferred request completes the conversation's status row: the row that said the turn was waiting on the
/// user says it has ended, and it is the newest row for the conversation.
#[tokio::test]
#[serial]
async fn cancelling_a_deferred_request_completes_the_conversations_status_row() {
    // Given a deferred resume waiting on the user
    let paused = a_conversation_paused_on_a_permission_request().await;
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;

    // When the main agent cancels the conversation
    paused
        .cancel_over(&paused.credentials.agent_token)
        .await
        .expect("cancelling a conversation with a pending request succeeds");

    // Then the conversation's newest status row is completed
    let call_id = format!("subagent-conversation:{}", paused.conversation_id);
    let rows: Vec<AgentActivityRecord> =
        std::fs::read_to_string(paused.session_dir.join(AGENT_ACTIVITY_FILENAME))
            .expect("the activity log exists")
            .lines()
            .filter_map(|line| serde_json::from_str::<AgentActivityRecord>(line).ok())
            .filter(|record| record.call_id == call_id)
            .collect();
    let newest = rows.last().expect("the conversation has a status row");
    assert_eq!(newest.status, STATUS_COMPLETED, "{rows:?}");
}
