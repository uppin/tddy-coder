//! `ProviderAcpAgent` — the ACP agent that fronts a model provider.
//!
//! A model (or an assistant built on one) in the Models & Agents screen is chatted with over the
//! same `acp.AcpService` stream the pr-stack chat already uses: this agent speaks ACP on one side
//! and the provider's OpenAI-compatible HTTP API — reached through [`OpenAiClient`] — on the other.
//!
//! Two seams keep the crate honest:
//!
//! - **Tool execution is a port.** [`ToolDispatcher`] names the tools and runs them; `tddy-acp`
//!   never depends on `tddy-tool-engine`. The daemon supplies the engine-backed implementation, the
//!   same way `session_catalog`'s `BuildCatalogProvider` keeps `tddy-core` free of `tddy-build`.
//! - **Session updates go to an injected sink.** The owner of the agent decides whether an update
//!   becomes an ACP notification on a connection, a web event, or a test assertion.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC10).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use agent_client_protocol as acp;
use tddy_discovery::openai::{
    ChatCompletionRequest, ChatMessage, OpenAiClient, ToolCall as ProviderToolCall, ToolDefinition,
    ToolFunctionDef,
};

/// Sampling temperature for the provider call. Zero, matching the other provider loops in the
/// workspace (`tddy_discovery::subagent`, `FastContextBackend`).
///
/// TODO: carry it on `ProviderAgentConfig` once an assistant can be configured with one.
const TEMPERATURE: f32 = 0.0;

/// How many rounds of tool calls one prompt turn may dispatch.
///
/// A round is: the model asks for tools, the tools run, the model sees their results. A turn that
/// has spent its budget and *still* asks for tools ends with [`acp::StopReason::MaxTurnRequests`]
/// rather than looping against the provider indefinitely, so a model stuck in a tool cycle costs a
/// bounded number of round trips instead of an unbounded bill. The conversation is kept intact, so
/// the operator can simply prompt again.
///
/// Ten, matching the round budget an assistant already gets as a subagent
/// (`tddy_discovery::agent_def::default_max_turns`, and `DEFAULT_MAX_TURNS` in the daemon's
/// `assistant_to_agent_def`), so the same assistant does not behave differently depending on which
/// loop is driving it. One prompt therefore costs at most eleven provider round trips: ten
/// tool rounds plus the completion that answers.
///
/// TODO: make it configurable per assistant, alongside the temperature above.
const MAX_TOOL_ROUNDS_PER_TURN: usize = 10;

/// How long the provider has to accept the connection before the turn fails.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long one completion has to come back, connection included.
///
/// A provider is a third-party endpoint: it can accept the connection and then say nothing at all.
/// Without a deadline the chat stream waits forever, and a LiveKit-routed RPC that never returns
/// never errors either — the operator sees a spinner rather than a failure.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Everything the agent needs to reach one provider and speak as one model.
#[derive(Debug, Clone)]
pub struct ProviderAgentConfig {
    /// Provider root, e.g. `http://127.0.0.1:11434`; `/v1/chat/completions` is appended to it.
    pub base_url: String,
    /// The model this agent speaks as — the session's current model.
    pub model: String,
    /// Bearer token for providers that require one. Local providers (Ollama) need none.
    pub api_key: Option<String>,
    /// An assistant's system prompt; it leads every conversation this agent opens.
    pub system_prompt: Option<String>,
    /// How long the provider has to accept the connection.
    pub connect_timeout: Duration,
    /// How long one completion has to come back.
    pub request_timeout: Duration,
}

impl ProviderAgentConfig {
    /// A config for `model` on `base_url` under the default transport budget, with no credential
    /// and no system prompt. Both are set by assignment, so a caller that forgets one is visible.
    pub fn speaking_to(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            api_key: None,
            system_prompt: None,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

/// One tool the dispatcher can run, in the shape the provider is told about it.
#[derive(Debug, Clone)]
pub struct ProviderTool {
    pub name: String,
    pub description: String,
    /// The tool's JSON-Schema parameter object, as JSON text.
    pub input_schema_json: String,
}

/// What running a tool produced. `output` is fed back to the model verbatim.
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub output: String,
    /// Whether the tool failed. The model is told either way — a failure is information, not a
    /// reason to hide the call.
    pub failed: bool,
}

impl ToolOutcome {
    /// A tool that ran and produced `output`.
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            failed: false,
        }
    }

    /// A tool that failed; `output` describes the failure for the model and the chat pane.
    pub fn failed(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            failed: true,
        }
    }
}

