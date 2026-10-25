//! `subagent_end` closes the conversation on the daemon running it — never a turn cancel, which a
//! daemon answers by keeping a coding agent's conversation open. It still sends
//! `keep_conversation: false` now that `subagent_cancel` sends `true`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use prost::Message;
use serde_json::json;
use serial_test::serial;
use tddy_discovery::subagent_runtime::{subagent_sessions, SubagentConversation};
use tddy_rpc::{RpcClientTransport, Status};
use tddy_service::proto::session_agents_svc::{
    CancelAgentConversationRequest, CancelAgentConversationResponse, SessionAgentEntry,
    SessionAgentRoster,
};

use crate::session_agents::{session_agent_roster, AgentConversationLink};
use crate::session_tool_client::SessionToolEnvelope;

/// The daemon's cancels, as it received them, decoded.
#[derive(Default)]
struct RecordingDaemon {
    cancels: Mutex<Vec<CancelAgentConversationRequest>>,
}

#[async_trait]
impl RpcClientTransport for RecordingDaemon {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        if method != "CancelAgentConversation" {
            return Err(Status::unimplemented(format!(
                "the fake daemon has no unary answer for {service}/{method}"
            )));
        }
        let request = CancelAgentConversationRequest::decode(request_bytes.as_slice())
            .expect("the cancel decodes as CancelAgentConversationRequest");
        self.cancels
            .lock()
            .expect("record the cancel")
            .push(request);
        // The daemon kept the conversation: it is a coding agent's, whose turn alone was cancelled.
        Ok(CancelAgentConversationResponse {
            conversation_kept: true,
        }
        .encode_to_vec())
    }

    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented(format!(
            "the fake daemon has no stream for {service}/{method}"
        )))
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
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
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

/// The roster agent the conversation is with.
const AGENT_ID: &str = "explorer@mac";

/// Open `conversation_id` with [`AGENT_ID`] on the process roster, as `subagent_new_session` does.
fn open_on_the_roster(conversation_id: &str, revision: u64) {
    let roster = session_agent_roster();
    roster.apply_snapshot(SessionAgentRoster {
        session_id: "1780828020298-keep".to_string(),
        rev: revision,
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

/// A daemon-run conversation `tool_id`, open in the tool's table and on the roster as `conversation_id`.
fn a_daemon_run_conversation(
    daemon: &Arc<RecordingDaemon>,
    tool_id: &str,
    conversation_id: &str,
    revision: u64,
) {
    open_on_the_roster(conversation_id, revision);
    let link = Arc::new(AgentConversationLink::new(
        Arc::clone(daemon) as Arc<dyn RpcClientTransport>,
        an_envelope(),
    ));
    let remote = link.handle(conversation_id.to_string());
    let session = link.session(conversation_id.to_string(), "gpt-5.2");
    let conversation = SubagentConversation::opened(
        "explorer".to_string(),
        None,
        Box::new(session),
        Some(remote),
    );
    subagent_sessions()
        .try_lock()
        .expect("no other test holds the conversation table")
        .open
        .insert(tool_id.to_string(), conversation);
}

#[tokio::test]
#[serial]
async fn subagent_end_closes_the_conversation_on_the_daemon_rather_than_cancelling_its_turn() {
    // Given a conversation with a daemon-run loop, whose daemon would keep it on a turn cancel
    let daemon = Arc::new(RecordingDaemon::default());
    a_daemon_run_conversation(&daemon, "sub-end-1", "conv-end-1", 9_201);

    // When the main agent ends it
    let reply: serde_json::Value =
        serde_json::from_str(&super::subagent_end_tool(json!({ "sessionId": "sub-end-1" })).await)
            .expect("subagent_end returns JSON");

    // Then the daemon was asked to close it, and it is closed here
    assert_eq!(reply["ended"], json!(true), "{reply}");
    let cancels = daemon.cancels.lock().expect("read the cancels").clone();
    assert_eq!(cancels.len(), 1, "{cancels:?}");
    assert!(
        !cancels[0].keep_conversation,
        "subagent_end closes the conversation: {cancels:?}"
    );
    assert!(
        !subagent_sessions()
            .lock()
            .await
            .open
            .contains_key("sub-end-1"),
        "the conversation is closed here"
    );
}
