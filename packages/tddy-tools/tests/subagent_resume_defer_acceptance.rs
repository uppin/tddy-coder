//! `subagent_resume` and `subagent_await` with `deferPermissionToUser`: the deferral is carried to
//! the daemon rather than refused, and a turn still waiting on the user says so.
//!
//! The daemon is a fake `RpcClientTransport` that holds each resume open, as a daemon does while a
//! deferred request waits for its answer. What is wrong-able here is what this process sends and
//! what it makes of the call that is still pending.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use serde_json::{json, Value};
use serial_test::serial;
use tddy_discovery::subagent_runtime::{subagent_sessions, SubagentConversation};
use tddy_rpc::{RpcClientTransport, Status};
use tddy_service::proto::session_agents_svc::{
    ResumeAgentConversationRequest, SessionAgentEntry, SessionAgentRoster,
};
use tddy_tools::server::{subagent_await_tool, subagent_resume_tool};
use tddy_tools::session_agents::{session_agent_roster, AgentConversationLink};
use tddy_tools::session_tool_client::SessionToolEnvelope;
use tokio::sync::mpsc;

/// The sending end of one resume's frame stream.
type FrameSender = mpsc::Sender<Result<Vec<u8>, Status>>;

/// The daemon's answer to a resume: held open, so the turn it starts stays in flight.
struct HeldResumes {
    /// Every resume's sending end, kept alive so its stream never ends.
    open: Mutex<Vec<FrameSender>>,
    /// The resume requests as the daemon received them, decoded.
    received: Mutex<Vec<ResumeAgentConversationRequest>>,
}

impl HeldResumes {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(Vec::new()),
            received: Mutex::new(Vec::new()),
        })
    }

    fn as_transport(self: &Arc<Self>) -> Arc<dyn RpcClientTransport> {
        Arc::clone(self) as Arc<dyn RpcClientTransport>
    }

    /// Wait until the daemon has received a resume, and return it.
    async fn the_resume_received(&self) -> ResumeAgentConversationRequest {
        for _ in 0..500 {
            if let Some(request) = self.received.lock().expect("read received").first() {
                return request.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the daemon was never sent a ResumeAgentConversation");
    }
}

#[async_trait]
impl RpcClientTransport for HeldResumes {
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
        self.received
            .lock()
            .expect("record the resume")
            .push(request);
        let (tx, rx) = mpsc::channel(1);
        self.open.lock().expect("hold the resume open").push(tx);
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

/// The identity a split session's `tddy-tools` carries in every request to its daemon.
fn an_envelope() -> SessionToolEnvelope {
    SessionToolEnvelope {
        session_id: "01a04d08-2bbf-7850-ae60-0df89791608a".to_string(),
        session_token: "session-token-for-the-facilitating-daemon".to_string(),
        daemon_instance_id: "udoo".to_string(),
    }
}

/// The roster agent the conversations below are with.
const AGENT_ID: &str = "explorer@mac";

/// Open `conversation_id` with [`AGENT_ID`] on the process roster, as `subagent_new_session` does.
/// The roster refuses a conversation until a revision is applied, so one is applied first; each
/// revision is higher than the last, since the roster only moves forward.
fn open_on_the_roster(conversation_id: &str) {
    static REVISION: AtomicU64 = AtomicU64::new(0);
    let roster = session_agent_roster();
    roster.apply_snapshot(SessionAgentRoster {
        session_id: "1780828020298-defer".to_string(),
        rev: REVISION.fetch_add(1, Ordering::SeqCst) + 1,
        agents: vec![SessionAgentEntry {
            agent_id: AGENT_ID.to_string(),
            name: "explorer".to_string(),
            daemon_instance_id: "mac".to_string(),
            ..Default::default()
        }],
    });
    roster
        .open_conversation_as(conversation_id, AGENT_ID)
        .expect("the conversation opens on the roster");
}

/// Open a conversation on the held daemon under the tool's `session_id`, as a roster agent the
/// main agent has already opened.
fn register_conversation(session_id: &str, daemon: &Arc<HeldResumes>) {
    open_on_the_roster(session_id);
    let link = Arc::new(AgentConversationLink::new(
        daemon.as_transport(),
        an_envelope(),
    ));
    let session = link.session("conv-7".to_string(), "gpt-5.2");
    let conversation =
        SubagentConversation::opened("explorer".to_string(), None, Box::new(session), None);
    let mut table = subagent_sessions()
        .try_lock()
        .expect("no other test holds the conversation table");
    table.open.insert(session_id.to_string(), conversation);
}

/// Parse a subagent tool's result, which is always a JSON object.
fn parsed(tool: &str, result: String) -> Value {
    serde_json::from_str(&result)
        .unwrap_or_else(|e| panic!("{tool} returned invalid JSON {result:?}: {e}"))
}

#[tokio::test]
#[serial]
async fn subagent_resume_hands_the_request_to_the_user_instead_of_refusing_it() {
    // Given an open conversation whose daemon holds its turn until the user answers
    let daemon = HeldResumes::new();
    register_conversation("sub-defer-hand-off", &daemon);

    // When the main agent resumes it, deferring the pending request to the user
    let reply = parsed(
        "subagent_resume",
        subagent_resume_tool(json!({
            "sessionId": "sub-defer-hand-off",
            "deferPermissionToUser": "perm-1",
            "graceMs": 100,
        }))
        .await,
    );

    // Then the call is not refused, and the turn is pending with the user
    assert!(reply.get("error").is_none(), "not refused: {reply}");
    assert_eq!(reply["pending"], json!(true), "{reply}");
    assert_eq!(reply["awaitingUser"], json!(true), "{reply}");
    assert!(reply["responseId"].is_string(), "{reply}");

    // And the daemon was sent the deferral, with no answer of the main agent's own
    let sent = daemon.the_resume_received().await;
    assert_eq!(sent.defer_permission_to_user.as_deref(), Some("perm-1"));
    assert!(sent.permission_answer_json.is_empty());
}

#[tokio::test]
#[serial]
async fn subagent_await_reports_a_turn_still_awaiting_the_user() {
    // Given a deferred turn the user has not answered
    let daemon = HeldResumes::new();
    register_conversation("sub-defer-await", &daemon);
    let pending = parsed(
        "subagent_resume",
        subagent_resume_tool(json!({
            "sessionId": "sub-defer-await",
            "deferPermissionToUser": "perm-1",
            "graceMs": 100,
        }))
        .await,
    );
    let response_id = pending["responseId"]
        .as_str()
        .expect("a response id")
        .to_string();

    // When the main agent collects it before the user has answered
    let collected = parsed(
        "subagent_await",
        subagent_await_tool(json!({ "responseId": response_id, "timeoutMs": 100 })).await,
    );

    // Then it is still pending, and marked as awaiting the user
    assert_eq!(collected["pending"], json!(true), "{collected}");
    assert_eq!(collected["awaitingUser"], json!(true), "{collected}");
}
