//! A daemon-run conversation's finished turn, recorded as the ACP frames the web replays.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_agent_backend::AcpUpdate;
use tddy_core::SessionAgentRecord;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    ContentBlock, PermissionChoice, PermissionRequestView, PermissionToolCall, PromptOutcome,
    StopReason, SubagentError, SubagentSession, TurnRequest,
};
use tddy_rpc::{Request, Status};
use tddy_service::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, StopReason as AcpStopReason,
};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, OpenAgentConversationRequest, PromptAgentConversationRequest,
    ResumeAgentConversationRequest, SessionAgentRoster, SessionAgentService,
};
use tddy_service::subagent_conversation_replay::{
    conversation_transcript_dir, read_conversation_transcript,
};
use tddy_session_agents::conversation_transcript::record_turn;
use tddy_session_agents::ports::{
    AdmittedAgent, AgentAdmission, AgentCatalog, AgentConversationPeers, AgentSessions,
    RosterBroadcast, SessionAgentPorts,
};
use tddy_session_agents::session_agent_clone::SessionAgentCloneStore;
use tddy_session_agents::session_agent_roster::SessionAgentRosterStore;
use tddy_session_agents::session_agent_status::SessionAgentActivityStore;
use tddy_session_agents::{OpenAgentConversations, SessionAgentServiceImpl};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{broadcast, Notify};

/// What each recorded frame is, as `user:<text>`, `agent:<text>`, `tool:<title>` or
/// `permission:<title>`.
fn shapes(frames: &[AcpAgentMessage]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(notification)) => notification
                .update
                .as_ref()
                .and_then(|update| update.update.as_ref())
                .map(|update| match update {
                    session_update::Update::UserMessageChunk(chunk) => {
                        format!("user:{}", text_of(chunk))
                    }
                    session_update::Update::AgentMessageChunk(chunk) => {
                        format!("agent:{}", text_of(chunk))
                    }
                    session_update::Update::ToolCall(call) => format!("tool:{}", call.title),
                    other => format!("other:{other:?}"),
                }),
            _ => None,
        })
        .collect()
}

fn text_of(chunk: &tddy_service::proto::acp::ContentChunk) -> String {
    use tddy_service::proto::acp::content_block::Block;
    match chunk.content.as_ref().and_then(|c| c.block.as_ref()) {
        Some(Block::Text(text)) => text.text.clone(),
        _ => String::new(),
    }
}

#[test]
fn a_finished_turn_records_its_prompt_and_answer() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");
    let outcome = PromptOutcome::new(
        StopReason::EndTurn,
        vec![ContentBlock::text("main is in src/main.rs")],
        TokenUsage::default(),
    );

    // When
    record_turn(
        session_dir.path(),
        "conv-1",
        Some("where is main?"),
        &outcome,
        1_000,
    )
    .expect("the turn is recorded");

    // Then
    let frames = read_conversation_transcript(session_dir.path(), "conv-1")
        .expect("read")
        .expect("recorded");
    assert_eq!(
        shapes(&frames),
        ["user:where is main?", "agent:main is in src/main.rs"]
    );
}

#[test]
fn a_turn_paused_on_a_permission_request_records_the_request() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");
    let mut outcome = PromptOutcome::new(
        StopReason::PermissionRequested,
        Vec::new(),
        TokenUsage::default(),
    );
    outcome.permission_request = Some(PermissionRequestView {
        request_id: "perm-1".to_string(),
        tool_call: PermissionToolCall {
            title: "Run cargo test".to_string(),
            kind: "execute".to_string(),
            raw_input: None,
        },
        options: vec![PermissionChoice {
            option_id: "allow-once".to_string(),
            name: "Allow once".to_string(),
            kind: "allow_once".to_string(),
        }],
    });

    // When
    record_turn(
        session_dir.path(),
        "conv-1",
        Some("test it"),
        &outcome,
        1_000,
    )
    .expect("the turn is recorded");

    // Then
    let frames = read_conversation_transcript(session_dir.path(), "conv-1")
        .expect("read")
        .expect("recorded");
    assert!(
        shapes(&frames).contains(&"tool:Permission: Run cargo test".to_string()),
        "the pending request is an entry naming the tool: {:?}",
        shapes(&frames)
    );
}

