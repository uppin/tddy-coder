//! `AcpService` implementation: the protobuf mirror of ACP served over the same `tddy_rpc`
//! transports as `TddyRemote` (LiveKit session connection, stdio, gRPC).
//!
//! Architecture mirrors [`crate::service::TddyRemoteService`]: this is a *view-adapter* onto the
//! session's Presenter, not a second workflow engine. A single `Session` bidi stream carries the
//! whole ACP conversation — the client's `initialize`/`new_session`/`prompt` and the agent's
//! streamed `session/update`s and terminal `PromptResponse` — multiplexed by the envelope `id`
//! (see `acp.proto`). Because it drives the same Presenter (`view_factory` → `ViewConnection`), a
//! browser can speak ACP directly over LiveKit and see exactly what `TddyRemote` would have shown.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;

use tddy_core::{ActivityEntry, ActivityKind, AppMode, ClarificationQuestion, PresenterEvent};

use crate::convert_acp::{
    clarification_request_permission, permission_response_to_intent, plan_entry, plan_update,
    presenter_event_to_session_update, prompt_response, prompt_text, prompt_to_intent,
    session_update_message, tool_call_completed, tool_call_progress, tool_call_started,
};

/// The clarification a select/multi-select mode is presenting, if the Presenter is blocked on one.
fn clarification_of(mode: &AppMode) -> Option<&ClarificationQuestion> {
    match mode {
        AppMode::Select { question, .. } | AppMode::MultiSelect { question, .. } => Some(question),
        _ => None,
    }
}
use crate::proto::acp::{
    acp_agent_message, acp_client_message, AcpAgentMessage, AcpError, AgentCapabilities,
    AuthenticateResponse, Implementation, InitializeResponse, LoadSessionResponse,
    NewSessionResponse, PlanEntry, PlanEntryStatus, ProtocolVersion, SessionId, SessionModelState,
    SessionUpdate, StopReason,
};

/// Per-stream outbound state: turns the Presenter broadcast into ACP `AcpAgentMessage`s, owning the
/// **tool-call id lifecycle** and the **synthesized `Plan`** — both need per-stream memory the pure
/// `convert_acp` mappers lack. Bounded by what `PresenterEvent` carries (the view-adapter's only
/// input), so ACP variants with no internal source stay unmapped (see `convert_acp` / `acp.proto`).
struct OutboundState {
    session_id: String,
    /// Monotonic tool-call ids so sequential tool calls stay distinct handles.
    next_tool_id: u64,
    /// The tool call currently "open" (started, not yet completed), if any.
    open_tool_id: Option<u64>,
    /// One plan entry per `TaskStarted`; prior entries flip to `Completed` as new tasks arrive.
    plan: Vec<PlanEntry>,
    /// Agent-allocated ids for the permission requests we initiate (client echoes the id on reply).
    next_permission_id: u64,
}

impl OutboundState {
    fn new(session_id: String) -> Self {
        Self {
            session_id,
            next_tool_id: 1,
            open_tool_id: None,
            plan: Vec::new(),
            next_permission_id: 1,
        }
    }

    fn wrap(&self, update: SessionUpdate) -> AcpAgentMessage {
        session_update_message(&self.session_id, update)
    }

    /// Complete the currently-open tool call, if any (called on a new tool call and at turn end).
    fn close_open_tool(&mut self) -> Vec<AcpAgentMessage> {
        match self.open_tool_id.take() {
            Some(id) => vec![self.wrap(tool_call_completed(id))],
            None => Vec::new(),
        }
    }

