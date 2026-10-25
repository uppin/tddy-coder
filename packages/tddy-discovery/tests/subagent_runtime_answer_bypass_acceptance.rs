//! A main agent's permission answer to a remote conversation whose deferred turn holds the session is
//! sent to the daemon at once, rather than queued behind the turn that is waiting on the user. A
//! conversation nothing holds still takes its answer through its session, as before.
//!
//! The daemon is a fake `RpcClientTransport` that refuses an answer the way the daemon does while a
//! turn holds the request: at once, saying the answer was delivered. The deferred turn is a fake
//! session that holds the conversation until the test releases it, as a remote turn does while its
//! request waits for the user.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::roster::AgentConversationLink;
use tddy_discovery::subagent::{
    ContentBlock, PermissionAnswer, PromptOutcome, StopReason, SubagentError, SubagentSession,
    TurnRequest,
};
use tddy_discovery::subagent_runtime::{
    run_turn, subagent_sessions, DeferredTurn, SubagentConversation, TurnState,
};
use tddy_rpc::{RpcClientTransport, Status};
use tddy_service::proto::session_agents_svc::ResumeAgentConversationRequest;
use tddy_session_tool_client::SessionToolEnvelope;
use tokio::sync::{mpsc, oneshot, Notify};

/// The endpoint the table records for these conversations. Any value: nothing here reads it.
const A_PROVIDER: &str = "http://127.0.0.1:11434";

/// The daemon's answer to a resume that carries a permission answer while a turn holds the request:
/// refused at once, and not delivered a second time.
const DELIVERED: &str =
    "permission request 'perm-1' was answered and the answer was delivered to the coding agent; \
     the turn that deferred it carries on and returns its outcome to the call that deferred it";

/// The fake daemon. It records each answer it is sent and refuses it as the daemon does while the
/// request is held: at once, saying the answer was delivered.
#[derive(Default)]
struct RefusingDaemon {
    answers: Mutex<Vec<ResumeAgentConversationRequest>>,
}

impl RefusingDaemon {
    fn answers_sent(&self) -> Vec<ResumeAgentConversationRequest> {
        self.answers.lock().expect("read the answers").clone()
    }
}

#[async_trait]
impl RpcClientTransport for RefusingDaemon {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented(format!(
            "the fake daemon has no unary answer for {service}/{method}"
        )))
    }

    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        if method != "ResumeAgentConversation" {
            return Err(Status::unimplemented(format!(
                "the fake daemon has no stream for {service}/{method}"
            )));
        }
        let request = ResumeAgentConversationRequest::decode(request_bytes.as_slice())
            .expect("the resume decodes as ResumeAgentConversationRequest");
        if request.permission_answer_json.is_empty() {
            return Err(Status::unimplemented(
                "this fake daemon only answers permission answers",
            ));
        }
        self.answers
            .lock()
            .expect("record the answer")
            .push(request);
        Err(Status::failed_precondition(DELIVERED))
    }

    async fn call_client_stream(
        &self,
        service: &str,
        method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented(format!(
            "the conversation client makes no client-streaming call, got {service}/{method}"
        )))
    }

    async fn call_bidi_stream(
        &self,
        service: &str,
        method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented(format!(
            "the conversation client makes no bidi call, got {service}/{method}"
        )))
    }
}

/// A turn's session. A deferred turn holds it until `released` is sent; with no release it answers
/// at once. `started` is signalled when a turn begins on it.
struct TurnSession {
    started: Arc<Notify>,
    released: Option<oneshot::Receiver<()>>,
}

