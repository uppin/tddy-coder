//! `ModelAcpService` — the daemon-hosted, model-addressed `acp.AcpService`.
//!
//! The other `AcpService` in this workspace (`tddy_service::service_acp::TddyAcpService`) is mounted
//! **per session** and speaks for that session's workflow. This one is mounted **per daemon**,
//! alongside `ModelRegistryService`, and speaks for any model or assistant in this daemon's
//! registry: a browser opening the Models & Agents chat addresses `daemon-{instanceId}` and names
//! its target in the handshake.
//!
//! Where the target is named, and why there: `NewSessionRequest.model_target` (`acp.proto`). The
//! target is a property of the session — `initialize` is answered before any target is known, and
//! the same stream may open a session for a different model next time — so it rides `new_session`
//! rather than the stream envelope. It carries the session token too, because reading a provider's
//! stored credential must be authorized by the same token every other registry RPC presents.
//!
//! Threading: `ProviderAcpAgent` holds `Rc`/`RefCell` state (the ACP SDK's traits are `?Send`), so
//! each stream gets its own OS thread running a current-thread runtime. Nothing but plain channels
//! crosses that boundary.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC10).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use agent_client_protocol::{self as acp, Agent as _};
use futures_util::StreamExt;
use tddy_acp::provider_agent::{ProviderAcpAgent, ProviderAgentConfig};
use tddy_rpc::{Request, Response, Status, Streaming};
use tddy_service::convert_acp::{prompt_response, prompt_text, session_update_message};
use tddy_service::proto::acp::{
    acp_agent_message, acp_client_message, AcpAgentMessage, AcpClientMessage, AcpError,
    AcpService as AcpServiceTrait, AgentCapabilities, AuthenticateResponse, ContentChunk,
    Implementation, InitializeResponse, ModelInfo, ModelSessionTarget, NewSessionRequest,
    NewSessionResponse, ProtocolVersion, SessionId, SessionModelState, SessionUpdate, StopReason,
    ToolCall, ToolCallId, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
};
use tddy_task::TaskRegistry;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::error::ModelRegistryError;
use super::provider_http::ProviderHttp;
use super::store::ModelRegistryStore;
use super::tool_dispatcher::EngineToolDispatcher;
use super::workspace::{resolve_chat_workspace, ChatWorkspaceRoots};
use crate::task_service::SessionUserResolver;

/// Outbound frames for one stream. Unbounded because the agent's update sink is a plain `Fn` with
/// nowhere to await back-pressure, and dropping an update would silently truncate the transcript.
type OutboundSender = tokio::sync::mpsc::UnboundedSender<Result<AcpAgentMessage, Status>>;

/// The daemon's model-addressed ACP surface.
pub struct ModelAcpService {
    store: Arc<ModelRegistryStore>,
    tasks: TaskRegistry,
    user_resolver: SessionUserResolver,
    workspace_roots: ChatWorkspaceRoots,
    http_config: ProviderHttp,
}

impl ModelAcpService {
    pub fn new(
        store: Arc<ModelRegistryStore>,
        tasks: TaskRegistry,
        user_resolver: SessionUserResolver,
        workspace_roots: ChatWorkspaceRoots,
    ) -> Self {
        Self {
            store,
            tasks,
            user_resolver,
            workspace_roots,
            http_config: ProviderHttp::default(),
        }
    }

    /// Talk to providers under a different transport budget than [`ProviderHttp::default`] — the
    /// same knob [`super::ollama::OllamaProviderClient::with_http_config`] offers, so a chat and an
    /// enumeration against the same host are configured the same way.
    pub fn with_http_config(mut self, http_config: ProviderHttp) -> Self {
        self.http_config = http_config;
        self
    }
}

#[async_trait::async_trait]
impl AcpServiceTrait for ModelAcpService {
    type SessionStream = UnboundedReceiverStream<Result<AcpAgentMessage, Status>>;

