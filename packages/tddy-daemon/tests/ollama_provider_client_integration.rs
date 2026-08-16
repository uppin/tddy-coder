//! The Ollama provider client's wire contract, pinned against a loopback HTTP stub that answers
//! the four endpoints the client uses: `/api/tags` (what is pulled), `/api/show` (capabilities),
//! `/api/ps` (what is resident) and `/api/generate` (residency control via `keep_alive`).
//!
//! A stub rather than a real Ollama: CI has no GPU and no pulled models, but the request shapes and
//! the response parsing are exactly what breaks silently, so they are worth pinning.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md.

use tddy_daemon::model_registry::{
    CredentialStyle, ModelRegistryError, OllamaProviderClient, OpenAiCompatibleProviderClient,
    ProviderClient, ProviderHttp,
};
use tddy_service::proto::models::ModelLoadState;
use tddy_testing_commons::{
    a_stub_http_endpoint_replying_in_sequence, a_stub_http_endpoint_routing, RoutedStubHttpEndpoint,
};

// ---------------------------------------------------------------------------
// Fixtures — the payload shapes Ollama actually returns
// ---------------------------------------------------------------------------

const TAGS_WITH_TWO_MODELS: &str = r#"{"models":[
  {"name":"qwen3:32b","size":20000000000,"details":{"family":"qwen3"}},
  {"name":"nomic-embed-text:latest","size":274000000,"details":{"family":"nomic-bert"}}
]}"#;

const SHOW_A_TOOL_CAPABLE_CHAT_MODEL: &str =
    r#"{"capabilities":["completion","tools"],"details":{"family":"qwen3"}}"#;

const SHOW_AN_EMBEDDING_MODEL: &str =
    r#"{"capabilities":["embedding"],"details":{"family":"nomic-bert"}}"#;

const PS_WITH_ONE_RESIDENT_MODEL: &str =
    r#"{"models":[{"name":"qwen3:32b","expires_at":"2026-08-16T12:00:00Z"}]}"#;

const GENERATE_ACK: &str = r#"{"model":"qwen3:32b","response":"","done":true}"#;

const CLOUD_MODELS_LISTING: &str =
    r#"{"data":[{"id":"accounts/fireworks/models/kimi-k2","object":"model"}]}"#;

/// An Ollama serving two pulled models — a tool-capable chat model and an embedding model — of
/// which only the chat model is resident.
///
/// `/api/show` is scripted **in sequence**, one answer per model in the order `/api/tags` lists
/// them: a single replayed answer would label both models identically, so the per-model capability
/// lookup would never be exercised.
async fn an_ollama_serving_a_chat_and_an_embedding_model() -> RoutedStubHttpEndpoint {
    a_stub_http_endpoint_replying_in_sequence(&[
        ("/api/tags", &[TAGS_WITH_TWO_MODELS]),
        (
            "/api/show",
            &[SHOW_A_TOOL_CAPABLE_CHAT_MODEL, SHOW_AN_EMBEDDING_MODEL],
        ),
        ("/api/ps", &[PS_WITH_ONE_RESIDENT_MODEL]),
    ])
    .await
}

// ---------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enumerates_the_models_the_host_has_pulled_in_the_order_it_lists_them() {
    // Given an Ollama reporting two pulled models
    let stub = an_ollama_serving_a_chat_and_an_embedding_model().await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let models = client.list_models().await.expect("enumerate the models");

    // Then
    let ids: Vec<String> = models.iter().map(|m| m.model_id.clone()).collect();
    assert_eq!(
        ids,
        vec![
            "qwen3:32b".to_string(),
            "nomic-embed-text:latest".to_string()
        ]
    );
}

#[tokio::test]
async fn labels_each_enumerated_model_from_its_own_capability_lookup() {
    // Given an Ollama whose two models report different capabilities
    let stub = an_ollama_serving_a_chat_and_an_embedding_model().await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let models = client.list_models().await.expect("enumerate the models");

    // Then — each row carries its own model's labels, not the first model's
    assert_eq!(
        models[0].labels,
        vec!["llm".to_string(), "tools".to_string()]
    );
    assert_eq!(models[1].labels, vec!["embedding".to_string()]);
}

#[tokio::test]
async fn reports_each_enumerated_model_with_the_residency_ps_gave_it() {
    // Given an Ollama holding only the chat model in memory
    let stub = an_ollama_serving_a_chat_and_an_embedding_model().await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let models = client.list_models().await.expect("enumerate the models");

    // Then — the catalog is cross-referenced against `/api/ps`, per row
    assert_eq!(models[0].load_state, ModelLoadState::Loaded as i32);
    assert_eq!(models[1].load_state, ModelLoadState::NotLoaded as i32);
}

