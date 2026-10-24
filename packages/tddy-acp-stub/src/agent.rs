//! Stub agent implementing the ACP Agent trait.

use std::cell::Cell;

use agent_client_protocol::{self as acp};
use tokio::sync::{mpsc, oneshot};

use crate::recorder::Recorder;
use crate::scenario::Scenario;

/// The one model the stub advertises in a new session.
const STUB_MODEL_ID: &str = "stub-model";
const STUB_MODEL_NAME: &str = "Stub model";

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
    recorder: Recorder,
    /// Woken by `session/cancel`, for a turn held open with `hold_until_cancel`.
    cancelled: std::rc::Rc<tokio::sync::Notify>,
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
            recorder: Recorder::from_env(),
            cancelled: std::rc::Rc::new(tokio::sync::Notify::new()),
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
        args: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        let session_id = self.next_session_id.get();
        self.next_session_id.set(session_id + 1);
        self.recorder.record(serde_json::json!({
            "event": "new_session",
            "session_id": session_id.to_string(),
            "cwd": args.cwd,
            "mcp_servers": mcp_server_names(&args.mcp_servers),
            "meta": args.meta,
        }));
        Ok(
            acp::NewSessionResponse::new(acp::SessionId::new(session_id.to_string()))
                .models(self.advertised_models()),
        )
    }

    async fn load_session(
        &self,
        args: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        self.recorder.record(serde_json::json!({
            "event": "load_session",
            "session_id": args.session_id.0.to_string(),
            "mcp_servers": mcp_server_names(&args.mcp_servers),
            "meta": args.meta,
        }));
        Ok(acp::LoadSessionResponse::default().models(self.advertised_models()))
    }

    async fn set_session_model(
        &self,
        args: acp::SetSessionModelRequest,
    ) -> Result<acp::SetSessionModelResponse, acp::Error> {
        self.recorder.record(serde_json::json!({
            "event": "set_session_model",
            "session_id": args.session_id.0.to_string(),
            "model_id": args.model_id.0.to_string(),
        }));
        Ok(acp::SetSessionModelResponse::default())
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
            let response = self.request_permission(req).await?;
            let outcome = match response.outcome {
                acp::RequestPermissionOutcome::Selected(selected) => {
                    format!("selected:{}", selected.option_id.0)
                }
                acp::RequestPermissionOutcome::Cancelled => "cancelled".to_string(),
                _ => "unknown".to_string(),
            };
            self.recorder.record(serde_json::json!({
                "event": "permission_outcome",
                "title": pr.title,
                "outcome": outcome,
            }));
        }

        if template.hold_until_cancel {
            self.cancelled.notified().await;
            return Ok(acp::PromptResponse::new(acp::StopReason::Cancelled));
        }

        let stop_reason = match template.stop_reason.as_str() {
            "cancelled" => acp::StopReason::Cancelled,
            _ => acp::StopReason::EndTurn,
        };

        Ok(acp::PromptResponse::new(stop_reason))
    }

    async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        self.recorder.record(serde_json::json!({
            "event": "cancel",
            "session_id": args.session_id.0.to_string(),
        }));
        self.cancelled.notify_one();
        Ok(())
    }
}

impl StubAgent {
    /// The scenario's models, or the single default stub model — a real agent advertises the models
    /// it can run, so a client that enumerates models through a session has a non-empty list.
    fn advertised_models(&self) -> acp::SessionModelState {
        let ids: Vec<String> = {
            let scenario = self.scenario.borrow();
            if scenario.models.is_empty() {
                vec![STUB_MODEL_ID.to_string()]
            } else {
                scenario.models.clone()
            }
        };
        let infos = ids
            .iter()
            .map(|id| {
                let name = if id == STUB_MODEL_ID {
                    STUB_MODEL_NAME.to_string()
                } else {
                    id.clone()
                };
                acp::ModelInfo::new(acp::ModelId::new(id.clone()), name)
            })
            .collect();
        acp::SessionModelState::new(acp::ModelId::new(ids[0].clone()), infos)
    }
}

fn mcp_server_names(servers: &[acp::McpServer]) -> Vec<String> {
    servers
        .iter()
        .map(|server| match server {
            acp::McpServer::Stdio(stdio) => stdio.name.clone(),
            acp::McpServer::Http(http) => http.name.clone(),
            acp::McpServer::Sse(sse) => sse.name.clone(),
            _ => "unknown".to_string(),
        })
        .collect()
}