#[async_trait]
impl SubagentSession for TurnSession {
    async fn take_turn(&mut self, _request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        self.started.notify_one();
        if let Some(released) = self.released.take() {
            let _ = released.await;
        }
        Ok(PromptOutcome::new(
            StopReason::EndTurn,
            vec![ContentBlock::text("tests pass")],
            TokenUsage::default(),
        ))
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        "stub-model"
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

fn an_envelope() -> SessionToolEnvelope {
    SessionToolEnvelope {
        session_id: "01a04d08-2bbf-7850-ae60-0df89791608a".to_string(),
        session_token: "session-token-for-the-facilitating-daemon".to_string(),
        daemon_instance_id: "udoo".to_string(),
    }
}

/// Open a remote conversation `conversation_id` in the table, whose loop runs on `daemon`, and
/// return its session as the turns of the table see it.
async fn open_remote_conversation(
    conversation_id: &str,
    daemon: &Arc<RefusingDaemon>,
    session: TurnSession,
) -> Arc<tokio::sync::Mutex<Box<dyn SubagentSession>>> {
    let link = Arc::new(AgentConversationLink::new(
        Arc::clone(daemon) as Arc<dyn RpcClientTransport>,
        an_envelope(),
    ));
    let conversation = SubagentConversation::opened(
        "explorer".to_string(),
        None,
        Box::new(session),
        Some(link.handle("conv-remote".to_string())),
    );
    let session = Arc::clone(&conversation.session);
    subagent_sessions()
        .lock()
        .await
        .open
        .insert(conversation_id.to_string(), conversation);
    session
}

/// Register a turn under `response_id` on `conversation_id`, as a tool call does before it runs one.
async fn register_turn(response_id: &str, conversation_id: &str) {
    subagent_sessions().lock().await.pending.start(
        response_id,
        conversation_id,
        A_PROVIDER,
        tokio::spawn(std::future::pending::<()>()).abort_handle(),
    );
}

/// What the turn registered under `response_id` has resolved to, as its caller reads it.
async fn answer_of(response_id: &str) -> String {
    let watched = subagent_sessions()
        .lock()
        .await
        .pending
        .watch(response_id)
        .expect("the turn is registered");
    let state = watched.borrow().clone();
    match state {
        TurnState::Done(result) => result,
        TurnState::Running => panic!("{response_id} has not resolved"),
    }
}

fn an_answer_to(request_id: &str) -> TurnRequest {
    TurnRequest::resuming().with_permission_answer(PermissionAnswer::Selected {
        request_id: request_id.to_string(),
        option_id: "allow-once".to_string(),
    })
}

#[tokio::test]
async fn a_permission_answer_reaches_the_daemon_while_a_deferred_turn_holds_the_conversation() {
    // Given a remote conversation whose deferred turn holds it, waiting on the user's answer
    let daemon = Arc::new(RefusingDaemon::default());
    let started = Arc::new(Notify::new());
    let (release, released) = oneshot::channel();
    let session = open_remote_conversation(
        "sub-withdraw-held",
        &daemon,
        TurnSession {
            started: Arc::clone(&started),
            released: Some(released),
        },
    )
    .await;
    register_turn("resp-deferred", "sub-withdraw-held").await;
    let deferred = tokio::spawn(run_turn(DeferredTurn {
        response_id: "resp-deferred".to_string(),
        conversation_id: "sub-withdraw-held".to_string(),
        request: TurnRequest::resuming().deferring_permission_to_user("perm-1"),
        session: Arc::clone(&session),
        reported_agent: None,
    }));
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("the deferred turn started and holds the conversation");

    // When the main agent answers the request on a resume of the same conversation
    register_turn("resp-answer", "sub-withdraw-held").await;
    tokio::time::timeout(
        Duration::from_secs(2),
        run_turn(DeferredTurn {
            response_id: "resp-answer".to_string(),
            conversation_id: "sub-withdraw-held".to_string(),
            request: an_answer_to("perm-1"),
            session: Arc::clone(&session),
            reported_agent: None,
        }),
    )
    .await
    .expect("the answer returns promptly rather than waiting behind the deferred turn");

    // Then the answer was sent to the daemon, which refused it as delivered
    let sent = daemon.answers_sent();
    assert_eq!(sent.len(), 1, "the answer is sent to the daemon once");
    assert!(
        sent[0].permission_answer_json.contains("allow-once"),
        "{}",
        sent[0].permission_answer_json
    );
    let answer = answer_of("resp-answer").await;
    assert!(answer.contains("delivered"), "{answer}");

    // And the deferred turn is still the one holding the conversation, until the user answers it
    let _ = release.send(());
    deferred.await.expect("the deferred turn finishes");
}

#[tokio::test]
async fn a_permission_answer_to_a_conversation_nothing_holds_takes_its_turn_through_the_session() {
    // Given a remote conversation no turn holds
    let daemon = Arc::new(RefusingDaemon::default());
    let session = open_remote_conversation(
        "sub-withdraw-free",
        &daemon,
        TurnSession {
            started: Arc::new(Notify::new()),
            released: None,
        },
    )
    .await;

    // When the main agent answers a request on it
    register_turn("resp-free", "sub-withdraw-free").await;
    tokio::time::timeout(
        Duration::from_secs(2),
        run_turn(DeferredTurn {
            response_id: "resp-free".to_string(),
            conversation_id: "sub-withdraw-free".to_string(),
            request: an_answer_to("perm-1"),
            session,
            reported_agent: None,
        }),
    )
    .await
    .expect("the turn returns");

    // Then the session took the turn, as before, and the daemon was not sent the answer separately
    assert!(daemon.answers_sent().is_empty());
    let answer = answer_of("resp-free").await;
    assert!(answer.contains("tests pass"), "{answer}");
}
