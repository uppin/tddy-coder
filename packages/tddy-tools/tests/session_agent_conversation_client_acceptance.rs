//! Acceptance tests: the in-jail client for a roster agent this process holds no def for.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Invoking an agent.
//!
//! A roster entry carries no endpoint and no credential, so an agent owned by another daemon — and
//! a local one attached after spawn — can only be run by asking the *facilitating* daemon to run
//! it: `OpenAgentConversation`, `PromptAgentConversation`, `CancelAgentConversation`. Until this
//! client existed, `subagent_new_session` answered every such agent with "this build does not yet
//! ask", which is what an operator saw when they attached an agent from another host.
//!
//! The peer is a fake `RpcClientTransport` rather than a daemon: what is wrong-able here is which
//! request this process sends and what it makes of the frames that come back. The daemon's own
//! half — resolving the roster entry and forwarding to the owning daemon — is covered where it is
//! implemented (`tddy-daemon/tests/session_agent_conversation_acceptance.rs`).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use prost::Message;
use tddy_discovery::subagent::{StopReason, SubagentSession};
use tddy_rpc::{RpcClientTransport, Status};
use tddy_service::proto::connection::{
    AgentConversationChunk, CancelAgentConversationRequest, OpenAgentConversationRequest,
    OpenAgentConversationResponse, PromptAgentConversationRequest,
};
use tddy_tools::session_agents::AgentConversationLink;
use tddy_tools::session_tool_client::SessionToolEnvelope;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The identity a split session's `tddy-tools` carries in every request to its daemon.
fn an_envelope() -> SessionToolEnvelope {
    SessionToolEnvelope {
        session_id: "01a04d08-2bbf-7850-ae60-0df89791608a".to_string(),
        session_token: "session-token-for-the-facilitating-daemon".to_string(),
        daemon_instance_id: "udoo".to_string(),
    }
}

/// One frame of an agent's answer, as the daemon splits a turn into them.
fn a_content_frame(chunk: &str) -> AgentConversationChunk {
    AgentConversationChunk {
        content_chunk: chunk.to_string(),
        stop_reason: String::new(),
        last: false,
    }
}

/// The final frame of a turn: the stop reason rides it, and it is what marks the answer complete.
fn a_final_frame(chunk: &str, stop_reason: &str) -> AgentConversationChunk {
    AgentConversationChunk {
        content_chunk: chunk.to_string(),
        stop_reason: stop_reason.to_string(),
        last: true,
    }
}

// ---------------------------------------------------------------------------
// The fake daemon
// ---------------------------------------------------------------------------

/// What one method of the fake daemon answers with.
enum Answer {
    Unary(Result<Vec<u8>, Status>),
    Frames(Vec<AgentConversationChunk>),
    Stream(Result<Vec<Result<Vec<u8>, Status>>, Status>),
}

/// One request this process sent, as the fake daemon received it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReceivedCall {
    service: String,
    method: String,
    payload: Vec<u8>,
}

/// A `ConnectionService` that answers from a script and records what it was asked.
struct AFakeDaemon {
    open: Mutex<Option<Answer>>,
    prompt: Mutex<Option<Answer>>,
    cancel: Mutex<Option<Answer>>,
    received: Mutex<Vec<ReceivedCall>>,
}

