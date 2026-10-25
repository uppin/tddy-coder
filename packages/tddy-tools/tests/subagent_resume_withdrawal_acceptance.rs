//! `subagent_resume` with a permission answer while a deferred turn holds the conversation: the answer
//! is sent to the daemon at once and comes back as the daemon's refusal, rather than waiting on the
//! lock the deferred turn holds.
//!
//! The daemon is a fake `RpcClientTransport` that holds each deferred resume open, as a daemon does
//! while a request waits for the user, and refuses an answer at once, as the daemon does while the
//! request is held. The conversation is opened the way a remote one is, with its daemon handle.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

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

/// The sending end of one deferred resume's frame stream.
type FrameSender = mpsc::Sender<Result<Vec<u8>, Status>>;

/// The fake daemon: deferred resumes are held open, and an answer is refused at once.
struct HeldResumes {
    /// Every deferred resume's sending end, kept alive so its stream never ends.
    open: Mutex<Vec<FrameSender>>,
    /// Every resume the daemon was sent, decoded, in order.
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

    /// The resumes the daemon has received, in order.
    fn resumes(&self) -> Vec<ResumeAgentConversationRequest> {
        self.received.lock().expect("read received").clone()
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
        let answers = !request.permission_answer_json.is_empty();
        self.received
            .lock()
            .expect("record the resume")
            .push(request);
        if answers {
            return Err(Status::failed_precondition(
                "permission request 'perm-1' was answered and the answer was delivered to the coding agent; \
                 the turn that deferred it carries on and returns its outcome to the call that deferred it",
            ));
        }
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
        session_id: "1780828020298-withdraw".to_string(),
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

/// Open a remote conversation on the held daemon under the tool's `session_id`, as a roster agent
/// the main agent has already opened. Its daemon handle is kept, as an opened remote one keeps it.
fn register_conversation(session_id: &str, daemon: &Arc<HeldResumes>) {
    open_on_the_roster(session_id);
    let link = Arc::new(AgentConversationLink::new(
        daemon.as_transport(),
        an_envelope(),
    ));
    let session = link.session("conv-7".to_string(), "gpt-5.2");
    let conversation = SubagentConversation::opened(
        "explorer".to_string(),
        None,
        Box::new(session),
        Some(link.handle("conv-7".to_string())),
    );
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
async fn a_permission_answer_is_refused_at_once_while_a_deferred_turn_holds_the_conversation() {
    // Given a remote conversation whose deferred turn holds it, waiting on the user
    let daemon = HeldResumes::new();
    register_conversation("sub-withdraw-answer", &daemon);
    let deferred = parsed(
        "subagent_resume",
        subagent_resume_tool(json!({
            "sessionId": "sub-withdraw-answer",
            "deferPermissionToUser": "perm-1",
            "graceMs": 100,
        }))
        .await,
    );
    assert_eq!(deferred["awaitingUser"], json!(true), "{deferred}");

    // When the main agent answers the same request itself
    let answered = tokio::time::timeout(
        Duration::from_secs(5),
        subagent_resume_tool(json!({
            "sessionId": "sub-withdraw-answer",
            "permission": { "requestId": "perm-1", "optionId": "allow-once" },
            "graceMs": 3000,
        })),
    )
    .await
    .expect("the answer returns promptly rather than waiting behind the deferred turn");
    let reply = parsed("subagent_resume", answered);

    // Then the call is refused with the daemon's word that the answer was delivered
    assert_eq!(reply["is_error"], json!(true), "{reply}");
    let error = reply["error"].as_str().expect("an error message");
    assert!(error.contains("delivered"), "{error}");

    // And the daemon was sent the answer, once, alongside the deferral it already held
    let resumes = daemon.resumes();
    assert_eq!(resumes.len(), 2, "{resumes:?}");
    assert_eq!(
        resumes[0].defer_permission_to_user.as_deref(),
        Some("perm-1")
    );
    assert!(resumes[1].permission_answer_json.contains("allow-once"));
    assert_eq!(resumes[1].defer_permission_to_user, None);
}

#[tokio::test]
#[serial]
async fn the_deferred_turn_is_still_pending_after_its_answer_was_refused() {
    // Given a deferred turn, and an answer to it already refused
    let daemon = HeldResumes::new();
    register_conversation("sub-withdraw-pending", &daemon);
    let deferred = parsed(
        "subagent_resume",
        subagent_resume_tool(json!({
            "sessionId": "sub-withdraw-pending",
            "deferPermissionToUser": "perm-1",
            "graceMs": 100,
        }))
        .await,
    );
    let response_id = deferred["responseId"]
        .as_str()
        .expect("a response id")
        .to_string();
    let refused = parsed(
        "subagent_resume",
        subagent_resume_tool(json!({
            "sessionId": "sub-withdraw-pending",
            "permission": { "requestId": "perm-1", "optionId": "allow-once" },
            "graceMs": 3000,
        }))
        .await,
    );
    assert_eq!(refused["is_error"], json!(true), "{refused}");

    // When the main agent collects the deferred turn
    let collected = parsed(
        "subagent_await",
        subagent_await_tool(json!({ "responseId": response_id, "timeoutMs": 100 })).await,
    );

    // Then it is still the turn the user has to answer: the refusal did not end it
    assert_eq!(collected["pending"], json!(true), "{collected}");
    assert_eq!(collected["awaitingUser"], json!(true), "{collected}");
}