#[tokio::test]
async fn stamps_each_enumerated_model_with_its_provider_daemon_and_size() {
    // Given
    let stub = an_ollama_serving_a_chat_and_an_embedding_model().await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let models = client.list_models().await.expect("enumerate the models");

    // Then — the merged, cross-daemon table can tell whose model this row is
    assert_eq!(models[0].provider_id, "prov-ollama");
    assert_eq!(models[0].daemon_instance_id, "workstation-1");
    assert_eq!(models[0].size_bytes, 20_000_000_000);
}

#[tokio::test]
async fn fails_enumeration_when_the_host_is_not_serving_the_tags_endpoint() {
    // Given an endpoint that serves nothing (every path 404s)
    let stub = a_stub_http_endpoint_routing(&[]).await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let result = client.list_models().await;

    // Then — an error, never an empty catalog that looks like "this host has no models"
    assert!(
        matches!(result, Err(ModelRegistryError::Provider(_))),
        "expected a provider error, got {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Residency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reports_a_model_listed_by_ps_as_resident() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[("/api/ps", PS_WITH_ONE_RESIDENT_MODEL)]).await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let state = client
        .load_state("qwen3:32b")
        .await
        .expect("read residency");

    // Then
    assert_eq!(state, ModelLoadState::Loaded);
}

#[tokio::test]
async fn reports_a_model_absent_from_ps_as_not_resident() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[("/api/ps", PS_WITH_ONE_RESIDENT_MODEL)]).await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let state = client
        .load_state("nomic-embed-text:latest")
        .await
        .expect("read residency");

    // Then
    assert_eq!(state, ModelLoadState::NotLoaded);
}

#[tokio::test]
async fn loads_a_model_with_a_keep_alive_generate() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[("/api/generate", GENERATE_ACK)]).await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    client.load("qwen3:32b").await.expect("load the model");

    // Then — the request names the model and asks Ollama to keep it resident
    assert_eq!(stub.paths(), vec!["/api/generate".to_string()]);
    let sent = stub.json_body_for("/api/generate");
    assert_eq!(sent["model"], "qwen3:32b");
    assert_eq!(sent["keep_alive"], "10m");
}

#[tokio::test]
async fn unloads_a_model_with_a_zero_keep_alive_generate() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[("/api/generate", GENERATE_ACK)]).await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    client.unload("qwen3:32b").await.expect("unload the model");

    // Then — `keep_alive: 0` is what evicts it from VRAM
    let sent = stub.json_body_for("/api/generate");
    assert_eq!(sent["model"], "qwen3:32b");
    assert_eq!(sent["keep_alive"], 0);
}

// ---------------------------------------------------------------------------
// Cloud providers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enumerates_a_cloud_provider_from_its_openai_models_endpoint() {
    // Given an OpenAI-compatible endpoint
    let stub = a_stub_http_endpoint_routing(&[("/v1/models", CLOUD_MODELS_LISTING)]).await;
    let client = OpenAiCompatibleProviderClient::new(
        &stub.base_url(),
        "prov-fireworks",
        "workstation-1",
        Some("fw-secret-key".to_string()),
    );

    // When
    let models = client.list_models().await.expect("enumerate the models");

    // Then
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model_id, "accounts/fireworks/models/kimi-k2");
    assert_eq!(models[0].load_state, ModelLoadState::Unsupported as i32);
}

#[tokio::test]
async fn presents_the_stored_api_key_to_a_cloud_provider_as_a_bearer_token() {
    // Given a cloud provider configured with a key
    let stub = a_stub_http_endpoint_routing(&[("/v1/models", CLOUD_MODELS_LISTING)]).await;
    let client = OpenAiCompatibleProviderClient::new(
        &stub.base_url(),
        "prov-fireworks",
        "workstation-1",
        Some("fw-secret-key".to_string()),
    );

    // When
    client.list_models().await.expect("enumerate the models");

    // Then — the key stored on this daemon actually reaches the endpoint; a provider that never
    // sees it answers 401, and the operator is told their catalog is empty
    assert_eq!(
        stub.header_for("/v1/models", "authorization"),
        Some("Bearer fw-secret-key".to_string())
    );
}