impl AFakeDaemon {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(None),
            prompt: Mutex::new(None),
            cancel: Mutex::new(None),
            received: Mutex::new(Vec::new()),
        })
    }

    fn answering_open_with(self: &Arc<Self>, conversation_id: &str) -> Arc<Self> {
        *self.open.lock().expect("script the open answer") =
            Some(Answer::Unary(Ok(OpenAgentConversationResponse {
                conversation_id: conversation_id.to_string(),
            }
            .encode_to_vec())));
        Arc::clone(self)
    }

    fn refusing_open_with(self: &Arc<Self>, status: Status) -> Arc<Self> {
        *self.open.lock().expect("script the open refusal") = Some(Answer::Unary(Err(status)));
        Arc::clone(self)
    }

    fn answering_prompt_with(self: &Arc<Self>, frames: Vec<AgentConversationChunk>) -> Arc<Self> {
        *self.prompt.lock().expect("script the prompt answer") = Some(Answer::Frames(frames));
        Arc::clone(self)
    }

    fn refusing_prompt_with(self: &Arc<Self>, status: Status) -> Arc<Self> {
        *self.prompt.lock().expect("script the prompt refusal") = Some(Answer::Stream(Err(status)));
        Arc::clone(self)
    }

    fn answering_cancel(self: &Arc<Self>) -> Arc<Self> {
        *self.cancel.lock().expect("script the cancel answer") =
            Some(Answer::Unary(Ok(Vec::new())));
        Arc::clone(self)
    }

    fn as_transport(self: &Arc<Self>) -> Arc<dyn RpcClientTransport> {
        Arc::clone(self) as Arc<dyn RpcClientTransport>
    }

    /// The single request sent to `method`, decoded by the caller. Fails loudly when the method was
    /// never called or called more than once — either would make the assertion meaningless.
    fn the_one_request_to(&self, method: &str) -> Vec<u8> {
        let received = self.received.lock().expect("read the received calls");
        let matching: Vec<&ReceivedCall> = received
            .iter()
            .filter(|call| call.method == method)
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "expected exactly one {method} call, got {}: {received:?}",
            matching.len()
        );
        matching[0].payload.clone()
    }

    fn methods_called(&self) -> Vec<String> {
        self.received
            .lock()
            .expect("read the received calls")
            .iter()
            .map(|call| format!("{}/{}", call.service, call.method))
            .collect()
    }

    fn record(&self, service: &str, method: &str, payload: &[u8]) {
        self.received
            .lock()
            .expect("record the received call")
            .push(ReceivedCall {
                service: service.to_string(),
                method: method.to_string(),
                payload: payload.to_vec(),
            });
    }

    fn scripted(&self, method: &str) -> Option<Answer> {
        match method {
            "OpenAgentConversation" => self.open.lock().expect("read the open script").take(),
            "PromptAgentConversation" => self.prompt.lock().expect("read the prompt script").take(),
            "CancelAgentConversation" => self.cancel.lock().expect("read the cancel script").take(),
            _ => None,
        }
    }
}

#[async_trait]
impl RpcClientTransport for AFakeDaemon {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.record(service, method, &request_bytes);
        match self.scripted(method) {
            Some(Answer::Unary(answer)) => answer,
            _ => Err(Status::unimplemented(format!(
                "the fake daemon has no unary answer scripted for {service}/{method}"
            ))),
        }
    }

    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        self.record(service, method, &request_bytes);
        let frames = match self.scripted(method) {
            Some(Answer::Frames(frames)) => frames
                .into_iter()
                .map(|frame| Ok(frame.encode_to_vec()))
                .collect(),
            Some(Answer::Stream(Ok(frames))) => frames,
            Some(Answer::Stream(Err(status))) => return Err(status),
            _ => {
                return Err(Status::unimplemented(format!(
                    "the fake daemon has no stream scripted for {service}/{method}"
                )))
            }
        };
        let (tx, rx) = mpsc::channel(frames.len().max(1));
        for frame in frames {
            tx.send(frame).await.expect("hand a frame to the client");
        }
        Ok(rx)
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

/// A link to the fake daemon, carrying the identity a real one authenticates.
fn a_link_to(daemon: &Arc<AFakeDaemon>) -> Arc<AgentConversationLink> {
    Arc::new(AgentConversationLink::new(
        daemon.as_transport(),
        an_envelope(),
    ))
}

fn assert_refused_naming<T: std::fmt::Debug>(result: Result<T, String>, fragment: &str) {
    let message = result.expect_err("the call should have been refused");
    assert!(
        message.contains(fragment),
        "refusal should name {fragment:?}, got: {message}"
    );
}

fn joined_text(outcome: &tddy_discovery::subagent::PromptOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

// ---------------------------------------------------------------------------
// Opening a conversation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn opens_a_conversation_on_the_facilitating_daemon_and_returns_its_id() {
    // Given a daemon that accepts the open under a conversation id of its own
    let daemon = AFakeDaemon::new().answering_open_with("conv-7");

    // When the agent is opened
    let opened = a_link_to(&daemon).open("FastContext@mac", "").await;

    // Then the daemon's id is what the caller is handed
    assert_eq!(
        opened.expect("the open should have been accepted"),
        "conv-7"
    );
}

