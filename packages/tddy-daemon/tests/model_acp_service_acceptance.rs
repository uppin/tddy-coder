//! The daemon's model-addressed `acp.AcpService`: the surface the Models & Agents chat opens
//! against `daemon-{instanceId}`.
//!
//! Driven end to end — a real `AcpClientMessage` stream in, real `AcpAgentMessage` frames out —
//! against a stub provider endpoint, so a daemon that serves no ACP, or one that never builds the
//! provider agent, fails here rather than in a browser.
//!
//! `tddy-acp`'s own suite pins the agent's behavior; this one pins the daemon's surface: the
//! handshake, the registry target, the credential, and the tools an assistant actually runs.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC10).

use std::sync::Arc;

use tddy_daemon::model_registry::{ModelAcpService, ModelRegistryStore, NewAssistant, NewProvider};
use tddy_rpc::{Request, Streaming};
use tddy_service::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, AcpClientMessage, AcpService as _,
    ContentBlock, InitializeRequest, ModelSessionTarget, NewSessionRequest, PromptRequest,
    SessionId, StopReason, TextContent,
};
use tddy_testing_commons::{
    a_stub_http_endpoint_replying_in_sequence, a_stub_http_endpoint_routing, RoutedStubHttpEndpoint,
};
use tokio_stream::StreamExt;

// ---------------------------------------------------------------------------
// Provider payloads
// ---------------------------------------------------------------------------

const COMPLETIONS_PATH: &str = "/v1/chat/completions";
const THIS_DAEMON: &str = "workstation-1";
const VALID_TOKEN: &str = "valid-token";

/// The operator `VALID_TOKEN` resolves to — the owner of the registry rows these tests create.
const THE_OPERATOR: &str = "testuser";

/// A second operator on the same daemon, and the token they present.
const ANOTHER_OPERATOR: &str = "bob";
const ANOTHER_OPERATORS_TOKEN: &str = "bobs-token";

/// Resolves a session token to the operator it belongs to, as the daemon's own resolver does.
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A plain assistant turn.
const COMPLETION_SAYING_HELLO: &str = r#"{"choices":[{"index":0,
  "message":{"role":"assistant","content":"Ollama here, ready."},
  "finish_reason":"stop"}]}"#;

/// A turn in which the model asks to read a file.
const COMPLETION_CALLING_READ: &str = r#"{"choices":[{"index":0,
  "message":{"role":"assistant","content":"",
    "tool_calls":[{"id":"call-1","type":"function",
      "function":{"name":"Read","arguments":"{\"path\":\"README.md\"}"}}]},
  "finish_reason":"tool_calls"}]}"#;

/// The turn that answers once the model has seen the file.
const COMPLETION_QUOTING_THE_README: &str = r#"{"choices":[{"index":0,
  "message":{"role":"assistant","content":"The readme says: tddy."},
  "finish_reason":"stop"}]}"#;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A daemon serving its model-addressed ACP surface over a registry with one Ollama provider
/// pointed at `stub`.
struct Harness {
    _dir: tempfile::TempDir,
    workspace: tempfile::TempDir,
    service: ModelAcpService,
    store: Arc<ModelRegistryStore>,
    stub: RoutedStubHttpEndpoint,
}