    /// One presenter event → zero or more outbound messages. `pending` correlates a terminal
    /// `PromptResponse` with the in-flight prompt id (shared with the inbound task).
    fn on_event(&mut self, event: PresenterEvent, pending: &AtomicU64) -> Vec<AcpAgentMessage> {
        match event {
            PresenterEvent::WorkflowComplete(result) => {
                let mut out = self.close_open_tool();
                let id = pending.swap(0, Ordering::SeqCst);
                out.push(match result {
                    Ok(_) => prompt_response(id, StopReason::EndTurn),
                    Err(e) => error_message(id, e),
                });
                out
            }
            PresenterEvent::ModeChanged(details) => {
                if let Some(question) = clarification_of(&details.mode) {
                    // A blocked clarification → an agent-initiated request_permission; the client's
                    // reply decodes back to an AnswerSelect intent.
                    let id = self.next_permission_id;
                    self.next_permission_id += 1;
                    vec![clarification_request_permission(
                        id,
                        &self.session_id,
                        question,
                    )]
                } else if details.awaiting_open_answer {
                    // Free-prompting turn boundary: no WorkflowComplete fires, so end the in-flight
                    // prompt's turn here so a turn-based ACP client can proceed.
                    let mut out = self.close_open_tool();
                    let id = pending.swap(0, Ordering::SeqCst);
                    if id != 0 {
                        out.push(prompt_response(id, StopReason::EndTurn));
                    }
                    out
                } else {
                    Vec::new()
                }
            }
            PresenterEvent::ActivityLogged(entry) => self.on_activity(entry),
            other => presenter_event_to_session_update(&other)
                .map(|u| vec![self.wrap(u)])
                .unwrap_or_default(),
        }
    }

    fn on_activity(&mut self, entry: ActivityEntry) -> Vec<AcpAgentMessage> {
        match entry.kind {
            // A real tool invocation: complete the previous open call, open a new one (stable id).
            ActivityKind::ToolUse => {
                let mut out = self.close_open_tool();
                let id = self.next_tool_id;
                self.next_tool_id += 1;
                self.open_tool_id = Some(id);
                out.push(self.wrap(tool_call_started(id, &entry.text)));
                out
            }
            // A workflow task step: append a plan entry; prior entries flip to Completed.
            ActivityKind::TaskStarted => {
                for e in &mut self.plan {
                    e.status = PlanEntryStatus::Completed as i32;
                }
                self.plan
                    .push(plan_entry(entry.text, PlanEntryStatus::InProgress));
                let update = plan_update(&self.plan);
                vec![self.wrap(update)]
            }
            // Progress on the open tool call, if there is one.
            ActivityKind::TaskProgress => match self.open_tool_id {
                Some(id) => vec![self.wrap(tool_call_progress(id))],
                None => Vec::new(),
            },
            // Informational / user-echo kinds → stateless text chunks via the pure mapper.
            ActivityKind::UserPrompt
            | ActivityKind::Info
            | ActivityKind::AgentOutput
            | ActivityKind::StateChange => {
                presenter_event_to_session_update(&PresenterEvent::ActivityLogged(entry))
                    .map(|u| vec![self.wrap(u)])
                    .unwrap_or_default()
            }
        }
    }
}

/// Builds a fresh [`tddy_core::ViewConnection`] per opened stream — same factory
/// `TddyRemoteService` uses, so both adapters observe one Presenter.
type ViewFactory = std::sync::Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>;

/// `AcpService` view-adapter. One instance is mounted per session process and serves every
/// `Session` stream from the same Presenter.
pub struct TddyAcpService {
    view_factory: ViewFactory,
}

impl TddyAcpService {
    pub fn with_view_factory(view_factory: ViewFactory) -> Self {
        Self { view_factory }
    }
}

/// Outbound `AcpAgentMessage` builders for the protocol handshake replies (each echoes the
/// initiating `AcpClientMessage.id`).
fn initialize_response(id: u64) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::Initialize(InitializeResponse {
            protocol_version: ProtocolVersion::V1 as i32,
            agent_capabilities: Some(AgentCapabilities { load_session: true }),
            agent_info: Some(Implementation {
                name: "tddy-coder".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("TDDY Coder".to_string()),
            }),
        })),
    }
}

fn authenticate_response(id: u64) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::Authenticate(
            AuthenticateResponse {},
        )),
    }
}

fn new_session_response(
    id: u64,
    session_id: &str,
    models: Option<SessionModelState>,
) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::NewSession(NewSessionResponse {
            session_id: Some(SessionId {
                value: session_id.to_string(),
            }),
            models,
        })),
    }
}

fn load_session_response(id: u64) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::LoadSession(LoadSessionResponse {
            models: None,
        })),
    }
}

fn error_message(id: u64, message: String) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::Error(AcpError {
            code: -32603, // JSON-RPC internal error, mirrors acp::Error::internal_error()
            message,
            data: None,
        })),
    }
}

#[async_trait::async_trait]
impl crate::proto::acp::AcpService for TddyAcpService {
    type SessionStream = ReceiverStream<Result<AcpAgentMessage, tddy_rpc::Status>>;