#[tokio::test]
async fn an_open_carries_the_session_identity_the_daemon_authenticates() {
    // Given a daemon that accepts the open
    let daemon = AFakeDaemon::new().answering_open_with("conv-7");

    // When an agent is opened under a caller-chosen conversation id
    let _ = a_link_to(&daemon)
        .open("FastContext@mac", "conv-mine")
        .await;

    // Then every field the daemon resolves the call against is on the wire
    let request = OpenAgentConversationRequest::decode(
        daemon
            .the_one_request_to("OpenAgentConversation")
            .as_slice(),
    )
    .expect("the request should decode as OpenAgentConversationRequest");
    assert_eq!(
        (
            request.session_id.as_str(),
            request.session_token.as_str(),
            request.daemon_instance_id.as_str(),
            request.agent_id.as_str(),
            request.conversation_id.as_str(),
        ),
        (
            "01a04d08-2bbf-7850-ae60-0df89791608a",
            "session-token-for-the-facilitating-daemon",
            "udoo",
            "FastContext@mac",
            "conv-mine",
        )
    );
}

#[tokio::test]
async fn an_open_the_daemon_refuses_is_reported_naming_the_refusal() {
    // Given a daemon that refuses the open because the agent is not attached
    let daemon = AFakeDaemon::new().refusing_open_with(Status::invalid_argument(
        "agent 'FastContext@mac' is not attached to session '01a04d08'",
    ));

    // When the agent is opened
    let opened = a_link_to(&daemon).open("FastContext@mac", "").await;

    // Then the daemon's own words reach the caller rather than a generic failure
    assert_refused_naming(opened, "is not attached to session");
}

// ---------------------------------------------------------------------------
// Prompting a conversation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_prompt_reassembles_every_frame_into_one_answer() {
    // Given a daemon that splits one turn across three frames
    let daemon = AFakeDaemon::new().answering_prompt_with(vec![
        a_content_frame("the roster "),
        a_content_frame("is served by "),
        a_final_frame("the owning daemon.", "EndTurn"),
    ]);

    // When the conversation is prompted
    let outcome = a_link_to(&daemon)
        .prompt("conv-7", "who serves the roster?")
        .await
        .expect("the prompt should have been answered");

    // Then the answer is the frames in order, not the last one alone
    assert_eq!(
        joined_text(&outcome),
        "the roster is served by the owning daemon."
    );
}

#[tokio::test]
async fn a_prompt_reports_the_stop_reason_from_the_final_frame() {
    // Given a daemon whose agent ran out of turns
    let daemon = AFakeDaemon::new()
        .answering_prompt_with(vec![a_final_frame("as far as I got", "MaxTurnRequests")]);

    // When the conversation is prompted
    let outcome = a_link_to(&daemon)
        .prompt("conv-7", "keep going")
        .await
        .expect("the prompt should have been answered");

    // Then the caller learns the turn was cut short rather than completed
    assert_eq!(outcome.stop_reason, StopReason::MaxTurnRequests);
}

#[tokio::test]
async fn a_prompt_carries_the_conversation_the_daemon_opened() {
    // Given a daemon that answers the turn
    let daemon = AFakeDaemon::new().answering_prompt_with(vec![a_final_frame("hi", "EndTurn")]);

    // When the conversation is prompted
    let _ = a_link_to(&daemon).prompt("conv-7", "say hi").await;

    // Then the request names the conversation and the prompt text
    let request = PromptAgentConversationRequest::decode(
        daemon
            .the_one_request_to("PromptAgentConversation")
            .as_slice(),
    )
    .expect("the request should decode as PromptAgentConversationRequest");
    assert_eq!(
        (request.conversation_id.as_str(), request.prompt.as_str()),
        ("conv-7", "say hi")
    );
}

#[tokio::test]
async fn an_answer_of_nothing_is_still_one_completed_turn() {
    // Given a daemon whose agent said nothing at all
    let daemon = AFakeDaemon::new().answering_prompt_with(vec![a_final_frame("", "EndTurn")]);

    // When the conversation is prompted
    let outcome = a_link_to(&daemon)
        .prompt("conv-7", "anything to add?")
        .await
        .expect("an empty answer is an answer");

    // Then it reads as an empty answer, not as a failure
    assert_eq!(
        (joined_text(&outcome), outcome.stop_reason),
        (String::new(), StopReason::EndTurn)
    );
}