impl Harness {
    /// Drive one ACP conversation: handshake, open a session on `target`, send `prompt`, collect
    /// every frame the daemon answered with until the stream ends.
    async fn chat(&self, target: ModelSessionTarget, prompt: &str) -> Vec<AcpAgentMessage> {
        let frames = vec![
            AcpClientMessage {
                id: 1,
                msg: Some(
                    tddy_service::proto::acp::acp_client_message::Msg::Initialize(
                        InitializeRequest::default(),
                    ),
                ),
            },
            AcpClientMessage {
                id: 2,
                msg: Some(
                    tddy_service::proto::acp::acp_client_message::Msg::NewSession(
                        NewSessionRequest {
                            cwd: self.workspace.path().display().to_string(),
                            model_target: Some(target),
                        },
                    ),
                ),
            },
            AcpClientMessage {
                id: 3,
                msg: Some(tddy_service::proto::acp::acp_client_message::Msg::Prompt(
                    PromptRequest {
                        session_id: Some(SessionId {
                            value: String::new(),
                        }),
                        prompt: vec![ContentBlock {
                            block: Some(tddy_service::proto::acp::content_block::Block::Text(
                                TextContent {
                                    text: prompt.to_string(),
                                },
                            )),
                        }],
                    },
                )),
            },
        ];
        let inbound = Streaming::new(tokio_stream::iter(frames.into_iter().map(Ok)));
        let mut outbound = self
            .service
            .session(Request::new(inbound))
            .await
            .expect("the daemon must serve an ACP session")
            .into_inner();

        let mut answered = Vec::new();
        while let Some(frame) = outbound.next().await {
            answered.push(frame.expect("the daemon must not fail the stream"));
        }
        answered
    }
}

async fn a_daemon_serving(stub: RoutedStubHttpEndpoint) -> Harness {
    let dir = tempfile::tempdir().expect("a tempdir for the registry db");
    let store = Arc::new(
        ModelRegistryStore::open(&dir.path().join("models.db"), THIS_DAEMON)
            .await
            .expect("open the registry store"),
    );
    store
        .create_provider(
            NewProvider {
                kind: tddy_service::proto::models::ProviderKind::Ollama,
                label: "Workstation Ollama".to_string(),
                base_url: stub.base_url(),
                api_key: None,
            },
            THE_OPERATOR,
        )
        .await
        .expect("the provider must be created");

    let user_resolver: UserResolver = Arc::new(|token| match token {
        VALID_TOKEN => Some(THE_OPERATOR.to_string()),
        ANOTHER_OPERATORS_TOKEN => Some(ANOTHER_OPERATOR.to_string()),
        _ => None,
    });
    let service = ModelAcpService::new(
        Arc::clone(&store),
        tddy_task::TaskRegistry::new(),
        user_resolver,
    );
    Harness {
        _dir: dir,
        workspace: tempfile::tempdir().expect("a tempdir for the chat workspace"),
        service,
        store,
        stub,
    }
}

/// A target naming the registry's model directly (the Models table's Chat action).
fn the_model_qwen() -> ModelSessionTarget {
    ModelSessionTarget {
        session_token: VALID_TOKEN.to_string(),
        provider_id: "prov-ollama".to_string(),
        model_id: "qwen3:32b".to_string(),
        assistant_id: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Assertions on the answered frames
// ---------------------------------------------------------------------------

/// Every agent text chunk, concatenated — what the chat pane would render.
fn agent_text(frames: &[AcpAgentMessage]) -> String {
    frames
        .iter()
        .filter_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => n.update.as_ref(),
            _ => None,
        })
        .filter_map(|update| match update.update.as_ref() {
            Some(session_update::Update::AgentMessageChunk(chunk)) => chunk.content.as_ref(),
            _ => None,
        })
        .filter_map(|content| match content.block.as_ref() {
            Some(tddy_service::proto::acp::content_block::Block::Text(text)) => {
                Some(text.text.clone())
            }
            _ => None,
        })
        .collect()
}

/// The titles of the tool calls the agent reported.
fn tool_call_titles(frames: &[AcpAgentMessage]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => n.update.as_ref(),
            _ => None,
        })
        .filter_map(|update| match update.update.as_ref() {
            Some(session_update::Update::ToolCall(call)) => Some(call.title.clone()),
            _ => None,
        })
        .collect()
}

/// The turn's stop reason, or a panic naming what came back instead.
fn stop_reason(frames: &[AcpAgentMessage]) -> StopReason {
    frames
        .iter()
        .find_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::Prompt(response)) => {
                Some(StopReason::try_from(response.stop_reason).expect("a known stop reason"))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no PromptResponse among the answered frames: {frames:?}"))
}