/// The tool-execution port: which tools this agent offers, and how one is run.
///
/// `?Send` because the ACP SDK's traits are, so an implementation may hold `Rc`/`RefCell` state.
#[async_trait::async_trait(?Send)]
pub trait ToolDispatcher {
    /// The tools to offer the model on every turn.
    fn tool_defs(&self) -> Vec<ProviderTool>;

    /// Run `name` with the model's raw JSON arguments.
    async fn execute(&self, name: &str, input_json: &str) -> ToolOutcome;
}

/// Where the agent's session updates go.
pub type SessionUpdateSink = Box<dyn Fn(acp::SessionUpdate)>;

/// An ACP agent whose "model" is a provider endpoint.
pub struct ProviderAcpAgent {
    config: ProviderAgentConfig,
    tools: Rc<dyn ToolDispatcher>,
    sink: SessionUpdateSink,
    client: OpenAiClient,
    /// Conversation history per open session.
    sessions: RefCell<HashMap<acp::SessionId, Vec<ChatMessage>>>,
    /// The cancel signal of the turn currently running for a session — present only while one is
    /// in flight, so a `cancel` that arrives between turns cannot end the next one.
    in_flight: RefCell<HashMap<acp::SessionId, Rc<tokio::sync::Notify>>>,
    next_session: Cell<u64>,
}

impl ProviderAcpAgent {
    pub fn new(
        config: ProviderAgentConfig,
        tools: Rc<dyn ToolDispatcher>,
        sink: SessionUpdateSink,
    ) -> Self {
        let client = OpenAiClient::new(config.base_url.clone())
            .api_key(config.api_key.clone())
            .timeouts(config.connect_timeout, config.request_timeout);
        Self {
            config,
            tools,
            sink,
            client,
            sessions: RefCell::new(HashMap::new()),
            in_flight: RefCell::new(HashMap::new()),
            next_session: Cell::new(0),
        }
    }

    fn emit(&self, update: acp::SessionUpdate) {
        (self.sink)(update);
    }

    /// The history a fresh session starts from: the assistant's system prompt, when it has one.
    fn opening_messages(&self) -> Vec<ChatMessage> {
        self.config
            .system_prompt
            .iter()
            .map(|prompt| ChatMessage::system(prompt.clone()))
            .collect()
    }

    /// Append to a session's history. Errors when the session was never opened.
    fn push_message(
        &self,
        session_id: &acp::SessionId,
        message: ChatMessage,
    ) -> Result<(), acp::Error> {
        self.sessions
            .borrow_mut()
            .get_mut(session_id)
            .ok_or_else(|| provider_error(format!("unknown session {session_id}")))?
            .push(message);
        Ok(())
    }

    /// A snapshot of a session's history, to send to the provider.
    fn history(&self, session_id: &acp::SessionId) -> Result<Vec<ChatMessage>, acp::Error> {
        self.sessions
            .borrow()
            .get(session_id)
            .cloned()
            .ok_or_else(|| provider_error(format!("unknown session {session_id}")))
    }

