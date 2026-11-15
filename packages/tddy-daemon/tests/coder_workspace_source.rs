//! The daemon's HTTP Coder workspace listing, against a local stand-in for a deployment.
//!
//! What matters is the request it sends — `GET /api/v2/workspaces` under the deployment's root,
//! asking for the token's own workspaces, the token in the `Coder-Session-Token` header and
//! nowhere else — and that each way a deployment can fail to list stays its own outcome.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-coder-shell.md

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::routing::get;
use axum::{Json, Router};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_credentials::SecretString;
use tddy_daemon::coder_workspace_source::HttpCoderWorkspaceSource;
use tddy_host_service::coder_workspaces::{
    CoderCredentials, CoderWorkspace, CoderWorkspaceListError, CoderWorkspaceSource,
};

const ADAS_TOKEN: &str = "adas-coder-session-token";

/// One request as the deployment saw it: its path, its decoded query, and its headers.
type SeenRequest = (String, HashMap<String, String>, HeaderMap);

#[derive(Clone, Default)]
struct Seen {
    requests: Arc<Mutex<Vec<SeenRequest>>>,
}

/// Coder's `GET /api/v2/workspaces`: the token's workspaces, or 401 in Coder's own error shape.
async fn workspaces(
    State(seen): State<Seen>,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let token = headers
        .get("coder-session-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    seen.requests
        .lock()
        .unwrap()
        .push((uri.path().to_string(), query, headers));
    match token.as_deref() {
        Some(ADAS_TOKEN) => (
            StatusCode::OK,
            Json(json!({
                "workspaces": [
                    {
                        "id": "0b6f4a0e-51a4-4c0e-8a0e-2f0d6a1c9d11",
                        "name": "devbox",
                        "owner_name": "ada-lovelace",
                        "latest_build": {
                            "status": "running",
                            "resources": [
                                { "agents": [ { "name": "main", "status": "connected" } ] }
                            ],
                        },
                    },
                    {
                        "id": "7c2d9b1f-3e55-4f6a-b0c4-9a8e7d6c5b42",
                        "name": "scratch",
                        "owner_name": "ada-lovelace",
                        "latest_build": { "status": "stopped", "resources": [] },
                    },
                ],
                "count": 2,
            })),
        ),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "message": "API key is invalid.", "detail": "" })),
        ),
    }
}

/// Serves `app` on a loopback port; the root it is reached at.
async fn serving(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    root
}

async fn a_coder_deployment() -> (String, Seen) {
    let seen = Seen::default();
    let app = Router::new()
        .route("/api/v2/workspaces", get(workspaces))
        .with_state(seen.clone());
    (serving(app).await, seen)
}

fn a_source() -> HttpCoderWorkspaceSource {
    HttpCoderWorkspaceSource::new(reqwest::Client::new())
}

fn credentials_at(root: &str, token: &str) -> CoderCredentials {
    CoderCredentials::new(root, SecretString::new(token))
}

fn adas_workspace(name: &str, status: &str) -> CoderWorkspace {
    CoderWorkspace {
        owner: "ada-lovelace".to_string(),
        name: name.to_string(),
        status: status.to_string(),
    }
}

#[tokio::test]
async fn the_http_source_asks_for_the_tokens_own_workspaces_and_reads_each_ones_status() {
    // Given a deployment where Ada has a running workspace and a stopped one
    let (root, seen) = a_coder_deployment().await;

    // When her workspaces are asked for
    let listed = a_source()
        .workspaces(&credentials_at(&root, ADAS_TOKEN))
        .await;

    // Then both come back in the deployment's order, each with its owner and status
    assert_eq!(
        listed,
        Ok(vec![
            adas_workspace("devbox", "running"),
            adas_workspace("scratch", "stopped"),
        ])
    );
    // And it was one request, for the token's own workspaces, with the token in Coder's header
    // and in no other
    let requests = seen.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    let (path, query, headers) = &requests[0];
    assert_eq!(path, "/api/v2/workspaces");
    assert_eq!(
        query,
        &HashMap::from([("q".to_string(), "owner:me".to_string())])
    );
    assert_eq!(
        headers
            .get("coder-session-token")
            .and_then(|value| value.to_str().ok()),
        Some(ADAS_TOKEN)
    );
    assert_eq!(headers.get("authorization"), None);
}

#[tokio::test]
async fn a_user_with_no_workspaces_is_an_empty_list_not_a_failure() {
    // Given a deployment that answers with no workspaces
    let app = Router::new().route(
        "/api/v2/workspaces",
        get(|| async { Json(json!({ "workspaces": [], "count": 0 })) }),
    );
    let root = serving(app).await;

    // When the workspaces are asked for
    let listed = a_source()
        .workspaces(&credentials_at(&root, ADAS_TOKEN))
        .await;

    // Then
    assert_eq!(listed, Ok(Vec::new()));
}

#[tokio::test]
async fn a_token_the_deployment_does_not_know_is_rejected_in_the_deployments_words() {
    // Given
    let (root, _seen) = a_coder_deployment().await;

    // When
    let refusal = a_source()
        .workspaces(&credentials_at(&root, "a-token-nobody-issued"))
        .await;

    // Then
    assert_eq!(
        refusal,
        Err(CoderWorkspaceListError::Rejected(
            "API key is invalid.".to_string()
        ))
    );
}

#[tokio::test]
async fn a_url_nothing_listens_on_is_unreachable() {
    // Given a port that was bound and released, so nothing answers on it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);

    // When
    let refusal = a_source()
        .workspaces(&credentials_at(&root, ADAS_TOKEN))
        .await;

    // Then
    assert!(
        matches!(refusal, Err(CoderWorkspaceListError::Unreachable(_))),
        "expected Unreachable, got {refusal:?}"
    );
}

#[tokio::test]
async fn a_server_that_is_not_a_coder_deployment_is_unusable_rather_than_a_rejected_token() {
    // Given something that answers the workspaces path with a web page
    let app = Router::new().route(
        "/api/v2/workspaces",
        get(|| async { (StatusCode::OK, "<html>Welcome</html>") }),
    );
    let root = serving(app).await;

    // When
    let refusal = a_source()
        .workspaces(&credentials_at(&root, ADAS_TOKEN))
        .await;

    // Then
    assert!(
        matches!(&refusal, Err(CoderWorkspaceListError::Unusable(seen)) if seen.contains("200")),
        "expected Unusable naming the 200 it was answered with, got {refusal:?}"
    );
}

#[tokio::test]
async fn no_failure_carries_the_token() {
    // Given a deployment that rejects every token, and one that is not a deployment at all
    let rejecting = Router::new().route(
        "/api/v2/workspaces",
        get(|| async {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "message": "API key is invalid.", "detail": "" })),
            )
        }),
    );
    let not_a_deployment = Router::new();
    let roots = (serving(rejecting).await, serving(not_a_deployment).await);

    // When each is asked with Ada's token
    let refusals = (
        a_source()
            .workspaces(&credentials_at(&roots.0, ADAS_TOKEN))
            .await,
        a_source()
            .workspaces(&credentials_at(&roots.1, ADAS_TOKEN))
            .await,
    );

    // Then one is a rejection and the other unusable, and neither's words hold the token
    let shown = format!("{refusals:?}");
    assert!(
        matches!(
            &refusals,
            (
                Err(CoderWorkspaceListError::Rejected(_)),
                Err(CoderWorkspaceListError::Unusable(_))
            )
        ),
        "expected a rejection and an unusable answer, got {shown}"
    );
    assert_eq!(shown.contains(ADAS_TOKEN), false, "got {shown}");
}