    async fn session(
        &self,
        request: Request<Streaming<AcpClientMessage>>,
    ) -> Result<Response<Self::SessionStream>, Status> {
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
        let inbound = request.into_inner();
        let context = StreamContext {
            store: Arc::clone(&self.store),
            tasks: self.tasks.clone(),
            user_resolver: Arc::clone(&self.user_resolver),
            workspace_roots: Arc::clone(&self.workspace_roots),
            http_config: self.http_config,
        };

        std::thread::Builder::new()
            .name("tddy-model-acp".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(e) => {
                        let _ = out_tx.send(Err(Status::internal(format!(
                            "could not start the model chat runtime: {e}"
                        ))));
                        return;
                    }
                };
                let local = tokio::task::LocalSet::new();
                local.block_on(&runtime, serve_stream(inbound, out_tx, context));
            })
            .map_err(|e| Status::internal(format!("could not start the model chat thread: {e}")))?;

        Ok(Response::new(UnboundedReceiverStream::new(out_rx)))
    }
}

/// Everything one stream's conversation is served from. A struct rather than seven arguments
/// because every one of them is cloned per stream and passed on together.
struct StreamContext {
    store: Arc<ModelRegistryStore>,
    tasks: TaskRegistry,
    user_resolver: SessionUserResolver,
    workspace_roots: ChatWorkspaceRoots,
    http_config: ProviderHttp,
}

/// The whole ACP conversation for one stream, on the thread that owns the agent.
async fn serve_stream(
    mut inbound: Streaming<AcpClientMessage>,
    out: OutboundSender,
    context: StreamContext,
) {
    // The ACP session handle, set by `new_session`; every session update is stamped with it.
    let session_id = Rc::new(RefCell::new(String::new()));
    // What this stream's tool calls are attributed to in the daemon's task registry. Distinct from
    // the ACP handle, which is only unique within one agent, and known before the first frame.
    let chat_id = format!("model-chat-{}", uuid::Uuid::now_v7());
    let mut agent: Option<Rc<ProviderAcpAgent>> = None;
    // The last frame an error can be attributed to. A transport failure carries no id of its own,
    // and answering `0` would look to a client like a reply to a request it never sent.
    let mut last_id = 0u64;
    // The turn currently running, if any. Turns are serialized — the conversation is one history —
    // but a turn runs *off* this loop so a `cancel` frame is read while it is still going.
    let mut in_flight: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        let message = match inbound.next().await {
            Some(Ok(message)) => message,
            // The stream simply ended — the client hung up, which is not a failure. A turn still
            // running answers first: dropping `out` here would truncate its transcript.
            None => return await_in_flight(in_flight).await,
            // A transport failure. Without a frame the client sees the stream end cleanly and
            // reports the conversation as finished, so say what happened before going.
            Some(Err(status)) => {
                await_in_flight(in_flight).await;
                let _ = out.send(Ok(error_message(
                    last_id,
                    AcpErrorCode::Internal,
                    format!("the chat stream failed: {status}"),
                )));
                return;
            }
        };
        let id = message.id;
        last_id = id;
        let reply = match message.msg {
            Some(acp_client_message::Msg::Initialize(_)) => Ok(initialize_response(id)),
            Some(acp_client_message::Msg::Authenticate(_)) => Ok(AcpAgentMessage {
                id,
                msg: Some(acp_agent_message::Msg::Authenticate(
                    AuthenticateResponse {},
                )),
            }),
            Some(acp_client_message::Msg::NewSession(request)) => {
                match open_session(request, &out, Rc::clone(&session_id), &chat_id, &context).await
                {
                    Ok((opened, response)) => {
                        agent = Some(Rc::new(opened));
                        Ok(AcpAgentMessage {
                            id,
                            msg: Some(acp_agent_message::Msg::NewSession(response)),
                        })
                    }
                    Err(e) => Err(e),
                }
            }
            Some(acp_client_message::Msg::LoadSession(_)) => Err(AcpFailure::new(
                // Nothing is persisted here: a chat with a model is a conversation held for the
                // life of the stream, so "resume session 5" has no answer to give.
                AcpErrorCode::UnsupportedOperation,
                "this daemon's model chat keeps no resumable sessions; open a new one",
            )),
            Some(acp_client_message::Msg::Prompt(request)) => match agent.clone() {
                Some(agent) => {
                    // One history, so one turn at a time: a prompt sent before the previous turn
                    // answered waits for it rather than interleaving into the same conversation.
                    await_in_flight(in_flight.take()).await;
                    let sid = session_id.borrow().clone();
                    // The turn runs as its own task so a `cancel` frame arriving mid-turn is read
                    // from the stream instead of queueing behind the very turn it must interrupt.
                    in_flight = Some(tokio::task::spawn_local(run_prompt(
                        agent,
                        request,
                        id,
                        sid,
                        out.clone(),
                    )));
                    continue;
                }
                None => Err(AcpFailure::new(
                    AcpErrorCode::InvalidParams,
                    "prompt arrived before new_session named a model",
                )),
            },
            Some(acp_client_message::Msg::Cancel(_)) => match agent.as_ref() {
                Some(agent) => {
                    let sid = session_id.borrow().clone();
                    match agent
                        .cancel(acp::CancelNotification::new(acp::SessionId::new(sid)))
                        .await
                    {
                        // ACP `cancel` is a notification: the interrupted turn's own response is
                        // what reports `Cancelled`, so a successful cancel answers nothing here.
                        Ok(()) => continue,
                        Err(e) => Err(AcpFailure::new(
                            AcpErrorCode::Internal,
                            e.message.to_string(),
                        )),
                    }
                }
                None => Err(AcpFailure::new(
                    AcpErrorCode::InvalidParams,
                    "cancel arrived before new_session opened anything to cancel",
                )),
            },
            // Permission replies have no meaning here: this agent never asks for permission.
            Some(acp_client_message::Msg::RequestPermission(_)) | None => continue,
        };

        let frame =
            reply.unwrap_or_else(|failure| error_message(id, failure.code, failure.message));
        if out.send(Ok(frame)).is_err() {
            return;
        }
    }
}

