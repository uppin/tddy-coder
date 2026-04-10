//! Stub agent implementing the ACP Agent trait.

use std::cell::Cell;

use agent_client_protocol::{self as acp};
use tokio::sync::{mpsc, oneshot};

use crate::scenario::Scenario;

/// Message from agent to background task (session notifications or permission requests).
pub enum AgentMessage {
    SessionNotification(acp::SessionNotification, oneshot::Sender<()>),
    RequestPermission(
        acp::RequestPermissionRequest,
        oneshot::Sender<acp::Result<acp::RequestPermissionResponse>>,
    ),
}

pub struct StubAgent {
    message_tx: mpsc::UnboundedSender<AgentMessage>,
    next_session_id: Cell<u64>,
    scenario: std::cell::RefCell<Scenario>,
}

impl StubAgent {
    /// Create with channel for agent messages (used by main).
    pub fn with_channel(
        message_tx: mpsc::UnboundedSender<AgentMessage>,
        scenario: Scenario,
    ) -> Self {
        Self {
            message_tx,
            next_session_id: Cell::new(0),
            scenario: std::cell::RefCell::new(scenario),
        }
    }

    async fn send_notification(
        &self,
        session_id: &acp::SessionId,
        update: acp::SessionUpdate,
    ) -> Result<(), acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.message_tx
            .send(AgentMessage::SessionNotification(
                acp::SessionNotification::new(session_id.clone(), update),
                tx,
            ))
            .map_err(|_| acp::Error::internal_error())?;
        rx.await.map_err(|_| acp::Error::internal_error())?;
        Ok(())
    }

    async fn request_permission(
        &self,
        req: acp::RequestPermissionRequest,
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.message_tx
            .send(AgentMessage::RequestPermission(req, tx))
            .map_err(|_| acp::Error::internal_error())?;
        rx.await
            .map_err(|_| acp::Error::internal_error())
            .and_then(std::convert::identity)
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Agent for StubAgent {
    async fn initialize(
        &self,
        _args: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(acp::InitializeResponse::new(acp::ProtocolVersion::V1)
            .agent_capabilities(acp::AgentCapabilities::new().load_session(true))
            .agent_info(acp::Implementation::new("tddy-acp-stub", "0.1.0").title("TDDY ACP Stub")))
    }

    async fn authenticate(
        &self,
        _args: acp::AuthenticateRequest,
    ) -> Result<acp::AuthenticateResponse, acp::Error> {
        Ok(acp::AuthenticateResponse::default())
    }

    async fn new_session(
        &self,
        _args: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        let session_id = self.next_session_id.get();
        self.next_session_id.set(session_id + 1);
        Ok(acp::NewSessionResponse::new(acp::SessionId::new(
            session_id.to_string(),
        )))
    }

    async fn load_session(
        &self,
        args: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        let _ = args;
        Ok(acp::LoadSessionResponse::default())
    }

    async fn prompt(&self, args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        let template = self
            .scenario
            .borrow_mut()
            .responses
            .drain(..1)
            .next()
            .unwrap_or_default();

        if template.error {
            return Err(acp::Error::internal_error());
        }

        for chunk in &template.chunks {
            self.send_notification(
                &args.session_id,
                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(chunk.clone().into())),
            )
            .await?;
        }

        for tc in &template.tool_calls {
            let tool_call_id = acp::ToolCallId::new(format!("tool-{}", tc.name));
            let raw_value: serde_json::Value = tc.input.clone();
            self.send_notification(
                &args.session_id,
                acp::SessionUpdate::ToolCall(
                    acp::ToolCall::new(tool_call_id, tc.name.clone()).raw_input(raw_value),
                ),
            )
            .await?;
        }

        for pr in &template.permission_requests {
            let tool_call_id = acp::ToolCallId::new("perm-request");
            let locations: Vec<acp::ToolCallLocation> = pr
                .locations
                .iter()
                .map(|p| acp::ToolCallLocation::new(p.clone()))
                .collect();
            let update = acp::ToolCallUpdate::new(
                tool_call_id,
                acp::ToolCallUpdateFields::new()
                    .title(pr.title.clone())
                    .locations(locations),
            );
            let options = vec![
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow-once"),
                    "Allow once",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("reject-once"),
                    "Reject",
                    acp::PermissionOptionKind::RejectOnce,
                ),
            ];
            let req = acp::RequestPermissionRequest::new(args.session_id.clone(), update, options);
            let _ = self.request_permission(req).await?;
        }

        let stop_reason = match template.stop_reason.as_str() {
            "cancelled" => acp::StopReason::Cancelled,
            _ => acp::StopReason::EndTurn,
        };

        Ok(acp::PromptResponse::new(stop_reason))
    }

    async fn cancel(&self, _args: acp::CancelNotification) -> Result<(), acp::Error> {
        Ok(())
    }
}
