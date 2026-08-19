//! Acceptance tests: conversations with an agent this daemon serves itself
//! (`OpenAgentConversation`, `PromptAgentConversation`, `CancelAgentConversation`).
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (§ What detach does, § Prompting an agent)
//!
//! One daemon, one locally-owned agent, and a stub model the test drives: everything here is about
//! what happens **while a turn is in flight**, which is decidable on one host and needs a model that
//! answers when the test says so rather than when it feels like it. What genuinely needs a second
//! daemon lives in `session_agent_remote_acceptance.rs`.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::connection_service::{ConnectionServiceImpl, HOST_DOCUMENT_FRAME_BYTES};
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    AgentConversationChunk, AttachSessionAgentRequest, CancelAgentConversationRequest,
    ConnectionService as ConnectionServiceTrait, ListSubagentsRequest,
    OpenAgentConversationRequest, PromptAgentConversationRequest,
};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A daemon serving one session with one agent attached, and the stub model that agent talks to.
struct ConversingSession {
    service: ConnectionServiceImpl,
    session_id: String,
    agent_id: String,
    model: StubModel,
    _sessions: tempfile::TempDir,
}

impl ConversingSession {
    async fn open_conversation(&self) -> String {
        self.service
            .open_agent_conversation(Request::new(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: self.agent_id.clone(),
                conversation_id: String::new(),
            }))
            .await
            .expect("opening a conversation with a local agent must succeed")
            .into_inner()
            .conversation_id
    }

    async fn prompt(
        &self,
        conversation_id: &str,
    ) -> impl futures_util::Stream<Item = Result<AgentConversationChunk, tddy_rpc::Status>> + Unpin
    {
        self.service
            .prompt_agent_conversation(Request::new(PromptAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
                prompt: "where is main?".to_string(),
            }))
            .await
            .expect("prompting an open conversation must succeed")
            .into_inner()
    }

    async fn cancel(&self, conversation_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .cancel_agent_conversation(Request::new(CancelAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
            }))
            .await
            .map(|_| ())
    }
}

/// A daemon whose `agents/` directory defines one agent, pointed at `model`, attached to a live
/// session.
async fn a_session_conversing_with_a_local_agent(model: StubModel) -> ConversingSession {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    write_agent_def(sessions.path(), "explorer", &model.base_url);

    let session_id = "1780828020298-conversation".to_string();
    let session_dir = unified_session_dir_path(sessions.path(), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(&session_dir, &a_managed_session(&session_id))
        .expect("write session metadata");

    let service = test_service(sessions.path().to_path_buf());
    // Read the way a client reads it: a hand-spelled "explorer@some-host" would pass while the
    // daemon stamped something else entirely.
    let agent_id = service
        .list_subagents(Request::new(ListSubagentsRequest {}))
        .await
        .expect("listing subagents must succeed")
        .into_inner()
        .subagents
        .into_iter()
        .find(|s| s.name == "explorer")
        .expect("the fixture must advertise a def named 'explorer'")
        .agent_id;
    service
        .attach_session_agent(Request::new(AttachSessionAgentRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.clone(),
            daemon_instance_id: String::new(),
            agent_id: agent_id.clone(),
        }))
        .await
        .expect("attaching a local agent must succeed");

    ConversingSession {
        service,
        session_id,
        agent_id,
        model,
        _sessions: sessions,
    }
}

fn write_agent_def(tddy_data_dir: &Path, name: &str, base_url: &str) {
    let agents_dir = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create agents dir");
    std::fs::write(
        agents_dir.join(format!("{name}.yaml")),
        format!(
            "name: {name}\nlabel: \"{name}\"\nmodel: stub-model\n\
             base_url: {base_url}\ntools: [READ, GLOB, GREP]\nreplaces: []\n"
        ),
    )
    .expect("write agent def");
}

fn a_managed_session(session_id: &str) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "project-under-conversation".to_string(),
        created_at: "2026-08-16T10:00:00Z".to_string(),
        updated_at: "2026-08-16T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/conversation".to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: Some(true),
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
    }
}

/// An OpenAI-compatible chat-completions endpoint the test drives.
///
/// Two knobs, because both scenarios here are about *when* and *how much* the model says: `answer`
/// is the whole content it eventually returns, and `release` decides when it returns it — a model
/// that answered immediately could never be caught mid-turn.
struct StubModel {
    base_url: String,
    /// Fires once the model has received a prompt: the turn is provably in flight from here on.
    prompt_arrived: tokio::sync::mpsc::UnboundedReceiver<()>,
    _server: tokio::task::JoinHandle<()>,
}

/// A stub model that answers `answer` as soon as it is asked.
async fn a_model_answering(answer: &str) -> StubModel {
    a_model(answer.to_string(), Answering::Immediately).await
}

/// A stub model that receives the prompt and then says nothing at all — a turn that is in flight for
/// as long as the test needs it to be.
async fn a_model_that_never_answers() -> StubModel {
    a_model(String::new(), Answering::Never).await
}

enum Answering {
    Immediately,
    Never,
}

async fn a_model(answer: String, answering: Answering) -> StubModel {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub model server");
    let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (arrived_tx, prompt_arrived) = tokio::sync::mpsc::unbounded_channel();

    let server = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut scratch = [0u8; 8192];
            let _ = socket.read(&mut scratch).await;
            let _ = arrived_tx.send(());
            if matches!(answering, Answering::Never) {
                // Held open and never answered: the caller's turn stays in flight until the fixture
                // is dropped at the end of the test.
                std::future::pending::<()>().await;
            }
            let body = serde_json::json!({
                "choices": [{ "message": { "role": "assistant", "content": answer },
                              "finish_reason": "stop" }]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        }
    });

    StubModel {
        base_url,
        prompt_arrived,
        _server: server,
    }
}

