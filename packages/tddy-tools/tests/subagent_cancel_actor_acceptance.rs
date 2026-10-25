//! `subagent_cancel` names the actor that cancelled the conversation on the wire: the main agent, so the daemon
//! can record who closed a deferred request. The daemon running a remote conversation's loop receives it.
//!
//! The daemon is a fake `RpcClientTransport` that records each cancel it is sent. The setup mirrors
//! `subagent_resume_defer_acceptance.rs`, copied rather than shared so each file reads alone.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use prost::Message;
use serde_json::json;
use serial_test::serial;
use tddy_discovery::subagent_runtime::{subagent_sessions, SubagentConversation};
use tddy_rpc::{RpcClientTransport, Status};
use tddy_service::proto::session_agents_svc::{
    CancelAgentConversationRequest, SessionAgentEntry, SessionAgentRoster,
};
use tddy_tools::server::subagent_cancel_tool;
use tddy_tools::session_agents::{session_agent_roster, AgentConversationLink};
use tddy_tools::session_tool_client::SessionToolEnvelope;

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
        Ok(Vec::new())
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
        session_id: "1780828020298-cancel".to_string(),
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

#[tokio::test]
#[serial]
async fn subagent_cancel_names_the_main_agent_as_the_actor_the_daemon_receives() {
    // Given a conversation with a daemon-run loop, open on the roster
    let daemon = Arc::new(RecordingDaemon::default());
    open_on_the_roster("conv-cancel-7", 9_001);
    let link = Arc::new(AgentConversationLink::new(
        Arc::clone(&daemon) as Arc<dyn RpcClientTransport>,
        an_envelope(),
    ));
    let remote = link.handle("conv-cancel-7".to_string());
    let session = link.session("conv-cancel-7".to_string(), "gpt-5.2");
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
        .insert("sub-cancel-actor".to_string(), conversation);

    // When the main agent cancels it
    let reply: serde_json::Value = serde_json::from_str(
        &subagent_cancel_tool(json!({ "sessionId": "sub-cancel-actor" })).await,
    )
    .expect("subagent_cancel returns JSON");

    // Then the tool reports it cancelled, and the daemon was told by whom
    assert_eq!(reply["cancelled"], json!(true), "{reply}");
    let cancels = daemon.cancels.lock().expect("read the cancels").clone();
    assert_eq!(cancels.len(), 1, "{cancels:?}");
    assert_eq!(cancels[0].conversation_id, "conv-cancel-7");
    assert_eq!(cancels[0].cancelled_by, "main-agent");
}
