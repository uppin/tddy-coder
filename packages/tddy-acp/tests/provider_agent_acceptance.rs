//! `ProviderAcpAgent` — the ACP agent that fronts a model provider, so a model or an assistant in
//! the Models & Agents screen can be chatted with over the same `acp.AcpService` stream the
//! pr-stack chat already uses.
//!
//! The agent speaks ACP on one side and the provider's OpenAI-compatible HTTP API on the other. It
//! is driven here through the `acp::Agent` trait directly (rather than a full
//! `AgentSideConnection`) so each test pins one behavior; the session updates it emits are captured
//! from the sink it is constructed with.
//!
//! Tool execution is a **port** (`ToolDispatcher`): `tddy-acp` names the tools and reports the
//! calls, while the daemon supplies a `tddy_tool_engine`-backed dispatcher. That keeps this crate
//! free of a dependency on the tool engine, the same way `session_catalog` keeps `tddy-core` free of
//! `tddy-build`.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC10).

use std::cell::RefCell;
use std::rc::Rc;

use agent_client_protocol::{self as acp, Agent as _};
use tddy_acp::provider_agent::{
    ProviderAcpAgent, ProviderAgentConfig, ProviderTool, ToolDispatcher, ToolOutcome,
};
use tddy_testing_commons::{
    a_stub_http_endpoint_replying_in_sequence, a_stub_http_endpoint_routing, RoutedStubHttpEndpoint,
};

// ---------------------------------------------------------------------------
// Provider payloads
// ---------------------------------------------------------------------------

/// The endpoint the agent completes against.
const COMPLETIONS_PATH: &str = "/v1/chat/completions";

/// A plain assistant turn.
const COMPLETION_SAYING_HELLO: &str = r#"{"choices":[{"index":0,
  "message":{"role":"assistant","content":"Ollama here, ready."},
  "finish_reason":"stop"}]}"#;

/// A turn in which the model asks for one tool call.
const COMPLETION_CALLING_READ: &str = r#"{"choices":[{"index":0,
  "message":{"role":"assistant","content":"",
    "tool_calls":[{"id":"call-1","type":"function",
      "function":{"name":"Read","arguments":"{\"path\":\"README.md\"}"}}]},
  "finish_reason":"tool_calls"}]}"#;

/// What the dispatcher's `Read` produces, and therefore what the model must be shown next.
const README_TOOL_OUTPUT: &str = r##"{"content":"# README"}"##;

/// What the dispatcher reports when `Read` could not run.
const READ_FAILURE: &str = "Read failed: README.md: No such file or directory";

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// Records what was dispatched and answers with a fixed outcome.
struct RecordingToolDispatcher {
    outcome: ToolOutcome,
    calls: RefCell<Vec<(String, String)>>,
}

/// A dispatcher whose `Read` succeeds and returns the readme.
fn a_dispatcher_reading_the_readme() -> Rc<RecordingToolDispatcher> {
    Rc::new(RecordingToolDispatcher {
        outcome: ToolOutcome::ok(README_TOOL_OUTPUT),
        calls: RefCell::new(Vec::new()),
    })
}

/// A dispatcher whose `Read` fails and says why.
fn a_dispatcher_whose_read_fails() -> Rc<RecordingToolDispatcher> {
    Rc::new(RecordingToolDispatcher {
        outcome: ToolOutcome::failed(READ_FAILURE),
        calls: RefCell::new(Vec::new()),
    })
}

#[async_trait::async_trait(?Send)]
impl ToolDispatcher for RecordingToolDispatcher {
    fn tool_defs(&self) -> Vec<ProviderTool> {
        vec![ProviderTool {
            name: "Read".to_string(),
            description: "Read a file".to_string(),
            input_schema_json: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#
                .to_string(),
        }]
    }

    async fn execute(&self, name: &str, input_json: &str) -> ToolOutcome {
        self.calls
            .borrow_mut()
            .push((name.to_string(), input_json.to_string()));
        self.outcome.clone()
    }
}

/// Collects the session updates the agent emits.
#[derive(Default, Clone)]
struct CapturedUpdates(Rc<RefCell<Vec<acp::SessionUpdate>>>);