    /// One provider round-trip: the session's history in, the model's message out.
    async fn complete(
        &self,
        session_id: &acp::SessionId,
        tools: Vec<ToolDefinition>,
    ) -> Result<ChatMessage, acp::Error> {
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: self.history(session_id)?,
            tools,
            tool_choice: serde_json::json!("auto"),
            temperature: TEMPERATURE,
        };
        let response = self.client.complete(request).await.map_err(|e| {
            provider_error(format!("provider {} failed: {e}", self.config.base_url))
        })?;
        response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or_else(|| provider_error("the provider returned no choices"))
    }

    /// Run one tool call the model asked for: report it, dispatch it, report its result, and
    /// return the tool-result message the model is shown next.
    async fn dispatch(&self, call: &ProviderToolCall) -> ChatMessage {
        let tool_call_id = acp::ToolCallId::new(call.id.clone());
        self.emit(acp::SessionUpdate::ToolCall(
            acp::ToolCall::new(tool_call_id.clone(), call.function.name.clone())
                .status(acp::ToolCallStatus::InProgress)
                .raw_input(
                    serde_json::from_str::<serde_json::Value>(&call.function.arguments).ok(),
                ),
        ));

        let outcome = self
            .tools
            .execute(&call.function.name, &call.function.arguments)
            .await;

        self.emit(acp::SessionUpdate::ToolCallUpdate(
            acp::ToolCallUpdate::new(
                tool_call_id,
                acp::ToolCallUpdateFields::new()
                    .status(if outcome.failed {
                        acp::ToolCallStatus::Failed
                    } else {
                        acp::ToolCallStatus::Completed
                    })
                    .content(vec![acp::ToolCallContent::from(outcome.output.clone())])
                    // Also as `raw_output`, because a client that mirrors ACP over protobuf carries
                    // `raw_output` but not `content` — without this the call surfaces as
                    // pending → completed with no result text at all.
                    .raw_output(Some(tool_output_value(&outcome.output))),
            ),
        ));

        ChatMessage::tool_result(outcome.output, call.id.clone(), call.function.name.clone())
    }

    /// The cancel signal for `session_id`'s in-flight turn, registered for the life of that turn.
    fn begin_turn(&self, session_id: &acp::SessionId) -> Rc<tokio::sync::Notify> {
        let signal = Rc::new(tokio::sync::Notify::new());
        self.in_flight
            .borrow_mut()
            .insert(session_id.clone(), Rc::clone(&signal));
        signal
    }

    /// Forget `session_id`'s cancel signal, so a later `cancel` cannot end a turn that is not
    /// running.
    fn end_turn(&self, session_id: &acp::SessionId) {
        self.in_flight.borrow_mut().remove(session_id);
    }
}

/// A tool's raw output as JSON. Tools answer with JSON text, so it is carried structured when it
/// parses and as a JSON string when it does not — never dropped, and never a fabricated shape.
fn tool_output_value(output: &str) -> serde_json::Value {
    serde_json::from_str(output).unwrap_or_else(|_| serde_json::Value::String(output.to_string()))
}

#[async_trait::async_trait(?Send)]
impl acp::Agent for ProviderAcpAgent {
    async fn initialize(
        &self,
        _args: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(
            acp::InitializeResponse::new(acp::ProtocolVersion::V1).agent_info(
                acp::Implementation::new("tddy-provider-agent", env!("CARGO_PKG_VERSION"))
                    .title("TDDY Provider Agent"),
            ),
        )
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
        let ordinal = self.next_session.get();
        self.next_session.set(ordinal + 1);
        let session_id = acp::SessionId::new(format!("provider-{ordinal}"));
        self.sessions
            .borrow_mut()
            .insert(session_id.clone(), self.opening_messages());

        // The agent speaks as exactly one model — the one the assistant was built on — so that is
        // both the current model and the whole menu.
        let model_id = acp::ModelId::new(self.config.model.clone());
        Ok(
            acp::NewSessionResponse::new(session_id).models(acp::SessionModelState::new(
                model_id.clone(),
                vec![acp::ModelInfo::new(model_id, self.config.model.clone())],
            )),
        )
    }

    async fn prompt(&self, args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        let session_id = args.session_id;
        let cancelled = self.begin_turn(&session_id);
        let outcome = self.run_turn(&session_id, args.prompt, &cancelled).await;
        self.end_turn(&session_id);
        outcome
    }

    /// End the session's in-flight turn, if one is running.
    ///
    /// `cancel` is an ACP *notification*: the turn it interrupts is what reports the outcome, as
    /// [`acp::StopReason::Cancelled`] on its own `prompt` response. A cancel that arrives when no
    /// turn is running has nothing to interrupt, and a session that was never opened is a client
    /// error rather than something to ignore.
    async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        if !self.sessions.borrow().contains_key(&args.session_id) {
            return Err(provider_error(format!(
                "unknown session {}",
                args.session_id
            )));
        }
        if let Some(signal) = self.in_flight.borrow().get(&args.session_id) {
            // `notify_waiters` deliberately stores no permit: a cancel with no turn in flight must
            // not end the next prompt the operator sends.
            signal.notify_waiters();
        }
        Ok(())
    }
}