/// Wait for a running turn to finish, if there is one. A turn that panicked has already lost its
/// reply; the stream carries on rather than taking the panic with it.
async fn await_in_flight(in_flight: Option<tokio::task::JoinHandle<()>>) {
    if let Some(task) = in_flight {
        if let Err(e) = task.await {
            log::error!(
                target: "tddy_daemon::model_registry",
                "a model chat turn ended abnormally: {e}"
            );
        }
    }
}

/// One turn, answered on the stream it came in on.
async fn run_prompt(
    agent: Rc<ProviderAcpAgent>,
    request: tddy_service::proto::acp::PromptRequest,
    id: u64,
    session_id: String,
    out: OutboundSender,
) {
    let frame = match prompt(&agent, request, id, &session_id).await {
        Ok(frame) => frame,
        Err(failure) => error_message(id, failure.code, failure.message),
    };
    let _ = out.send(Ok(frame));
}

/// Build the agent the stream will speak through, from the target the client named.
async fn open_session(
    request: NewSessionRequest,
    out: &OutboundSender,
    session_id: Rc<RefCell<String>>,
    chat_id: &str,
    context: &StreamContext,
) -> Result<(ProviderAcpAgent, NewSessionResponse), AcpFailure> {
    let target = request.model_target.ok_or_else(|| {
        AcpFailure::new(
            AcpErrorCode::InvalidParams,
            "new_session carried no model_target; this surface serves the registry, so it has to \
             be told which model to speak as",
        )
    })?;
    // The operator this chat is held as. The provider's credential is read on their behalf, so a
    // chat opened against a colleague's provider is refused by `credential_for` rather than run
    // against their endpoint with no key at all.
    let Some(caller) = (context.user_resolver)(&target.session_token) else {
        return Err(AcpFailure::new(
            AcpErrorCode::AuthRequired,
            "invalid or expired session token",
        ));
    };

    let resolved = resolve_target(&target, &context.store, &caller).await?;
    // A tool-less chat runs nothing, so it needs no directory to run it in — that is the model
    // chat the Models table opens, and it sends no cwd at all. An assistant *with* tools names one,
    // and it is resolved against this caller's own roots before a tool exists to use it.
    let workspace = match resolved.tools.is_empty() {
        true => PathBuf::new(),
        false => {
            let roots = (context.workspace_roots)(&target.session_token)?;
            resolve_chat_workspace(&request.cwd, &roots)?
        }
    };
    let dispatcher =
        EngineToolDispatcher::new(&resolved.tools, workspace, context.tasks.clone(), chat_id)?;

    let sink_session_id = Rc::clone(&session_id);
    let sink_out = out.clone();
    let agent = ProviderAcpAgent::new(
        ProviderAgentConfig {
            base_url: resolved.base_url,
            model: resolved.model.clone(),
            api_key: resolved.api_key,
            system_prompt: resolved.system_prompt,
            // The chat pays the same transport budget as the enumeration that listed the model:
            // a provider that accepts the connection and then says nothing fails the turn instead
            // of wedging the stream.
            connect_timeout: context.http_config.connect_timeout,
            request_timeout: context.http_config.request_timeout,
        },
        Rc::new(dispatcher),
        Box::new(move |update| {
            if let Some(update) = to_proto_update(update) {
                let _ = sink_out.send(Ok(session_update_message(
                    &sink_session_id.borrow(),
                    update,
                )));
            }
        }),
    );

    let opened = agent
        .new_session(acp::NewSessionRequest::new(std::path::PathBuf::from("/")))
        .await
        .map_err(|e| AcpFailure::new(AcpErrorCode::Internal, e.message.to_string()))?;
    *session_id.borrow_mut() = opened.session_id.0.to_string();

    Ok((
        agent,
        NewSessionResponse {
            session_id: Some(SessionId {
                value: session_id.borrow().clone(),
            }),
            // One agent speaks as exactly one model, so that model is both the current one and the
            // whole menu.
            models: Some(SessionModelState {
                available_models: vec![ModelInfo {
                    model_id: resolved.model.clone(),
                    name: resolved.model.clone(),
                }],
                current_model_id: resolved.model,
            }),
        },
    ))
}