// ─── a daemon-run turn taken through the service ────────────────────────────────────────────────

const THIS_DAEMON: &str = "udoo";
const A_SESSION: &str = "01a04d08-2bbf-7850-ae60-0df89791608a";
const A_SESSION_TOKEN: &str = "session-token-for-the-facilitating-daemon";
const AN_AGENT: &str = "FastContext@udoo";
const CONVERSATION: &str = "conv-1";
const THE_ANSWER: &str = "main is in src/main.rs";

/// A turn loop that answers every turn with [`THE_ANSWER`].
struct AnswersEveryTurn;

#[async_trait]
impl SubagentSession for AnswersEveryTurn {
    async fn take_turn(&mut self, _request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        Ok(PromptOutcome::new(
            StopReason::EndTurn,
            vec![ContentBlock::text(THE_ANSWER)],
            TokenUsage::default(),
        ))
    }

    async fn prompt(&mut self, _text: &str) -> Result<PromptOutcome, SubagentError> {
        Err(SubagentError::from(
            "these tests take turns through take_turn".to_string(),
        ))
    }

    fn model(&self) -> &str {
        "fastcontext-tools-32k:latest"
    }

    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, _max_messages: usize) -> Vec<String> {
        Vec::new()
    }
}

/// A host that holds the clone for the session, so every conversation it opens runs its loop here
/// as the owner of the agent. Opening one never reads a roster, which keeps the fixture to the
/// transcript this test is about.
struct OwnedLoopAnswering;

#[async_trait]
impl AgentSessions for OwnedLoopAnswering {
    fn hosts_a_clone_for(&self, _session_id: &str) -> bool {
        true
    }

    async fn open_owned(
        &self,
        _session_id: &str,
        _agent_id: &str,
        _system_prompt: Option<&str>,
    ) -> Result<Option<Box<dyn SubagentSession>>, Status> {
        Ok(Some(Box::new(AnswersEveryTurn)))
    }

    async fn open_local(
        &self,
        _session_id: &str,
        _conversation_id: &str,
        _session_dir: &Path,
        _record: &SessionAgentRecord,
        _session_token: &str,
        _system_prompt: Option<&str>,
    ) -> Result<Box<dyn SubagentSession>, Status> {
        Err(Status::internal(
            "this host holds the clone, so the conversation's loop is opened as its owner",
        ))
    }

    fn refuse_unready_clone(
        &self,
        _session_id: &str,
        _record: &SessionAgentRecord,
    ) -> Result<(), Status> {
        Ok(())
    }

    async fn refuse_departed_owner(&self, _daemon_instance_id: &str) -> Result<(), Status> {
        Ok(())
    }
}

/// Every other port. A turn on a conversation this host runs reaches none of them, so each one
/// names itself rather than quietly succeeding.
struct NoOtherPort;

#[async_trait]
impl AgentCatalog for NoOtherPort {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> {
        Err(Status::internal(format!(
            "a turn on an open conversation does not read '{agent_id}' from the catalog"
        )))
    }
}

#[async_trait]
impl AgentAdmission for NoOtherPort {
    async fn admit(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        _record: &SessionAgentRecord,
        _session_token: &str,
    ) -> Result<AdmittedAgent, Status> {
        Err(Status::internal(
            "a turn on an open conversation admits no agent",
        ))
    }

    async fn withdraw(&self, _session_id: &str, _admitted: &AdmittedAgent, _session_token: &str) {}

    async fn tear_down(
        &self,
        _session_id: &str,
        _daemon_instance_id: &str,
        _codebase_session_id: &str,
        _session_token: &str,
    ) -> Result<(), Status> {
        Err(Status::internal(
            "a turn on an open conversation tears no checkout down",
        ))
    }
}

#[async_trait]
impl RosterBroadcast for NoOtherPort {
    async fn broadcast(&self, _session_id: &str, _roster: &SessionAgentRoster) {}
}

#[async_trait]
impl AgentConversationPeers for NoOtherPort {
    async fn open(
        &self,
        _request: &OpenAgentConversationRequest,
        owner: &str,
        _conversation_id: &str,
    ) -> Result<(), Status> {
        Err(Status::internal(format!(
            "this host runs the conversation itself; nothing is forwarded to '{owner}'"
        )))
    }