#[tokio::test]
async fn sends_no_authorization_header_for_a_keyless_provider() {
    // Given a provider configured without a key
    let stub = a_stub_http_endpoint_routing(&[("/v1/models", CLOUD_MODELS_LISTING)]).await;
    let client = OpenAiCompatibleProviderClient::new(
        &stub.base_url(),
        "prov-local-openai",
        "workstation-1",
        None,
    );

    // When
    client.list_models().await.expect("enumerate the models");

    // Then — nothing is invented to send in a missing key's place
    assert_eq!(stub.header_for("/v1/models", "authorization"), None);
}

#[tokio::test]
async fn refuses_to_load_a_cloud_provider_model() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[]).await;
    let client = OpenAiCompatibleProviderClient::new(
        &stub.base_url(),
        "prov-fireworks",
        "workstation-1",
        None,
    );

    // When
    let result = client.load("accounts/fireworks/models/kimi-k2").await;

    // Then — residency has no meaning here; saying so beats pretending it worked
    assert!(
        matches!(result, Err(ModelRegistryError::UnsupportedOperation(_))),
        "expected UnsupportedOperation, got {result:?}"
    );
    assert!(stub.paths().is_empty(), "no request should have been made");
}

// ---------------------------------------------------------------------------
// The credential an Ollama provider was configured with
// ---------------------------------------------------------------------------

#[tokio::test]
async fn presents_the_stored_api_key_to_ollama() {
    // Given an Ollama published behind something that authenticates — a reverse proxy, an access
    // gateway, a hosted tier — and a provider row configured with the key it wants
    let stub = a_stub_http_endpoint_routing(&[("/api/ps", PS_WITH_ONE_RESIDENT_MODEL)]).await;
    let client = OllamaProviderClient::new(
        &stub.base_url(),
        "prov-ollama",
        "workstation-1",
        Some("proxy-secret".to_string()),
    );

    // When
    client
        .load_state("qwen3:32b")
        .await
        .expect("read residency");

    // Then — the key the registry stored actually reaches the endpoint. Dropping it would leave
    // the provider row saying `has_credential: true` while every request went out anonymous.
    assert_eq!(
        stub.header_for("/api/ps", "authorization"),
        Some("Bearer proxy-secret".to_string())
    );
}

#[tokio::test]
async fn sends_no_authorization_header_to_a_keyless_ollama() {
    // Given
    let stub = a_stub_http_endpoint_routing(&[("/api/ps", PS_WITH_ONE_RESIDENT_MODEL)]).await;
    let client = OllamaProviderClient::new(&stub.base_url(), "prov-ollama", "workstation-1", None);

    // When
    client
        .load_state("qwen3:32b")
        .await
        .expect("read residency");

    // Then — nothing is invented in a missing key's place
    assert_eq!(stub.header_for("/api/ps", "authorization"), None);
}

// ---------------------------------------------------------------------------
// An Anthropic provider is authenticated the way Anthropic authenticates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn presents_an_anthropic_key_as_x_api_key_rather_than_a_bearer_token() {
    // Given an Anthropic provider
    let stub = a_stub_http_endpoint_routing(&[("/v1/models", CLOUD_MODELS_LISTING)]).await;
    let client = OpenAiCompatibleProviderClient::with_credential_style(
        &stub.base_url(),
        "prov-anthropic",
        "workstation-1",
        Some("sk-ant-secret".to_string()),
        CredentialStyle::AnthropicApiKey,
    );

    // When
    client.list_models().await.expect("enumerate the models");

    // Then — Anthropic refuses `Authorization: Bearer` and requires a version, so a bearer token
    // would 401 every request and land the 401 body in the provider's enumeration error
    assert_eq!(
        stub.header_for("/v1/models", "x-api-key"),
        Some("sk-ant-secret".to_string())
    );
    assert_eq!(
        stub.header_for("/v1/models", "anthropic-version"),
        Some("2023-06-01".to_string())
    );
    assert_eq!(stub.header_for("/v1/models", "authorization"), None);
}

// ---------------------------------------------------------------------------
// A provider that does not answer, and one that answers with far too much
// ---------------------------------------------------------------------------

/// A loopback socket under this test's control. Dropping it stops accepting.
struct RawHost {
    port: u16,
    _accepting: tokio::task::JoinHandle<()>,
}

impl RawHost {
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// A host that accepts the connection and then says nothing at all — how a wedged provider (a GPU
/// box swapping, a proxy holding the request open) behaves. Not the same as a refused connection,
/// which fails immediately on its own.
async fn a_host_that_accepts_and_never_answers() -> RawHost {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback listener");
    let port = listener.local_addr().expect("the bound address").port();
    let accepting = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            // Held open, unanswered, for as long as the test runs.
            held.push(socket);
        }
    });
    RawHost {
        port,
        _accepting: accepting,
    }
}