/// One turn: the operator's text in, the agent's stop reason out. Session updates have already
/// streamed through the sink by the time this returns.
async fn prompt(
    agent: &ProviderAcpAgent,
    request: tddy_service::proto::acp::PromptRequest,
    id: u64,
    session_id: &str,
) -> Result<AcpAgentMessage, AcpFailure> {
    let text = prompt_text(&request.prompt);
    let response = agent
        .prompt(acp::PromptRequest::new(
            acp::SessionId::new(session_id.to_string()),
            vec![acp::ContentBlock::from(text)],
        ))
        .await
        .map_err(|e| AcpFailure::new(AcpErrorCode::Internal, e.message.to_string()))?;
    Ok(prompt_response(
        id,
        to_proto_stop_reason(response.stop_reason),
    ))
}

/// Everything `ProviderAcpAgent` needs, resolved from one registry.
struct ResolvedTarget {
    base_url: String,
    model: String,
    api_key: Option<String>,
    system_prompt: Option<String>,
    tools: Vec<String>,
}

/// Resolve the named assistant — or, failing that, the named provider+model — out of the registry,
/// on behalf of `caller`.
async fn resolve_target(
    target: &ModelSessionTarget,
    store: &ModelRegistryStore,
    caller: &str,
) -> Result<ResolvedTarget, AcpFailure> {
    if !target.assistant_id.is_empty() {
        let assistant = store.assistant(&target.assistant_id).await?;
        let provider = store.provider(&assistant.provider_id).await?;
        reject_a_kind_this_chat_cannot_speak(&provider)?;
        return Ok(ResolvedTarget {
            base_url: provider.base_url,
            model: assistant.model_id,
            api_key: store.credential_for(&assistant.provider_id, caller).await?,
            system_prompt: (!assistant.system_prompt.is_empty()).then_some(assistant.system_prompt),
            tools: assistant.tools,
        });
    }

    if target.model_id.is_empty() {
        return Err(AcpFailure::new(
            AcpErrorCode::InvalidParams,
            "model_target named neither an assistant nor a model",
        ));
    }
    let provider = store.provider(&target.provider_id).await?;
    reject_a_kind_this_chat_cannot_speak(&provider)?;
    Ok(ResolvedTarget {
        base_url: provider.base_url,
        model: target.model_id.clone(),
        api_key: store.credential_for(&target.provider_id, caller).await?,
        system_prompt: None,
        tools: Vec::new(),
    })
}