#[tokio::test]
async fn a_stream_that_ends_without_a_final_frame_is_refused_as_truncated() {
    // Given a daemon whose stream ends mid-answer, with no frame marked last
    let daemon = AFakeDaemon::new().answering_prompt_with(vec![a_content_frame("half an ans")]);

    // When the conversation is prompted
    let outcome = a_link_to(&daemon).prompt("conv-7", "explain").await;

    // Then the partial answer is refused rather than returned as if it were complete
    assert_refused_naming(outcome, "truncated");
}

#[tokio::test]
async fn a_final_frame_naming_an_unknown_stop_reason_is_refused() {
    // Given a daemon that ended the turn with a spelling this build does not know
    let daemon =
        AFakeDaemon::new().answering_prompt_with(vec![a_final_frame("done", "SomethingElse")]);

    // When the conversation is prompted
    let outcome = a_link_to(&daemon).prompt("conv-7", "explain").await;

    // Then the disagreement is reported naming the spelling, not silently read as EndTurn
    assert_refused_naming(outcome, "SomethingElse");
}

#[tokio::test]
async fn a_prompt_the_daemon_refuses_is_reported_naming_the_refusal() {
    // Given a daemon that refuses the prompt because the agent was detached underneath it
    let daemon = AFakeDaemon::new().refusing_prompt_with(Status::failed_precondition(
        "agent 'FastContext@mac' was detached",
    ));

    // When the conversation is prompted
    let outcome = a_link_to(&daemon).prompt("conv-7", "still there?").await;

    // Then the daemon's own words reach the caller
    assert_refused_naming(outcome, "was detached");
}

// ---------------------------------------------------------------------------
// Cancelling a conversation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_cancel_closes_the_conversation_on_the_daemon() {
    // Given a daemon holding an open conversation
    let daemon = AFakeDaemon::new().answering_cancel();

    // When the conversation is cancelled
    a_link_to(&daemon)
        .cancel("conv-7")
        .await
        .expect("the cancel should have been accepted");

    // Then the daemon was told which conversation to close
    let request = CancelAgentConversationRequest::decode(
        daemon
            .the_one_request_to("CancelAgentConversation")
            .as_slice(),
    )
    .expect("the request should decode as CancelAgentConversationRequest");
    assert_eq!(request.conversation_id, "conv-7");
}

// ---------------------------------------------------------------------------
// The conversation as a subagent session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_remote_conversation_prompts_over_the_link_it_was_opened_on() {
    // Given a conversation the daemon opened for an agent it owns
    let daemon = AFakeDaemon::new()
        .answering_open_with("conv-7")
        .answering_prompt_with(vec![a_final_frame("mac answered", "EndTurn")]);
    let link = a_link_to(&daemon);
    let conversation_id = link
        .open("FastContext@mac", "")
        .await
        .expect("the open should have been accepted");
    let mut session = link.session(conversation_id, "qwen2.5-coder:7b");

    // When the main agent prompts it through the ordinary subagent surface
    let outcome = session
        .prompt("hi")
        .await
        .expect("the prompt should have been answered");

    // Then the turn ran on the daemon, over the same link
    assert_eq!(joined_text(&outcome), "mac answered");
    assert_eq!(
        daemon.methods_called(),
        vec![
            "connection.ConnectionService/OpenAgentConversation",
            "connection.ConnectionService/PromptAgentConversation",
        ]
    );
}

#[tokio::test]
async fn a_remote_conversation_reports_the_model_the_roster_named() {
    // Given a conversation with an agent the roster says runs on a named model
    let daemon = AFakeDaemon::new().answering_open_with("conv-7");
    let link = a_link_to(&daemon);
    let session = link.session("conv-7".to_string(), "qwen2.5-coder:7b");

    // When the accounting asks what it talks to
    // Then it is the roster's model, so a remote conversation is not recorded as modelless
    assert_eq!(session.model(), "qwen2.5-coder:7b");
}
