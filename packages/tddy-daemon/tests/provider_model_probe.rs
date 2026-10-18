//! The daemon's HTTP model probe, against a local stand-in for a provider endpoint.
//!
//! What matters is the request it sends: the OpenAI root is used as given (Meta's and Fireworks'
//! roots already end in `/v1`, and a second one would 404), the key travels as a bearer token, and
//! an Anthropic-only endpoint is asked at `/v1/models` with `x-api-key`.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-accounts.md

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::routing::get;
use axum::{Json, Router};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_accounts::{ProviderEndpoints, ProviderModelProbe};
use tddy_credentials::SecretString;
use tddy_daemon::provider_model_probe::HttpProviderModelProbe;

#[derive(Clone, Default)]
struct Seen {
    requests: Arc<Mutex<Vec<(String, HeaderMap)>>>,
}

async fn models(
    State(seen): State<Seen>,
    uri: Uri,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    seen.requests
        .lock()
        .unwrap()
        .push((uri.path().to_string(), headers));
    (
        StatusCode::OK,
        Json(
            json!({ "object": "list", "data": [ { "id": "muse-spark-1.3" }, { "id": "muse-wonder-1.0" } ] }),
        ),
    )
}

async fn a_provider_endpoint() -> (String, Seen) {
    let seen = Seen::default();
    let app = Router::new()
        .route("/v1/models", get(models))
        .route("/inference/v1/models", get(models))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (root, seen)
}

fn a_probe() -> HttpProviderModelProbe {
    HttpProviderModelProbe::new(reqwest::Client::new())
}

#[tokio::test]
async fn the_http_probe_lists_models_from_the_openai_root_without_doubling_v1() {
    // Given
    let (root, seen) = a_provider_endpoint().await;
    let endpoints = ProviderEndpoints {
        openai_chat: Some(format!("{root}/v1")),
        openai_responses: Some(format!("{root}/v1")),
        anthropic_messages: Some(root.clone()),
    };

    // When
    let listed = a_probe()
        .list_models(&endpoints, &SecretString::new("a-key".to_string()))
        .await
        .expect("models");

    // Then
    assert_eq!(listed, vec!["muse-spark-1.3", "muse-wonder-1.0"]);
    let requests = seen.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    let (path, headers) = &requests[0];
    assert_eq!(path, "/v1/models");
    assert_eq!(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        Some("Bearer a-key")
    );
}

#[tokio::test]
async fn an_anthropic_only_endpoint_is_asked_at_v1_models_with_an_api_key_header() {
    // Given
    let (root, seen) = a_provider_endpoint().await;
    let endpoints = ProviderEndpoints {
        anthropic_messages: Some(root.clone()),
        ..ProviderEndpoints::default()
    };

    // When
    a_probe()
        .list_models(
            &endpoints,
            &SecretString::new("an-anthropic-key".to_string()),
        )
        .await
        .expect("models");

    // Then
    let requests = seen.requests.lock().unwrap().clone();
    let (path, headers) = &requests[0];
    assert_eq!(path, "/v1/models");
    assert_eq!(
        headers.get("x-api-key").and_then(|v| v.to_str().ok()),
        Some("an-anthropic-key")
    );
    assert!(headers.get("anthropic-version").is_some());
}

#[tokio::test]
async fn a_provider_refusal_is_reported_in_the_providers_words() {
    // Given
    let app = Router::new().route(
        "/v1/models",
        get(|| async { (StatusCode::UNAUTHORIZED, "invalid API key") }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let endpoints = ProviderEndpoints {
        openai_chat: Some(format!("{root}/v1")),
        ..ProviderEndpoints::default()
    };

    // When
    let refusal = a_probe()
        .list_models(&endpoints, &SecretString::new("wrong".to_string()))
        .await
        .expect_err("refused");

    // Then
    assert!(refusal.contains("401"));
    assert!(refusal.contains("invalid API key"));
}