/// Refuse to open a chat against a provider whose API this agent does not speak.
///
/// `ProviderAcpAgent` talks OpenAI-compatible chat completions with a bearer token. Anthropic
/// serves neither — it would answer every turn with a 401 the operator has to decode. Said once,
/// up front, instead.
///
/// TODO: an Anthropic-native chat path (the `/v1/messages` API with `x-api-key`), at which point
/// this refusal goes away. Enumeration already speaks Anthropic
/// (`openai_compatible::CredentialStyle::AnthropicApiKey`).
fn reject_a_kind_this_chat_cannot_speak(
    provider: &tddy_service::proto::models::ProviderEntry,
) -> Result<(), ModelRegistryError> {
    if provider.kind == tddy_service::proto::models::ProviderKind::Anthropic as i32 {
        return Err(ModelRegistryError::UnsupportedOperation(format!(
            "provider {} is an Anthropic endpoint; this daemon's chat speaks the \
             OpenAI-compatible completions api, which Anthropic does not serve",
            provider.provider_id
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ACP SDK types -> the protobuf mirror
// ---------------------------------------------------------------------------

/// The proto form of a session update the provider agent emitted, or `None` for a variant this
/// agent never produces.
fn to_proto_update(update: acp::SessionUpdate) -> Option<SessionUpdate> {
    use tddy_service::proto::acp::session_update::Update;
    let update = match update {
        acp::SessionUpdate::AgentMessageChunk(chunk) => Update::AgentMessageChunk(ContentChunk {
            content: Some(to_proto_content(chunk.content)),
        }),
        acp::SessionUpdate::ToolCall(call) => Update::ToolCall(ToolCall {
            tool_call_id: Some(ToolCallId {
                value: call.tool_call_id.0.to_string(),
            }),
            title: call.title,
            kind: 0,
            status: to_proto_tool_status(call.status) as i32,
            locations: Vec::new(),
            raw_input: call.raw_input.map(|v| v.to_string()),
            raw_output: call.raw_output.map(|v| v.to_string()),
        }),
        acp::SessionUpdate::ToolCallUpdate(call) => Update::ToolCallUpdate(ToolCallUpdate {
            tool_call_id: Some(ToolCallId {
                value: call.tool_call_id.0.to_string(),
            }),
            fields: Some(ToolCallUpdateFields {
                title: call.fields.title,
                kind: None,
                status: call
                    .fields
                    .status
                    .map(|status| to_proto_tool_status(status) as i32),
                locations: Vec::new(),
                raw_input: call.fields.raw_input.map(|v| v.to_string()),
                raw_output: call.fields.raw_output.map(|v| v.to_string()),
            }),
        }),
        _ => return None,
    };
    Some(SessionUpdate {
        update: Some(update),
    })
}

fn to_proto_content(block: acp::ContentBlock) -> tddy_service::proto::acp::ContentBlock {
    use tddy_service::proto::acp::{content_block, TextContent};
    let text = match block {
        acp::ContentBlock::Text(text) => text.text,
        // Only text is produced by this agent; anything else is reported as its debug form rather
        // than dropped, so a future block type is visible instead of silently missing.
        other => format!("{other:?}"),
    };
    tddy_service::proto::acp::ContentBlock {
        block: Some(content_block::Block::Text(TextContent { text })),
    }
}

fn to_proto_tool_status(status: acp::ToolCallStatus) -> ToolCallStatus {
    match status {
        acp::ToolCallStatus::Pending => ToolCallStatus::Pending,
        acp::ToolCallStatus::InProgress => ToolCallStatus::InProgress,
        acp::ToolCallStatus::Completed => ToolCallStatus::Completed,
        acp::ToolCallStatus::Failed => ToolCallStatus::Failed,
        _ => ToolCallStatus::Unspecified,
    }
}

fn to_proto_stop_reason(reason: acp::StopReason) -> StopReason {
    match reason {
        acp::StopReason::EndTurn => StopReason::EndTurn,
        acp::StopReason::Cancelled => StopReason::Cancelled,
        acp::StopReason::MaxTokens => StopReason::MaxTokens,
        acp::StopReason::MaxTurnRequests => StopReason::MaxTurnRequests,
        acp::StopReason::Refusal => StopReason::Refusal,
        _ => StopReason::Unspecified,
    }
}

fn initialize_response(id: u64) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::Initialize(InitializeResponse {
            protocol_version: ProtocolVersion::V1 as i32,
            agent_capabilities: Some(AgentCapabilities {
                load_session: false,
            }),
            agent_info: Some(Implementation {
                name: "tddy-provider-agent".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("TDDY Provider Agent".to_string()),
            }),
        })),
    }
}

// ---------------------------------------------------------------------------
// The error taxonomy, as JSON-RPC codes
// ---------------------------------------------------------------------------

/// What this surface answers a refusal with.
///
/// Everything used to arrive as `-32603` (internal error), which told the chat pane only "something
/// broke" — "you may not touch that operator's provider", "there is no such model" and "this daemon
/// cannot speak that API" were indistinguishable. The first three are JSON-RPC's own codes; the
/// rest are server-defined (`-32000`..`-32099`), which is the block the spec reserves for exactly
/// this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpErrorCode {
    /// The request could not be acted on as sent (JSON-RPC `invalid params`).
    InvalidParams,
    /// This daemon failed (JSON-RPC `internal error`).
    Internal,
    /// The session token is missing, invalid or expired. `acp::ErrorCode::AuthRequired`.
    AuthRequired,
    /// The row exists and belongs to somebody else.
    PermissionDenied,
    /// No such provider, model or assistant on this daemon.
    NotFound,
    /// The operation has no meaning here (residency on a cloud provider; resuming a chat).
    UnsupportedOperation,
    /// The *provider* endpoint failed — not this daemon.
    ProviderUnavailable,
}

impl AcpErrorCode {
    /// The wire value. Fixed numbers rather than derived ones: a client matches on them.
    fn as_i64(self) -> i64 {
        match self {
            AcpErrorCode::InvalidParams => -32602,
            AcpErrorCode::Internal => -32603,
            AcpErrorCode::AuthRequired => -32000,
            AcpErrorCode::PermissionDenied => -32001,
            AcpErrorCode::NotFound => -32002,
            AcpErrorCode::UnsupportedOperation => -32003,
            AcpErrorCode::ProviderUnavailable => -32004,
        }
    }
}

/// A refusal on its way to an `AcpError` frame: what went wrong, and which kind of wrong it is.
pub struct AcpFailure {
    code: AcpErrorCode,
    message: String,
}

impl AcpFailure {
    fn new(code: AcpErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<ModelRegistryError> for AcpFailure {
    fn from(e: ModelRegistryError) -> Self {
        let code = match &e {
            ModelRegistryError::Storage(inner) => {
                // The only place the sqlx detail is allowed to go: the host's log, never a frame.
                log::error!(
                    target: "tddy_daemon::model_registry",
                    "model registry storage failed: {inner}"
                );
                AcpErrorCode::Internal
            }
            ModelRegistryError::AlreadyExists(_) => AcpErrorCode::InvalidParams,
            ModelRegistryError::NotFound(_) => AcpErrorCode::NotFound,
            ModelRegistryError::InUse(_) => AcpErrorCode::InvalidParams,
            ModelRegistryError::UnknownTool(_) => AcpErrorCode::InvalidParams,
            ModelRegistryError::InvalidName(_) => AcpErrorCode::InvalidParams,
            ModelRegistryError::InvalidBaseUrl(_) => AcpErrorCode::InvalidParams,
            ModelRegistryError::InvalidWorkspace(_) => AcpErrorCode::InvalidParams,
            ModelRegistryError::PermissionDenied(_) => AcpErrorCode::PermissionDenied,
            ModelRegistryError::Provider(_) => AcpErrorCode::ProviderUnavailable,
            ModelRegistryError::UnsupportedOperation(_) => AcpErrorCode::UnsupportedOperation,
        };
        AcpFailure::new(code, e.to_string())
    }
}

fn error_message(id: u64, code: AcpErrorCode, message: String) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::Error(AcpError {
            code: code.as_i64(),
            message,
            data: None,
        })),
    }
}
