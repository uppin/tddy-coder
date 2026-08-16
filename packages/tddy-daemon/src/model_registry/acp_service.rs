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
use super::store::ModelRegistryStore;
use super::tool_dispatcher::EngineToolDispatcher;
use crate::task_service::SessionUserResolver;

/// Outbound frames for one stream. Unbounded because the agent's update sink is a plain `Fn` with
/// nowhere to await back-pressure, and dropping an update would silently truncate the transcript.
type OutboundSender = tokio::sync::mpsc::UnboundedSender<Result<AcpAgentMessage, Status>>;

/// The daemon's model-addressed ACP surface.
pub struct ModelAcpService {
    store: Arc<ModelRegistryStore>,
    tasks: TaskRegistry,
    user_resolver: SessionUserResolver,
}

impl ModelAcpService {
    pub fn new(
        store: Arc<ModelRegistryStore>,
        tasks: TaskRegistry,
        user_resolver: SessionUserResolver,
    ) -> Self {
        Self {
            store,
            tasks,
            user_resolver,
        }
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
        let store = Arc::clone(&self.store);
        let tasks = self.tasks.clone();
        let user_resolver = Arc::clone(&self.user_resolver);

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
                local.block_on(
                    &runtime,
                    serve_stream(inbound, out_tx, store, tasks, user_resolver),
                );
            })
            .map_err(|e| Status::internal(format!("could not start the model chat thread: {e}")))?;

        Ok(Response::new(UnboundedReceiverStream::new(out_rx)))
    }
}

/// The whole ACP conversation for one stream, on the thread that owns the agent.
async fn serve_stream(
    mut inbound: Streaming<AcpClientMessage>,
    out: OutboundSender,
    store: Arc<ModelRegistryStore>,
    tasks: TaskRegistry,
    user_resolver: SessionUserResolver,
) {
    // The ACP session handle, set by `new_session`; every session update is stamped with it.
    let session_id = Rc::new(RefCell::new(String::new()));
    // What this stream's tool calls are attributed to in the daemon's task registry. Distinct from
    // the ACP handle, which is only unique within one agent, and known before the first frame.
    let chat_id = format!("model-chat-{}", uuid::Uuid::now_v7());
    let mut agent: Option<ProviderAcpAgent> = None;

    while let Some(Ok(message)) = inbound.next().await {
        let id = message.id;
        let reply = match message.msg {
            Some(acp_client_message::Msg::Initialize(_)) => Ok(initialize_response(id)),
            Some(acp_client_message::Msg::Authenticate(_)) => Ok(AcpAgentMessage {
                id,
                msg: Some(acp_agent_message::Msg::Authenticate(
                    AuthenticateResponse {},
                )),
            }),
            Some(acp_client_message::Msg::NewSession(request)) => {
                match open_session(
                    request,
                    &out,
                    Rc::clone(&session_id),
                    &chat_id,
                    &store,
                    &tasks,
                    &user_resolver,
                )
                .await
                {
                    Ok((opened, response)) => {
                        agent = Some(opened);
                        Ok(AcpAgentMessage {
                            id,
                            msg: Some(acp_agent_message::Msg::NewSession(response)),
                        })
                    }
                    Err(e) => Err(e),
                }
            }
            Some(acp_client_message::Msg::LoadSession(_)) => Err(
                // Nothing is persisted here: a chat with a model is a conversation held for the
                // life of the stream, so "resume session 5" has no answer to give.
                "this daemon's model chat keeps no resumable sessions; open a new one".to_string(),
            ),
            Some(acp_client_message::Msg::Prompt(request)) => match agent.as_ref() {
                Some(agent) => {
                    let sid = session_id.borrow().clone();
                    prompt(agent, request, id, &sid).await
                }
                None => Err("prompt arrived before new_session named a model".to_string()),
            },
            Some(acp_client_message::Msg::Cancel(_)) => {
                if let Some(agent) = agent.as_ref() {
                    let sid = session_id.borrow().clone();
                    let _ = agent
                        .cancel(acp::CancelNotification::new(acp::SessionId::new(sid)))
                        .await;
                }
                continue;
            }
            // Permission replies have no meaning here: this agent never asks for permission.
            Some(acp_client_message::Msg::RequestPermission(_)) | None => continue,
        };

        let frame = reply.unwrap_or_else(|message| error_message(id, message));
        if out.send(Ok(frame)).is_err() {
            return;
        }
    }
}

/// Build the agent the stream will speak through, from the target the client named.
#[allow(clippy::too_many_arguments)] // one stream's whole context; a struct would only rename it
async fn open_session(
    request: NewSessionRequest,
    out: &OutboundSender,
    session_id: Rc<RefCell<String>>,
    chat_id: &str,
    store: &ModelRegistryStore,
    tasks: &TaskRegistry,
    user_resolver: &SessionUserResolver,
) -> Result<(ProviderAcpAgent, NewSessionResponse), String> {
    let target = request
        .model_target
        .ok_or_else(|| "new_session carried no model_target; this surface serves the registry, so it has to be told which model to speak as".to_string())?;
    // The operator this chat is held as. The provider's credential is read on their behalf, so a
    // chat opened against a colleague's provider is refused by `credential_for` rather than run
    // against their endpoint with no key at all.
    let Some(caller) = (user_resolver)(&target.session_token) else {
        return Err("invalid or expired session token".to_string());
    };

    let resolved = resolve_target(&target, store, &caller)
        .await
        .map_err(|e| e.to_string())?;
    let workspace = PathBuf::from(request.cwd.trim());
    if !resolved.tools.is_empty() && workspace.as_os_str().is_empty() {
        return Err(format!(
            "assistant '{}' has tools, so new_session needs a cwd to run them in",
            resolved.model
        ));
    }
    let dispatcher = EngineToolDispatcher::new(&resolved.tools, workspace, tasks.clone(), chat_id)
        .map_err(|e| e.to_string())?;

    let sink_session_id = Rc::clone(&session_id);
    let sink_out = out.clone();
    let agent = ProviderAcpAgent::new(
        ProviderAgentConfig {
            base_url: resolved.base_url,
            model: resolved.model.clone(),
            api_key: resolved.api_key,
            system_prompt: resolved.system_prompt,
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
        .map_err(|e| e.message.to_string())?;
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
) -> Result<AcpAgentMessage, String> {
    let text = prompt_text(&request.prompt);
    let response = agent
        .prompt(acp::PromptRequest::new(
            acp::SessionId::new(session_id.to_string()),
            vec![acp::ContentBlock::from(text)],
        ))
        .await
        .map_err(|e| e.message.to_string())?;
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
) -> Result<ResolvedTarget, ModelRegistryError> {
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
        return Err(ModelRegistryError::NotFound(
            "model_target named neither an assistant nor a model".to_string(),
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

fn error_message(id: u64, message: String) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::Error(AcpError {
            code: -32603, // JSON-RPC internal error, mirroring `acp::Error::internal_error()`
            message,
            data: None,
        })),
    }
}