    async fn prompt(
        &self,
        _request: &PromptAgentConversationRequest,
        owner: &str,
    ) -> Result<UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status> {
        Err(Status::internal(format!(
            "this host runs the conversation itself; nothing is prompted on '{owner}'"
        )))
    }

    async fn resume(
        &self,
        _request: &ResumeAgentConversationRequest,
        owner: &str,
    ) -> Result<UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status> {
        Err(Status::internal(format!(
            "this host runs the conversation itself; nothing is resumed on '{owner}'"
        )))
    }

    async fn cancel(
        &self,
        _session_token: &str,
        _session_id: &str,
        owner: &str,
        _conversation_id: &str,
    ) -> Result<(), Status> {
        Err(Status::internal(format!(
            "this host runs the conversation itself; nothing is cancelled on '{owner}'"
        )))
    }
}

/// The service, serving `session_dir` and holding nothing but the conversation under test.
fn a_daemon_serving(session_dir: &Path) -> SessionAgentServiceImpl {
    a_daemon_serving_with(session_dir, Arc::new(OwnedLoopAnswering))
}

/// The service, serving `session_dir` with the agent sessions `sessions` opens.
fn a_daemon_serving_with(
    session_dir: &Path,
    sessions: Arc<dyn AgentSessions>,
) -> SessionAgentServiceImpl {
    let session_dir: PathBuf = session_dir.to_path_buf();
    let clones = Arc::new(SessionAgentCloneStore::new());
    SessionAgentServiceImpl::new(SessionAgentPorts {
        session_dirs: Arc::new(move |_token, _session_id| Ok(session_dir.clone())),
        local_instance_id: THIS_DAEMON.to_string(),
        roster_keepalive: Duration::from_secs(60),
        rosters: Arc::new(SessionAgentRosterStore::new(
            Arc::clone(&clones),
            Arc::new(SessionAgentActivityStore::new()),
        )),
        clones,
        conversations: Arc::new(OpenAgentConversations::new()),
        conversation_frames: Arc::new(
            tddy_daemon_kernel::conversation_frames::ConversationFrames::default(),
        ),
        admission: Arc::new(NoOtherPort),
        catalog: Arc::new(NoOtherPort),
        broadcast: Arc::new(NoOtherPort),
        sessions,
        peers: Arc::new(NoOtherPort),
    })
}

fn an_open_of_the_conversation() -> Request<OpenAgentConversationRequest> {
    Request::direct(OpenAgentConversationRequest {
        session_token: A_SESSION_TOKEN.to_string(),
        session_id: A_SESSION.to_string(),
        daemon_instance_id: THIS_DAEMON.to_string(),
        agent_id: AN_AGENT.to_string(),
        conversation_id: CONVERSATION.to_string(),
        system_prompt: String::new(),
    })
}

fn a_prompt_on_the_conversation() -> Request<PromptAgentConversationRequest> {
    Request::direct(PromptAgentConversationRequest {
        session_token: A_SESSION_TOKEN.to_string(),
        session_id: A_SESSION.to_string(),
        daemon_instance_id: THIS_DAEMON.to_string(),
        conversation_id: CONVERSATION.to_string(),
        prompt: "where is main?".to_string(),
        ..Default::default()
    })
}

#[tokio::test]
async fn a_turn_that_cannot_be_recorded_fails_naming_the_recording_and_delivers_no_answer() {
    // Given a conversation whose transcript cannot be created, because its path is a regular file
    let session_dir = tempfile::tempdir().expect("a temporary session directory");
    let transcript = conversation_transcript_dir(session_dir.path(), CONVERSATION);
    std::fs::create_dir_all(transcript.parent().expect("a parent directory"))
        .expect("the conversations directory");
    std::fs::write(&transcript, b"occupied by a file").expect("occupy the transcript path");
    let daemon = a_daemon_serving(session_dir.path());
    daemon
        .open_agent_conversation(an_open_of_the_conversation())
        .await
        .expect("the conversation opens");

    // When a prompt is taken on it
    let response = daemon
        .prompt_agent_conversation(a_prompt_on_the_conversation())
        .await
        .expect("the turn starts");
    let mut frames = response.into_inner().into_receiver();
    let mut received = Vec::new();
    while let Some(item) = frames.recv().await {
        received.push(item);
    }

    // Then the turn fails, naming the recording failure, and no answer frame is delivered
    assert_eq!(
        received.len(),
        1,
        "the turn yields one failure and nothing else: {received:?}"
    );
    let status = received
        .remove(0)
        .expect_err("a turn that is not recorded fails rather than answering");
    assert!(
        status.message().starts_with(&format!(
            "conversation '{CONVERSATION}' turn could not be recorded: "
        )),
        "the failure names the recording: {}",
        status.message()
    );
}

