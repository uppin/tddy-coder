//! A deferred resume records its conversation as waiting on the user, through the conversation's own status row:
//! running with `awaitingUser` while the request is held, running without it once the user answers, and completed
//! when the turn ends. The three records share one call id, one per conversation.
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
use tddy_core::agent_activity::{AgentActivityRecord, AGENT_ACTIVITY_FILENAME};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, McpServerSpec, SessionMetadata};
use tddy_daemon_rpc::test_util::{test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{
    ActivityService as _, StreamMode, StreamSessionActivityRequest,
};
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AnswerAgentPermissionRequest, AnswerAgentPermissionResponse,
    AttachSessionAgentRequest, OpenAgentConversationRequest, PromptAgentConversationRequest,
    ResumeAgentConversationRequest, SessionAgentService as _,
};
use tddy_service::subagent_conversation_replay::CONVERSATION_ROW_SOURCE;
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

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

    let mut prompt = service
        .session_agents_service()
        .prompt_agent_conversation(Request::direct(PromptAgentConversationRequest {
            session_token: TEST_TOKEN.to_string(),
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
            session_token: TEST_TOKEN.to_string(),
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
                session_token: TEST_TOKEN.to_string(),
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

    /// The session's live activity, as a web client follows it: records published after this call, no snapshot.
    async fn follow_live_activity(
        &self,
    ) -> tddy_worktree_service::stream::MpscResultStream<
        tddy_service::proto::activity::AgentActivityRecord,
    > {
        self.service
            .activity_service()
            .stream_session_activity(Request::direct(StreamSessionActivityRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                mode: StreamMode::LiveOnly as i32,
            }))
            .await
            .expect("following the session's activity succeeds")
            .into_inner()
    }

    /// Every row the session's activity log holds for the conversation's status row, oldest first, as written.
    fn status_rows(&self) -> Vec<AgentActivityRecord> {
        let call_id = format!("subagent-conversation:{}", self.conversation_id);
        std::fs::read_to_string(self.session_dir.join(AGENT_ACTIVITY_FILENAME))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<AgentActivityRecord>(line).ok())
            .filter(|record| record.call_id == call_id)
            .collect()
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

/// Whether a status row reads as waiting on the user: running, with `awaitingUser` set.
fn is_awaiting_the_user(record: &AgentActivityRecord) -> bool {
    record.status == tddy_core::agent_activity::STATUS_RUNNING
        && record.result.get("awaitingUser") == Some(&serde_json::Value::Bool(true))
}

#[tokio::test]
#[serial]
async fn a_deferred_resume_records_its_conversation_as_waiting_on_the_user() {
    // Given a conversation paused on a request the main agent hands to the user
    let paused = a_conversation_paused_on_a_permission_request().await;
    let resume = paused.resume_deferring();
    tokio::pin!(resume);

    // When the main agent's resume is taken, and the turn waits on the user
    let held = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;
    assert!(held.is_err(), "the deferred resume waits for the user");

    // Then the conversation's status row says it is waiting on the user
    let rows = paused.status_rows();
    assert_eq!(rows.len(), 1, "one row so far: {rows:?}");
    assert!(
        is_awaiting_the_user(&rows[0]),
        "running with awaitingUser true: {:?}",
        rows[0]
    );
    assert_eq!(rows[0].tool_name, "subagent_conversation");
    assert_eq!(rows[0].input["sessionId"], paused.conversation_id.as_str());
}

#[tokio::test]
#[serial]
async fn the_status_row_clears_when_the_user_answers_and_completes_when_the_turn_ends() {
    // Given a deferred resume waiting on the user
    let paused = a_conversation_paused_on_a_permission_request().await;
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;

    // When the user answers the request
    paused
        .answer_as_the_user(&paused.request_id, "allow-once")
        .await
        .expect("the user's answer is taken");

    // Then the newest status row is running, no longer waiting on the user
    let after_answer = paused.status_rows();
    let newest = after_answer.last().expect("a row after the answer");
    assert_eq!(newest.status, tddy_core::agent_activity::STATUS_RUNNING);
    assert!(
        newest.result.get("awaitingUser").is_none(),
        "no awaitingUser after the answer: {newest:?}"
    );

    // When the turn ends
    let frames = tokio::time::timeout(Duration::from_secs(10), &mut resume)
        .await
        .expect("the turn continued once the user answered");
    assert_eq!(frames.last().expect("a final frame").stop_reason, "EndTurn");

    // Then the status row is completed, and all three rows share the call id
    let rows = paused.status_rows();
    assert_eq!(rows.len(), 3, "awaiting, answered, completed: {rows:?}");
    assert!(is_awaiting_the_user(&rows[0]), "{:?}", rows[0]);
    assert_eq!(rows[1].status, tddy_core::agent_activity::STATUS_RUNNING);
    assert_eq!(rows[2].status, tddy_core::agent_activity::STATUS_COMPLETED);
    let call_id = &rows[0].call_id;
    assert!(rows.iter().all(|row| &row.call_id == call_id));
}

#[tokio::test]
#[serial]
async fn a_live_activity_subscriber_sees_the_conversation_waiting_on_the_user_at_once() {
    // Given a conversation paused on a request, and a web client following the session's activity live
    let paused = a_conversation_paused_on_a_permission_request().await;
    let mut live = paused.follow_live_activity().await;
    let resume = paused.resume_deferring();
    tokio::pin!(resume);

    // When the main agent hands the request to the user, and the turn waits on the user
    let held = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;
    assert!(held.is_err(), "the deferred resume waits for the user");

    // Then the live stream has already carried the conversation's status row, waiting on the user
    let call_id = format!("subagent-conversation:{}", paused.conversation_id);
    let row = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(record) = live.next().await {
            let record = record.expect("the live stream must not error");
            if record.call_id == call_id {
                return record;
            }
        }
        panic!("the live stream ended before the status row arrived");
    })
    .await
    .expect("the status row is published live, not only at the next snapshot");
    assert_eq!(row.status, tddy_core::agent_activity::STATUS_RUNNING);
}

#[tokio::test]
#[serial]
async fn every_status_row_is_marked_as_the_daemons_status_row() {
    // Given a deferred resume waiting on the user, then answered, then ended
    let paused = a_conversation_paused_on_a_permission_request().await;
    let resume = paused.resume_deferring();
    tokio::pin!(resume);
    let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;
    paused
        .answer_as_the_user(&paused.request_id, "allow-once")
        .await
        .expect("the user's answer is taken");
    tokio::time::timeout(Duration::from_secs(10), &mut resume)
        .await
        .expect("the turn continued once the user answered");

    // Then every row it wrote carries the status source, so raw activity can tell it from an agent's call
    let rows = paused.status_rows();
    assert_eq!(rows.len(), 3, "{rows:?}");
    for row in &rows {
        assert_eq!(row.source, CONVERSATION_ROW_SOURCE, "{row:?}");
    }
}