/// A host answering every request with `status_line` and `body`.
async fn a_host_answering_with(status_line: &'static str, body: String) -> RawHost {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback listener");
    let port = listener.local_addr().expect("the bound address").port();
    let accepting = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        while let Ok((mut socket, _)) = listener.accept().await {
            let body = body.clone();
            tokio::spawn(async move {
                // Drain enough of the request that the client is not reset mid-write.
                let mut buffer = [0_u8; 4096];
                let _ = socket.read(&mut buffer).await;
                let response = format!(
                    "{status_line}\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });
    RawHost {
        port,
        _accepting: accepting,
    }
}

/// How long a test is willing to wait for a client that is supposed to give up quickly.
const A_GENEROUS_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// A budget short enough that a test notices it, standing in for the production one.
fn a_short_budget() -> ProviderHttp {
    ProviderHttp {
        connect_timeout: std::time::Duration::from_millis(500),
        request_timeout: std::time::Duration::from_millis(300),
        enumeration_budget: std::time::Duration::from_secs(5),
    }
}

#[tokio::test]
async fn gives_up_on_a_provider_that_accepts_the_connection_and_then_says_nothing() {
    // Given a host that never answers
    let host = a_host_that_accepts_and_never_answers().await;
    let client = OllamaProviderClient::new(&host.base_url(), "prov-ollama", "workstation-1", None)
        .with_http_config(a_short_budget());

    // When
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(A_GENEROUS_WAIT, client.list_models()).await;

    // Then the RPC that asked gets an answer. Without a request timeout it would wait forever —
    // and a LiveKit-routed RPC that never returns never errors either, so the operator would be
    // left with a spinner rather than a failure.
    let result = result.expect("the client must not wait indefinitely");
    assert!(
        matches!(result, Err(ModelRegistryError::Provider(_))),
        "expected a provider error, got {result:?}"
    );
    assert!(
        started.elapsed() < A_GENEROUS_WAIT,
        "gave up only after {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn gives_up_on_an_enumeration_that_outlasts_its_budget() {
    // Given a host that never answers, and a per-request timeout longer than the whole budget, so
    // only the budget can be what ends this
    let host = a_host_that_accepts_and_never_answers().await;
    let client = OllamaProviderClient::new(&host.base_url(), "prov-ollama", "workstation-1", None)
        .with_http_config(ProviderHttp {
            connect_timeout: std::time::Duration::from_millis(500),
            request_timeout: std::time::Duration::from_secs(30),
            enumeration_budget: std::time::Duration::from_millis(300),
        });

    // When
    let result = tokio::time::timeout(A_GENEROUS_WAIT, client.list_models())
        .await
        .expect("the enumeration must not run past its budget");

    // Then — enumeration is `/api/tags`, `/api/ps` and one `/api/show` per pulled model, so a big
    // library is dozens of round trips inside one RefreshProviderModels; the walk as a whole is
    // bounded, not just each step
    let message = match result {
        Err(ModelRegistryError::Provider(message)) => message,
        other => panic!("expected a provider error, got {other:?}"),
    };
    assert!(
        message.contains("took longer than"),
        "the failure must say the budget ran out; got: {message}"
    );
}

#[tokio::test]
async fn keeps_only_a_bounded_part_of_a_providers_error_page() {
    // Given a gateway answering with a large HTML error page, as they do
    let host = a_host_answering_with(
        "HTTP/1.1 502 Bad Gateway",
        "<html><body>".to_string() + &"e".repeat(200_000) + "</body></html>",
    )
    .await;
    let client = OllamaProviderClient::new(&host.base_url(), "prov-ollama", "workstation-1", None);

    // When
    let result = client.list_models().await;

    // Then — this message is persisted on the provider row and returned by every ListProviders; a
    // payload past ~60 KB is chunk-framed over LiveKit, where one lost frame wedges the call with
    // no error at all
    let message = match result {
        Err(ModelRegistryError::Provider(message)) => message,
        other => panic!("expected a provider error, got {other:?}"),
    };
    assert!(
        message.len() < 1_000,
        "the provider's error page became a {}-byte message",
        message.len()
    );
    assert!(
        message.contains("502"),
        "the failure must still say what the provider answered; got: {message}"
    );
}