impl StubModel {
    /// Block until the model has been asked something, so a test acting "mid-turn" provably is.
    async fn await_a_prompt(&mut self) {
        // 5s: one local HTTP round trip inside a turn loop that has already been started.
        tokio::time::timeout(Duration::from_secs(5), self.prompt_arrived.recv())
            .await
            .expect("the agent never asked the model anything")
            .expect("the stub model server ended before it was asked anything");
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// Every frame of an answer, drained to the `last` one.
async fn collect_frames<S>(stream: &mut S) -> Vec<AgentConversationChunk>
where
    S: futures_util::Stream<Item = Result<AgentConversationChunk, tddy_rpc::Status>> + Unpin,
{
    // 10s: a local stub model plus one turn loop, generous enough for a loaded runner.
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
    .expect("the conversation never produced its final frame");
    frames
}

/// The refusal an in-flight turn ends with, or a failure naming what arrived instead.
async fn refusal_from<S>(stream: &mut S) -> tddy_rpc::Status
where
    S: futures_util::Stream<Item = Result<AgentConversationChunk, tddy_rpc::Status>> + Unpin,
{
    // 5s: the refusal is produced by the daemon itself, with no model round trip in the way.
    tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("the cancelled turn produced nothing at all")
        .expect("the cancelled turn's stream ended instead of reporting the cancellation")
        .expect_err("a cancelled turn must not answer as if it completed")
}

// ---------------------------------------------------------------------------
// Cancelling a turn in flight
// ---------------------------------------------------------------------------

/// A cancel has to land *while the turn is running*, which is the only moment it means anything.
/// Holding the map of open conversations across the turn would make this call wait for the very turn
/// it exists to interrupt.
#[tokio::test]
async fn cancels_a_conversation_whose_turn_is_still_in_flight() {
    // Given — a turn that has reached the model and will never come back on its own
    let mut session =
        a_session_conversing_with_a_local_agent(a_model_that_never_answers().await).await;
    let conversation = session.open_conversation().await;
    let mut answer = session.prompt(&conversation).await;
    session.model.await_a_prompt().await;

    // When
    // 5s: the cancel touches nothing but an in-memory map, so anything approaching the turn's own
    // duration means it is waiting for that turn.
    let cancelled = tokio::time::timeout(Duration::from_secs(5), session.cancel(&conversation))
        .await
        .expect("the cancel waited for the turn it was meant to interrupt");

    // Then
    cancelled.expect("cancelling an open conversation must succeed");
    let refusal = refusal_from(&mut answer).await;
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains(&conversation),
        "the refusal must name the conversation it closed, was: {}",
        refusal.message()
    );
}

/// The second cancel is `NOT_FOUND`: a caller told a turn was cancelled when nothing was open would
/// go on to read a stale answer.
#[tokio::test]
async fn refuses_to_cancel_a_conversation_that_is_no_longer_open() {
    // Given
    let mut session =
        a_session_conversing_with_a_local_agent(a_model_that_never_answers().await).await;
    let conversation = session.open_conversation().await;
    let _answer = session.prompt(&conversation).await;
    session.model.await_a_prompt().await;
    session
        .cancel(&conversation)
        .await
        .expect("the first cancel must succeed");

    // When
    let result = session.cancel(&conversation).await;

    // Then
    let status = result.expect_err("cancelling a closed conversation must be refused");
    assert_eq!(status.code(), Code::NotFound);
}

// ---------------------------------------------------------------------------
// Framing an answer
// ---------------------------------------------------------------------------

/// An answer larger than one transport frame arrives in frames the transport carries whole. Over
/// LiveKit anything past `MAX_CHUNK_FRAME_BYTES` is chunk-framed, and one lost chunk frame wedges
/// the call with no error at all.
#[tokio::test]
async fn frames_an_answer_larger_than_one_transport_frame() {
    // Given — two and a bit frames' worth of answer
    let answer = "a".repeat(HOST_DOCUMENT_FRAME_BYTES * 2 + 1_024);
    let session = a_session_conversing_with_a_local_agent(a_model_answering(&answer).await).await;
    let conversation = session.open_conversation().await;

    // When
    let mut stream = session.prompt(&conversation).await;
    let frames = collect_frames(&mut stream).await;

    // Then
    let sizes: Vec<usize> = frames.iter().map(|f| f.content_chunk.len()).collect();
    assert_eq!(
        sizes,
        vec![HOST_DOCUMENT_FRAME_BYTES, HOST_DOCUMENT_FRAME_BYTES, 1_024]
    );
    let flags: Vec<bool> = frames.iter().map(|f| f.last).collect();
    assert_eq!(
        flags,
        vec![false, false, true],
        "only the final frame may be marked last, or a consumer stops on the first one"
    );
    assert_eq!(
        frames
            .iter()
            .map(|f| f.content_chunk.as_str())
            .collect::<String>(),
        answer,
        "the answer must arrive whole across its frames"
    );
}

/// The stop reason rides the final frame, and an answer that fits in one frame still produces
/// exactly one — a consumer never has to tell "said nothing" from "nothing arrived".
#[tokio::test]
async fn carries_the_stop_reason_on_the_final_frame_of_a_short_answer() {
    // Given
    let session =
        a_session_conversing_with_a_local_agent(a_model_answering("src/main.rs").await).await;
    let conversation = session.open_conversation().await;

    // When
    let mut stream = session.prompt(&conversation).await;
    let frames = collect_frames(&mut stream).await;

    // Then
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].content_chunk, "src/main.rs");
    assert_eq!(frames[0].stop_reason, "EndTurn");
    assert!(frames[0].last);
}