impl ProviderAcpAgent {
    /// One prompt turn, abandoned as soon as `cancelled` fires.
    ///
    /// The `select!`s are what make cancellation real rather than advisory: dropping the provider
    /// call's future cancels the HTTP request, and dropping the dispatch future stops the tool
    /// loop before the next tool starts.
    async fn run_turn(
        &self,
        session_id: &acp::SessionId,
        prompt: Vec<acp::ContentBlock>,
        cancelled: &tokio::sync::Notify,
    ) -> Result<acp::PromptResponse, acp::Error> {
        self.push_message(session_id, ChatMessage::user(prompt_text(&prompt)?))?;
        let tools = tool_definitions(&self.tools.tool_defs())?;

        // Registered once, before the first await, so a cancel racing the first provider call is
        // still observed by this turn.
        let cancelled = cancelled.notified();
        tokio::pin!(cancelled);

        let mut rounds_left = MAX_TOOL_ROUNDS_PER_TURN;
        loop {
            let message = tokio::select! {
                biased;
                () = &mut cancelled => return Ok(acp::PromptResponse::new(acp::StopReason::Cancelled)),
                message = self.complete(session_id, tools.clone()) => message?,
            };

            if let Some(text) = message.content.clone().filter(|t| !t.is_empty()) {
                self.emit(acp::SessionUpdate::AgentMessageChunk(
                    acp::ContentChunk::new(text.into()),
                ));
            }

            let calls = message.tool_calls.clone().unwrap_or_default();
            // No tool calls — either absent, or the empty list Ollama sends for a plain turn.
            if calls.is_empty() {
                self.push_message(session_id, ChatMessage::assistant(message.content, None))?;
                return Ok(acp::PromptResponse::new(acp::StopReason::EndTurn));
            }

            if rounds_left == 0 {
                // The unanswered tool calls are dropped from the history on purpose: a provider
                // rejects an assistant tool-call message that no tool result follows, which would
                // poison every later prompt in this session.
                self.push_message(session_id, ChatMessage::assistant(message.content, None))?;
                return Ok(acp::PromptResponse::new(acp::StopReason::MaxTurnRequests));
            }
            rounds_left -= 1;

            self.push_message(
                session_id,
                ChatMessage::assistant(message.content, Some(calls.clone())),
            )?;
            for call in &calls {
                let result = tokio::select! {
                    biased;
                    () = &mut cancelled => return Ok(acp::PromptResponse::new(acp::StopReason::Cancelled)),
                    result = self.dispatch(call) => result,
                };
                self.push_message(session_id, result)?;
            }
        }
    }
}

/// The user's message as the provider's text. Non-text blocks are refused rather than dropped, so
/// a client never believes it sent context the model never saw.
fn prompt_text(blocks: &[acp::ContentBlock]) -> Result<String, acp::Error> {
    let mut text = String::new();
    for block in blocks {
        match block {
            acp::ContentBlock::Text(t) => text.push_str(&t.text),
            // TODO: carry images, audio and resources once the provider agent needs them.
            other => {
                return Err(acp::Error::invalid_params().data(serde_json::json!({
                    "details": format!("unsupported prompt content block: {other:?}"),
                })))
            }
        }
    }
    if text.is_empty() {
        return Err(acp::Error::invalid_params().data(serde_json::json!({
            "details": "the prompt carried no text",
        })));
    }
    Ok(text)
}

/// The dispatcher's tools as OpenAI function definitions. A tool whose schema is not JSON fails the
/// turn — silently offering the model a broken tool would waste the whole conversation.
fn tool_definitions(tools: &[ProviderTool]) -> Result<Vec<ToolDefinition>, acp::Error> {
    tools
        .iter()
        .map(|tool| {
            Ok(ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: serde_json::from_str(&tool.input_schema_json).map_err(|e| {
                        provider_error(format!(
                            "tool {} has an unparseable input schema: {e}",
                            tool.name
                        ))
                    })?,
                },
            })
        })
        .collect()
}

/// An ACP internal error carrying `message`, so the failure reaches the chat pane instead of
/// surfacing as an empty turn.
fn provider_error(message: impl Into<String>) -> acp::Error {
    acp::Error::new(i32::from(acp::ErrorCode::InternalError), message)
}