/// The error the daemon reported, or a panic if it reported none.
fn error_text(frames: &[AcpAgentMessage]) -> String {
    frames
        .iter()
        .find_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::Error(e)) => Some(e.message.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no error among the answered frames: {frames:?}"))
}

/// The model the opened session reports as current.
fn current_model(frames: &[AcpAgentMessage]) -> String {
    frames
        .iter()
        .find_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::NewSession(response)) => response.models.as_ref(),
            _ => None,
        })
        .map(|models| models.current_model_id.clone())
        .unwrap_or_else(|| panic!("no NewSessionResponse among the answered frames: {frames:?}"))
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chatting_with_a_registry_model_streams_the_provider_reply_back_over_acp() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let harness = a_daemon_serving(stub).await;

    // When
    let frames = harness.chat(the_model_qwen(), "Are you there?").await;

    // Then
    assert_eq!(agent_text(&frames), "Ollama here, ready.");
    assert_eq!(stop_reason(&frames), StopReason::EndTurn);
}

#[tokio::test]
async fn the_session_speaks_as_the_model_the_target_named() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let harness = a_daemon_serving(stub).await;

    // When
    let frames = harness.chat(the_model_qwen(), "Are you there?").await;

    // Then — the model the operator picked is what reached the provider
    assert_eq!(current_model(&frames), "qwen3:32b");
    assert_eq!(
        harness.stub.json_body_for(COMPLETIONS_PATH)["model"],
        "qwen3:32b"
    );
}

#[tokio::test]
async fn an_assistants_assigned_tools_run_against_the_sessions_workspace() {
    // Given — a model that asks to read a file, then answers once it has seen it
    let stub = a_stub_http_endpoint_replying_in_sequence(&[(
        COMPLETIONS_PATH,
        &[COMPLETION_CALLING_READ, COMPLETION_QUOTING_THE_README],
    )])
    .await;
    let harness = a_daemon_serving(stub).await;
    std::fs::write(harness.workspace.path().join("README.md"), "tddy")
        .expect("seed the workspace readme");
    let assistant = harness
        .store
        .create_assistant(
            NewAssistant {
                name: "repo-explorer".to_string(),
                label: "Repo explorer".to_string(),
                provider_id: "prov-ollama".to_string(),
                model_id: "qwen3:32b".to_string(),
                system_prompt: "You explore repositories.".to_string(),
                tools: vec!["Read".to_string()],
            },
            THE_OPERATOR,
        )
        .await
        .expect("the assistant must be created");

    // When
    let frames = harness
        .chat(
            ModelSessionTarget {
                session_token: VALID_TOKEN.to_string(),
                provider_id: String::new(),
                model_id: String::new(),
                assistant_id: assistant.assistant_id,
            },
            "What does the readme say?",
        )
        .await;

    // Then — the tool ran, its output went back to the model, and the model answered from it
    assert_eq!(tool_call_titles(&frames), vec!["Read".to_string()]);
    assert_eq!(agent_text(&frames), "The readme says: tddy.");
    let tool_result = harness.stub.requests_to(COMPLETIONS_PATH)[1].body.clone();
    assert!(
        tool_result.contains("tddy"),
        "the model must be shown what the tool read; sent: {tool_result}"
    );
}

#[tokio::test]
async fn the_assistants_system_prompt_leads_its_conversation() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let harness = a_daemon_serving(stub).await;
    let assistant = harness
        .store
        .create_assistant(
            NewAssistant {
                name: "repo-explorer".to_string(),
                label: String::new(),
                provider_id: "prov-ollama".to_string(),
                model_id: "qwen3:32b".to_string(),
                system_prompt: "You explore repositories.".to_string(),
                tools: Vec::new(),
            },
            THE_OPERATOR,
        )
        .await
        .expect("the assistant must be created");

    // When
    harness
        .chat(
            ModelSessionTarget {
                session_token: VALID_TOKEN.to_string(),
                provider_id: String::new(),
                model_id: String::new(),
                assistant_id: assistant.assistant_id,
            },
            "Are you there?",
        )
        .await;

    // Then
    let sent = harness.stub.json_body_for(COMPLETIONS_PATH);
    assert_eq!(sent["messages"][0]["role"], "system");
    assert_eq!(sent["messages"][0]["content"], "You explore repositories.");
}

