//! The daemon's HTTP Coder probe, against a local stand-in for a deployment.
//!
//! What matters is the request it sends — `GET /api/v2/users/me` under the deployment's root, the
//! token in the `Coder-Session-Token` header and nowhere else — and that each way a deployment can
//! fail to name a user stays its own outcome.
//!
//! Feature: docs/ft/daemon/provider-accounts.md

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::routing::get;
use axum::{Json, Router};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_accounts::{CoderDeploymentProbe, CoderDeploymentUrl, CoderProbeError, CoderUser};
use tddy_credentials::SecretString;
use tddy_daemon::coder_deployment_probe::HttpCoderDeploymentProbe;

const ADAS_TOKEN: &str = "adas-coder-session-token";

#[derive(Clone, Default)]
struct Seen {
    requests: Arc<Mutex<Vec<(String, HeaderMap)>>>,
}

/// Coder's `GET /api/v2/users/me`: the token's user, or 401 in Coder's own error shape.
async fn users_me(
    State(seen): State<Seen>,
    uri: Uri,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let token = headers
        .get("coder-session-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    seen.requests
        .lock()
        .unwrap()
        .push((uri.to_string(), headers));
    match token.as_deref() {
        Some(ADAS_TOKEN) => (
            StatusCode::OK,
            Json(json!({
                "id": "3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10",
                "username": "ada-lovelace",
                "email": "ada@example.com",
                "status": "active",
            })),
        ),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "message": "API key is invalid.", "detail": "" })),
        ),
    }
}

async fn serving(app: Router) -> CoderDeploymentUrl {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    CoderDeploymentUrl::parse(&root).expect("a loopback root is a deployment URL")
}

async fn a_coder_deployment() -> (CoderDeploymentUrl, Seen) {
    let seen = Seen::default();
    let app = Router::new()
        .route("/api/v2/users/me", get(users_me))
        .with_state(seen.clone());
    (serving(app).await, seen)
}

fn a_probe() -> HttpCoderDeploymentProbe {
    HttpCoderDeploymentProbe::new(reqwest::Client::new())
}

#[tokio::test]
async fn the_http_probe_sends_the_token_as_the_coder_session_header_and_reads_the_user() {
    // Given
    let (deployment, seen) = a_coder_deployment().await;

    // When
    let user = a_probe()
        .whoami(&deployment, &SecretString::new(ADAS_TOKEN))
        .await;

    // Then
    assert_eq!(
        user,
        Ok(CoderUser {
            id: "3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10".to_string(),
            username: "ada-lovelace".to_string(),
        })
    );
    let requests = seen.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    let (uri, headers) = &requests[0];
    assert_eq!(uri, "/api/v2/users/me");
    assert_eq!(
        headers
            .get("coder-session-token")
            .and_then(|value| value.to_str().ok()),
        Some(ADAS_TOKEN)
    );
    assert_eq!(headers.get("authorization"), None);
}

#[tokio::test]
async fn a_token_the_deployment_does_not_know_is_rejected_in_the_deployments_words() {
    // Given
    let (deployment, _seen) = a_coder_deployment().await;

    // When
    let refusal = a_probe()
        .whoami(&deployment, &SecretString::new("a-token-nobody-issued"))
        .await;

    // Then
    assert!(
        matches!(&refusal, Err(CoderProbeError::Rejected(reason)) if reason.contains("API key is invalid")),
        "expected Rejected carrying the deployment's message, got {refusal:?}"
    );
}

#[tokio::test]
async fn a_url_nothing_listens_on_is_unreachable() {
    // Given a port that was bound and released, so nothing answers on it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let deployment = CoderDeploymentUrl::parse(&root).expect("a loopback root");

    // When
    let refusal = a_probe()
        .whoami(&deployment, &SecretString::new(ADAS_TOKEN))
        .await;

    // Then
    assert!(
        matches!(refusal, Err(CoderProbeError::Unreachable(_))),
        "expected Unreachable, got {refusal:?}"
    );
}

#[tokio::test]
async fn a_server_that_is_not_a_coder_deployment_is_unusable_rather_than_a_rejected_token() {
    // Given something that answers the user path with a web page
    let app = Router::new().route(
        "/api/v2/users/me",
        get(|| async { (StatusCode::OK, "<html>Welcome</html>") }),
    );
    let not_a_deployment = serving(app).await;

    // When
    let refusal = a_probe()
        .whoami(&not_a_deployment, &SecretString::new(ADAS_TOKEN))
        .await;

    // Then
    assert!(
        matches!(&refusal, Err(CoderProbeError::Unusable(seen)) if seen.contains("200")),
        "expected Unusable naming the 200 it was answered with, got {refusal:?}"
    );
}