/// The text a streaming coding agent sends before its turn ends.
const STREAMED_TEXT: &str = "Let me find where main is defined.";

/// A turn loop that streams its text and a tool call while its turn is open, then ends the turn: with an
/// answer once `release` is signalled (or at once when there is none), or with an error when `dies`.
struct StreamsItsTurn {
    updates: broadcast::Sender<AcpUpdate>,
    release: Option<Arc<Notify>>,
    dies: bool,
}

impl StreamsItsTurn {
    fn holding_until(release: Arc<Notify>) -> Self {
        Self {
            updates: broadcast::channel(16).0,
            release: Some(release),
            dies: false,
        }
    }

    fn dying_after_streaming() -> Self {
        Self {
            updates: broadcast::channel(16).0,
            release: None,
            dies: true,
        }
    }
}

#[async_trait]
impl SubagentSession for StreamsItsTurn {
    async fn take_turn(&mut self, _request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        let _ = self
            .updates
            .send(AcpUpdate::Text(STREAMED_TEXT.to_string()));
        let _ = self.updates.send(AcpUpdate::ToolCall {
            id: "call-1".to_string(),
            title: "Read".to_string(),
            kind: "read".to_string(),
            raw_input: Some(serde_json::json!({ "file_path": "src/parser.rs" })),
        });
        if let Some(release) = &self.release {
            release.notified().await;
        }
        if self.dies {
            return Err(SubagentError::from("the coding agent went away"));
        }
        Ok(PromptOutcome::new(
            StopReason::EndTurn,
            vec![ContentBlock::text(THE_ANSWER)],
            TokenUsage::default(),
        ))
    }

    async fn prompt(&mut self, _text: &str) -> Result<PromptOutcome, SubagentError> {
        Err(SubagentError::from(
            "these tests take turns through take_turn".to_string(),
        ))
    }

    fn model(&self) -> &str {
        "fastcontext-tools-32k:latest"
    }

    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, _max_messages: usize) -> Vec<String> {
        Vec::new()
    }

    fn updates(&self) -> Option<broadcast::Receiver<AcpUpdate>> {
        Some(self.updates.subscribe())
    }
}

/// A host that holds the clone for the session and opens the one loop it was given.
struct OwnedLoopOnce(std::sync::Mutex<Option<Box<dyn SubagentSession>>>);

#[async_trait]
impl AgentSessions for OwnedLoopOnce {
    fn hosts_a_clone_for(&self, _session_id: &str) -> bool {
        true
    }

    async fn open_owned(
        &self,
        _session_id: &str,
        _agent_id: &str,
        _system_prompt: Option<&str>,
    ) -> Result<Option<Box<dyn SubagentSession>>, Status> {
        Ok(self.0.lock().expect("the loop").take())
    }

    async fn open_local(
        &self,
        _session_id: &str,
        _conversation_id: &str,
        _session_dir: &Path,
        _record: &SessionAgentRecord,
        _session_token: &str,
        _system_prompt: Option<&str>,
    ) -> Result<Box<dyn SubagentSession>, Status> {
        Err(Status::internal(
            "this host holds the clone, so the conversation's loop is opened as its owner",
        ))
    }

    fn refuse_unready_clone(
        &self,
        _session_id: &str,
        _record: &SessionAgentRecord,
    ) -> Result<(), Status> {
        Ok(())
    }

    async fn refuse_departed_owner(&self, _daemon_instance_id: &str) -> Result<(), Status> {
        Ok(())
    }
}