impl CapturedUpdates {
    fn agent_text(&self) -> String {
        self.0
            .borrow()
            .iter()
            .filter_map(|u| match u {
                acp::SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                    acp::ContentBlock::Text(text) => Some(text.text.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn tool_call_titles(&self) -> Vec<String> {
        self.0
            .borrow()
            .iter()
            .filter_map(|u| match u {
                acp::SessionUpdate::ToolCall(call) => Some(call.title.clone()),
                _ => None,
            })
            .collect()
    }

    /// The status carried by every `ToolCallUpdate` the agent emitted, in order. An update with no
    /// status would leave the chat pane showing a call that never resolves, so it panics here
    /// rather than being filtered silently out of the assertion.
    fn tool_call_statuses(&self) -> Vec<acp::ToolCallStatus> {
        self.0
            .borrow()
            .iter()
            .filter_map(|u| match u {
                acp::SessionUpdate::ToolCallUpdate(update) => {
                    Some(update.fields.status.unwrap_or_else(|| {
                        panic!("a tool call update carried no status: {update:?}")
                    }))
                }
                _ => None,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// ACP requires a session cwd. Chatting with a bare model touches no worktree, so any absolute
/// path serves; the agent only forwards it to a tool dispatcher that has one of its own.
const A_SESSION_CWD: &str = "/tmp";

fn a_config_for(base_url: &str) -> ProviderAgentConfig {
    ProviderAgentConfig {
        base_url: base_url.to_string(),
        model: "qwen3:32b".to_string(),
        api_key: None,
        system_prompt: None,
    }
}

fn a_prompt(session_id: &acp::SessionId, text: &str) -> acp::PromptRequest {
    acp::PromptRequest::new(session_id.clone(), vec![acp::ContentBlock::from(text)])
}

/// Build the agent over a dispatcher whose `Read` succeeds, plus its update sink.
fn an_agent(
    config: ProviderAgentConfig,
) -> (
    ProviderAcpAgent,
    CapturedUpdates,
    Rc<RecordingToolDispatcher>,
) {
    an_agent_dispatching(config, a_dispatcher_reading_the_readme())
}

/// Build the agent, its update sink, and the tool dispatcher it runs tools through.
fn an_agent_dispatching(
    config: ProviderAgentConfig,
    tools: Rc<RecordingToolDispatcher>,
) -> (
    ProviderAcpAgent,
    CapturedUpdates,
    Rc<RecordingToolDispatcher>,
) {
    let updates = CapturedUpdates::default();
    let sink = updates.clone();
    let agent = ProviderAcpAgent::new(
        config,
        tools.clone(),
        Box::new(move |update: acp::SessionUpdate| sink.0.borrow_mut().push(update)),
    );
    (agent, updates, tools)
}

/// A provider whose first turn asks for a tool call and whose second answers in words — the two
/// halves of one tool round. Replaying a single body instead would make the "second turn" another
/// tool request, which is a budget-exhaustion test wearing a tool loop's name.
async fn a_provider_calling_a_tool_then_answering() -> RoutedStubHttpEndpoint {
    a_stub_http_endpoint_replying_in_sequence(&[(
        COMPLETIONS_PATH,
        &[COMPLETION_CALLING_READ, COMPLETION_SAYING_HELLO],
    )])
    .await
}

/// The bodies of the completion round trips one prompt made, in order.
fn completion_requests(stub: &RoutedStubHttpEndpoint) -> Vec<serde_json::Value> {
    stub.json_bodies_for(COMPLETIONS_PATH)
}

async fn a_started_session(agent: &ProviderAcpAgent) -> acp::SessionId {
    agent
        .initialize(acp::InitializeRequest::new(acp::ProtocolVersion::V1))
        .await
        .expect("initialize the provider agent");
    agent
        .new_session(acp::NewSessionRequest::new(A_SESSION_CWD))
        .await
        .expect("open a session")
        .session_id
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertises_the_configured_model_as_the_session_default() {
    // Given an agent fronting a provider
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let (agent, _updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    agent
        .initialize(acp::InitializeRequest::new(acp::ProtocolVersion::V1))
        .await
        .expect("initialize the provider agent");

    // When
    let session = agent
        .new_session(acp::NewSessionRequest::new(A_SESSION_CWD))
        .await
        .expect("open a session");

    // Then — the model the assistant was built on, not a menu of everything the host has
    let models = session.models.expect("the session's model state");
    assert_eq!(models.current_model_id.to_string(), "qwen3:32b");
}

// ---------------------------------------------------------------------------
// Prompting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn streams_the_providers_completion_as_agent_message_chunks() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let (agent, updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    let response = agent
        .prompt(a_prompt(&session, "Say hello"))
        .await
        .expect("prompt the provider agent");

    // Then
    assert_eq!(updates.agent_text(), "Ollama here, ready.");
    assert_eq!(response.stop_reason, acp::StopReason::EndTurn);
}

#[tokio::test]
async fn sends_the_configured_model_and_the_operators_prompt_to_the_provider() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let (agent, _updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "Say hello"))
        .await
        .expect("prompt the provider agent");

    // Then
    let sent = stub.json_body_for(COMPLETIONS_PATH);
    assert_eq!(sent["model"], "qwen3:32b");
    assert_eq!(sent["messages"][0]["role"], "user");
    assert_eq!(sent["messages"][0]["content"], "Say hello");
}

#[tokio::test]
async fn leads_the_conversation_with_the_configured_system_prompt() {
    // Given an assistant carrying a system prompt
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let config = ProviderAgentConfig {
        system_prompt: Some("You read code and answer questions about it.".to_string()),
        ..a_config_for(&stub.base_url())
    };
    let (agent, _updates, _tools) = an_agent(config);
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "What does this repo do?"))
        .await
        .expect("prompt the provider agent");

    // Then
    let sent = stub.json_body_for(COMPLETIONS_PATH);
    assert_eq!(sent["messages"][0]["role"], "system");
    assert_eq!(
        sent["messages"][0]["content"],
        "You read code and answer questions about it."
    );
}

#[tokio::test]
async fn fails_the_turn_when_the_provider_is_unreachable() {
    // Given an endpoint serving nothing
    let stub = a_stub_http_endpoint_routing(&[]).await;
    let (agent, updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    let result = agent.prompt(a_prompt(&session, "Say hello")).await;

    // Then — an ACP error, not an empty turn that looks like the model had nothing to say
    assert!(result.is_err(), "expected the prompt to fail");
    assert_eq!(updates.agent_text(), "");
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[tokio::test]
async fn offers_the_dispatchers_tools_to_the_provider() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let (agent, _updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — the assistant's assigned tools reach the model as function definitions
    let sent = stub.json_body_for(COMPLETIONS_PATH);
    assert_eq!(sent["tools"][0]["function"]["name"], "Read");
}

#[tokio::test]
async fn dispatches_a_tool_call_the_model_asked_for() {
    // Given a provider whose first turn requests a tool call and whose second answers
    let stub = a_provider_calling_a_tool_then_answering().await;
    let (agent, _updates, tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — the tool ran once, with the arguments the model supplied
    assert_eq!(
        tools.calls.borrow().clone(),
        vec![("Read".to_string(), r#"{"path":"README.md"}"#.to_string())]
    );
}

#[tokio::test]
async fn shows_the_model_the_tools_result_on_the_next_round() {
    // Given
    let stub = a_provider_calling_a_tool_then_answering().await;
    let (agent, _updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — the second round trip carries the tool result, tied to the call the model made
    let rounds = completion_requests(&stub);
    assert_eq!(rounds.len(), 2);
    assert_eq!(
        rounds[1]["messages"][2],
        serde_json::json!({
            "role": "tool",
            "content": README_TOOL_OUTPUT,
            "tool_call_id": "call-1",
            "name": "Read",
        })
    );
}

#[tokio::test]
async fn ends_the_turn_with_the_answer_the_model_gave_after_its_tool_ran() {
    // Given
    let stub = a_provider_calling_a_tool_then_answering().await;
    let (agent, updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    let response = agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — the turn completes on the model's words, not on an exhausted budget
    assert_eq!(updates.agent_text(), "Ollama here, ready.");
    assert_eq!(response.stop_reason, acp::StopReason::EndTurn);
}

#[tokio::test]
async fn reports_a_dispatched_tool_call_as_a_completed_acp_tool_call_update() {
    // Given
    let stub = a_provider_calling_a_tool_then_answering().await;
    let (agent, updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — the chat pane renders the call, exactly as it does for a coding backend
    assert_eq!(updates.tool_call_titles(), vec!["Read".to_string()]);
    assert_eq!(
        updates.tool_call_statuses(),
        vec![acp::ToolCallStatus::Completed]
    );
}

#[tokio::test]
async fn reports_a_tool_that_failed_as_a_failed_acp_tool_call_update() {
    // Given a dispatcher whose tool cannot run
    let stub = a_provider_calling_a_tool_then_answering().await;
    let (agent, updates, _tools) = an_agent_dispatching(
        a_config_for(&stub.base_url()),
        a_dispatcher_whose_read_fails(),
    );
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — the failure is rendered as a failed call, not hidden behind a completed one
    assert_eq!(
        updates.tool_call_statuses(),
        vec![acp::ToolCallStatus::Failed]
    );
}

#[tokio::test]
async fn tells_the_model_a_tool_failed_instead_of_dropping_the_call() {
    // Given a dispatcher whose tool cannot run
    let stub = a_provider_calling_a_tool_then_answering().await;
    let (agent, _updates, _tools) = an_agent_dispatching(
        a_config_for(&stub.base_url()),
        a_dispatcher_whose_read_fails(),
    );
    let session = a_started_session(&agent).await;

    // When
    agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — a failure is information the model gets to act on
    let rounds = completion_requests(&stub);
    assert_eq!(rounds[1]["messages"][2]["role"], "tool");
    assert_eq!(rounds[1]["messages"][2]["content"], READ_FAILURE);
}

#[tokio::test]
async fn stops_a_turn_whose_model_keeps_asking_for_tools_past_its_round_budget() {
    // Given a provider that answers every round with the same tool request, forever
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_CALLING_READ)]).await;
    let (agent, _updates, _tools) = an_agent(a_config_for(&stub.base_url()));
    let session = a_started_session(&agent).await;

    // When
    let response = agent
        .prompt(a_prompt(&session, "Read the readme"))
        .await
        .expect("prompt the provider agent");

    // Then — the turn ends on the budget, having cost the ten rounds it is allowed plus the
    // eleventh completion that spent the last of them; never an unbounded loop
    assert_eq!(response.stop_reason, acp::StopReason::MaxTurnRequests);
    assert_eq!(completion_requests(&stub).len(), 11);
}