#[tokio::test]
async fn a_session_opened_without_a_valid_token_is_refused() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let harness = a_daemon_serving(stub).await;

    // When
    let frames = harness
        .chat(
            ModelSessionTarget {
                session_token: "expired-token".to_string(),
                ..the_model_qwen()
            },
            "Are you there?",
        )
        .await;

    // Then — and nothing was ever asked of the provider
    assert_eq!(error_text(&frames), "invalid or expired session token");
    assert_eq!(harness.stub.paths(), Vec::<String>::new());
}

#[tokio::test]
async fn a_session_opened_on_a_model_no_provider_offers_is_refused() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let harness = a_daemon_serving(stub).await;

    // When
    let frames = harness
        .chat(
            ModelSessionTarget {
                provider_id: "prov-fireworks".to_string(),
                ..the_model_qwen()
            },
            "Are you there?",
        )
        .await;

    // Then
    assert_eq!(
        error_text(&frames),
        "not found: no provider prov-fireworks on this daemon"
    );
}

#[tokio::test]
async fn a_chat_opened_against_another_operators_provider_is_refused() {
    // Given a provider the first operator configured (`a_daemon_serving` owns it as THE_OPERATOR)
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let harness = a_daemon_serving(stub).await;

    // When a colleague on the same daemon opens a chat with one of its models
    let frames = harness
        .chat(
            ModelSessionTarget {
                session_token: ANOTHER_OPERATORS_TOKEN.to_string(),
                ..the_model_qwen()
            },
            "Are you there?",
        )
        .await;

    // Then he is told why, and nothing was asked of the endpoint — a chat that ran anyway would
    // either use her stored key on her behalf or talk to her provider unauthenticated
    assert!(
        error_text(&frames).contains("permission denied"),
        "expected a permission refusal, got: {}",
        error_text(&frames)
    );
    assert_eq!(harness.stub.paths(), Vec::<String>::new());
}

#[tokio::test]
async fn a_chat_with_an_anthropic_provider_is_refused_rather_than_401ing_every_turn() {
    // Given an Anthropic provider whose endpoint would answer anything asked of it, so only this
    // daemon's own refusal can be what ends the chat
    let stub = a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    let harness = a_daemon_serving(stub).await;
    let anthropic =
        a_stub_http_endpoint_routing(&[(COMPLETIONS_PATH, COMPLETION_SAYING_HELLO)]).await;
    harness
        .store
        .create_provider(
            NewProvider {
                kind: tddy_service::proto::models::ProviderKind::Anthropic,
                label: "Anthropic".to_string(),
                base_url: anthropic.base_url(),
                api_key: Some("sk-ant-secret".to_string()),
            },
            THE_OPERATOR,
        )
        .await
        .expect("the provider must be created");

    // When a chat is opened against one of its models
    let frames = harness
        .chat(
            ModelSessionTarget {
                session_token: VALID_TOKEN.to_string(),
                provider_id: "prov-anthropic".to_string(),
                model_id: "claude-sonnet-4".to_string(),
                assistant_id: String::new(),
            },
            "Are you there?",
        )
        .await;

    // Then the operator is told once, up front, that this daemon's chat cannot speak that api —
    // rather than having the key sent to `/v1/chat/completions`, which Anthropic does not serve
    // and answers 401 to, turn after turn
    let refusal = error_text(&frames);
    assert!(
        refusal.contains("OpenAI-compatible completions api"),
        "the refusal must say what this daemon's chat can and cannot speak; got: {refusal}"
    );
    assert_eq!(anthropic.paths(), Vec::<String>::new());
}