/// The stop reasons a transcript records, in order.
fn stop_reasons(frames: &[AcpAgentMessage]) -> Vec<AcpStopReason> {
    frames
        .iter()
        .filter_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::Prompt(response)) => {
                AcpStopReason::try_from(response.stop_reason).ok()
            }
            _ => None,
        })
        .collect()
}

/// The conversation's transcript once it holds at least `count` frames, read as it is written.
async fn recorded_frames_of_at_least(session_dir: &Path, count: usize) -> Vec<AcpAgentMessage> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let frames = read_conversation_transcript(session_dir, CONVERSATION)
            .expect("read")
            .unwrap_or_default();
        if frames.len() >= count || std::time::Instant::now() >= deadline {
            return frames;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn a_streamed_turns_text_and_tool_calls_are_recorded_before_the_turn_ends() {
    // Given a conversation whose agent streams its text and a tool call, and holds its turn open
    let session_dir = tempfile::tempdir().expect("a temporary session directory");
    let release = Arc::new(Notify::new());
    let sessions = Arc::new(OwnedLoopOnce(std::sync::Mutex::new(Some(Box::new(
        StreamsItsTurn::holding_until(Arc::clone(&release)),
    )))));
    let daemon = a_daemon_serving_with(session_dir.path(), sessions);
    daemon
        .open_agent_conversation(an_open_of_the_conversation())
        .await
        .expect("the conversation opens");
    let response = daemon
        .prompt_agent_conversation(a_prompt_on_the_conversation())
        .await
        .expect("the turn starts");
    let mut frames = response.into_inner().into_receiver();

    // When the agent has streamed its text and its tool call, and the turn is still open
    let recorded = recorded_frames_of_at_least(session_dir.path(), 3).await;

    // Then the prompt, the text and the tool call are in the transcript, and the turn has not ended
    assert_eq!(
        shapes(&recorded),
        [
            "user:where is main?",
            &format!("agent:{STREAMED_TEXT}"),
            "tool:Read"
        ]
    );
    assert_eq!(stop_reasons(&recorded), Vec::<AcpStopReason>::new());

    // When the turn is released and ends with its answer
    release.notify_one();
    while frames.recv().await.is_some() {}

    // Then the turn's stop reason is written after what streamed
    let recorded = read_conversation_transcript(session_dir.path(), CONVERSATION)
        .expect("read")
        .expect("recorded");
    assert_eq!(stop_reasons(&recorded), [AcpStopReason::EndTurn]);
    assert_eq!(recorded.len(), 4, "{:?}", shapes(&recorded));
}

#[tokio::test]
async fn a_streamed_turn_that_dies_mid_stream_still_records_a_stop_and_fails_the_rpc() {
    // Given a conversation whose agent streams its text and a tool call, then fails before answering
    let session_dir = tempfile::tempdir().expect("a temporary session directory");
    let sessions = Arc::new(OwnedLoopOnce(std::sync::Mutex::new(Some(Box::new(
        StreamsItsTurn::dying_after_streaming(),
    )))));
    let daemon = a_daemon_serving_with(session_dir.path(), sessions);
    daemon
        .open_agent_conversation(an_open_of_the_conversation())
        .await
        .expect("the conversation opens");

    // When a prompt is taken on it
    let response = daemon
        .prompt_agent_conversation(a_prompt_on_the_conversation())
        .await
        .expect("the turn starts");
    let mut frames = response.into_inner().into_receiver();
    let mut received = Vec::new();
    while let Some(item) = frames.recv().await {
        received.push(item);
    }

    // Then the RPC returns the error, and the transcript holds the prompt, what streamed, and a stop
    assert_eq!(received.len(), 1, "{received:?}");
    let status = received
        .remove(0)
        .expect_err("a turn that fails mid-stream returns its error");
    assert!(
        status.message().contains("the coding agent went away"),
        "{}",
        status.message()
    );
    let recorded = read_conversation_transcript(session_dir.path(), CONVERSATION)
        .expect("read")
        .expect("recorded");
    assert_eq!(
        shapes(&recorded),
        [
            "user:where is main?",
            &format!("agent:{STREAMED_TEXT}"),
            "tool:Read"
        ]
    );
    assert_eq!(stop_reasons(&recorded), [AcpStopReason::Unspecified]);
}