    async fn session(
        &self,
        request: tddy_rpc::Request<tddy_rpc::Streaming<crate::proto::acp::AcpClientMessage>>,
    ) -> Result<tddy_rpc::Response<Self::SessionStream>, tddy_rpc::Status> {
        // Open the Presenter view for this stream (snapshot + live events + intent sender).
        let conn = (self.view_factory)()
            .ok_or_else(|| tddy_rpc::Status::internal("presenter unavailable"))?;
        let mut event_rx = conn.event_rx;
        let intent_tx = conn.intent_tx;

        // Stable session handle for this stream; returned by new_session and stamped on every
        // outbound session/update notification.
        let session_id = uuid::Uuid::now_v7().to_string();

        let (out_tx, out_rx) =
            tokio::sync::mpsc::channel::<Result<AcpAgentMessage, tddy_rpc::Status>>(64);

        // Correlates the in-flight prompt's id with the terminal PromptResponse.
        let pending_prompt_id = Arc::new(AtomicU64::new(0));
        // First prompt starts the workflow (SubmitFeatureInput); later ones queue.
        let has_started = Arc::new(AtomicBool::new(false));

        // Inbound: client AcpClientMessage -> handshake replies + Presenter intents.
        {
            let mut inbound = request.into_inner();
            let out_tx = out_tx.clone();
            let intent_tx = intent_tx.clone();
            let session_id = session_id.clone();
            let pending_prompt_id = pending_prompt_id.clone();
            let has_started = has_started.clone();
            tokio::spawn(async move {
                use acp_client_message::Msg;
                while let Some(Ok(msg)) = inbound.next().await {
                    let id = msg.id;
                    match msg.msg {
                        Some(Msg::Initialize(_)) => {
                            let _ = out_tx.send(Ok(initialize_response(id))).await;
                        }
                        Some(Msg::Authenticate(_)) => {
                            let _ = out_tx.send(Ok(authenticate_response(id))).await;
                        }
                        Some(Msg::NewSession(_)) => {
                            let _ = out_tx
                                .send(Ok(new_session_response(id, &session_id, None)))
                                .await;
                        }
                        Some(Msg::LoadSession(_)) => {
                            let _ = out_tx.send(Ok(load_session_response(id))).await;
                        }
                        Some(Msg::Prompt(p)) => {
                            pending_prompt_id.store(id, Ordering::SeqCst);
                            let text = prompt_text(&p.prompt);
                            let started = has_started.swap(true, Ordering::SeqCst);
                            let _ = intent_tx.send(prompt_to_intent(text, started));
                        }
                        Some(Msg::Cancel(_)) => {
                            let _ = intent_tx.send(tddy_core::UserIntent::Quit);
                        }
                        Some(Msg::RequestPermission(r)) => {
                            if let Some(intent) = permission_response_to_intent(&r) {
                                let _ = intent_tx.send(intent);
                            }
                        }
                        None => {}
                    }
                }
            });
        }

        // Outbound: Presenter events -> AcpAgentMessages, via the per-stream OutboundState (tool-call
        // lifecycle + synthesized Plan + permission/turn-boundary handling).
        {
            let out_tx = out_tx.clone();
            let session_id = session_id.clone();
            let pending_prompt_id = pending_prompt_id.clone();
            tokio::spawn(async move {
                let mut state = OutboundState::new(session_id);
                loop {
                    match event_rx.recv().await {
                        Ok(event) => {
                            for msg in state.on_event(event, &pending_prompt_id) {
                                if out_tx.send(Ok(msg)).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        Ok(tddy_rpc::Response::new(ReceiverStream::new(out_rx)))
    }
}

#[cfg(test)]
mod outbound_state_tests {
    use super::*;
    use crate::proto::acp::{session_update, ToolCallStatus};
    use tddy_core::ActivityEntry;

    fn tool_use(text: &str) -> PresenterEvent {
        PresenterEvent::ActivityLogged(ActivityEntry {
            text: text.into(),
            kind: ActivityKind::ToolUse,
        })
    }
    fn task_started(text: &str) -> PresenterEvent {
        PresenterEvent::ActivityLogged(ActivityEntry {
            text: text.into(),
            kind: ActivityKind::TaskStarted,
        })
    }
    /// The `SessionUpdate::Update` inside an `AcpAgentMessage` (panics otherwise).
    fn update_of(msg: &AcpAgentMessage) -> session_update::Update {
        match &msg.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                n.update.clone().unwrap().update.unwrap()
            }
            other => panic!("expected a SessionUpdate, got {other:?}"),
        }
    }

    #[test]
    fn tool_calls_get_stable_ids_and_the_previous_one_completes_when_the_next_starts() {
        let mut s = OutboundState::new("s".into());
        let pending = AtomicU64::new(0);

        // First tool call → one ToolCall(tool-1, InProgress).
        let first = s.on_event(tool_use("Read"), &pending);
        assert_eq!(first.len(), 1);
        match update_of(&first[0]) {
            session_update::Update::ToolCall(tc) => {
                assert_eq!(tc.tool_call_id.unwrap().value, "tool-1");
                assert_eq!(tc.title, "Read");
                assert_eq!(tc.status, ToolCallStatus::InProgress as i32);
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }

        // Second tool call → completes tool-1, then opens tool-2.
        let second = s.on_event(tool_use("Bash"), &pending);
        assert_eq!(second.len(), 2, "complete previous + start new");
        match update_of(&second[0]) {
            session_update::Update::ToolCallUpdate(u) => {
                assert_eq!(u.tool_call_id.unwrap().value, "tool-1");
                assert_eq!(
                    u.fields.unwrap().status,
                    Some(ToolCallStatus::Completed as i32)
                );
            }
            other => panic!("expected ToolCallUpdate(Completed) for tool-1, got {other:?}"),
        }
        match update_of(&second[1]) {
            session_update::Update::ToolCall(tc) => {
                assert_eq!(tc.tool_call_id.unwrap().value, "tool-2");
                assert_eq!(tc.title, "Bash");
            }
            other => panic!("expected ToolCall(tool-2), got {other:?}"),
        }
    }

    #[test]
    fn task_started_events_synthesize_a_growing_plan_with_prior_entries_completed() {
        let mut s = OutboundState::new("s".into());
        let pending = AtomicU64::new(0);

        let a = s.on_event(task_started("Write the test"), &pending);
        match update_of(&a[0]) {
            session_update::Update::Plan(p) => {
                assert_eq!(p.entries.len(), 1);
                assert_eq!(p.entries[0].content, "Write the test");
                assert_eq!(p.entries[0].status, PlanEntryStatus::InProgress as i32);
            }
            other => panic!("expected Plan, got {other:?}"),
        }

        let b = s.on_event(task_started("Make it pass"), &pending);
        match update_of(&b[0]) {
            session_update::Update::Plan(p) => {
                assert_eq!(p.entries.len(), 2);
                assert_eq!(p.entries[0].status, PlanEntryStatus::Completed as i32);
                assert_eq!(p.entries[1].content, "Make it pass");
                assert_eq!(p.entries[1].status, PlanEntryStatus::InProgress as i32);
            }
            other => panic!("expected 2-entry Plan, got {other:?}"),
        }
    }

    #[test]
    fn an_open_tool_call_is_completed_at_workflow_complete_before_the_prompt_response() {
        let mut s = OutboundState::new("s".into());
        let pending = AtomicU64::new(9); // in-flight prompt id

        let _ = s.on_event(tool_use("Edit"), &pending);
        let payload = tddy_core::WorkflowCompletePayload {
            summary: "done".into(),
            session_dir: None,
        };
        let done = s.on_event(PresenterEvent::WorkflowComplete(Ok(payload)), &pending);
        assert_eq!(done.len(), 2, "close open tool, then PromptResponse");
        match update_of(&done[0]) {
            session_update::Update::ToolCallUpdate(u) => {
                assert_eq!(
                    u.fields.unwrap().status,
                    Some(ToolCallStatus::Completed as i32)
                );
            }
            other => panic!("expected ToolCallUpdate(Completed), got {other:?}"),
        }
        match &done[1].msg {
            Some(acp_agent_message::Msg::Prompt(resp)) => {
                assert_eq!(done[1].id, 9);
                assert_eq!(resp.stop_reason, StopReason::EndTurn as i32);
            }
            other => panic!("expected PromptResponse, got {other:?}"),
        }
    }
}
