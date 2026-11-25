//! `GET /user` — the account a token belongs to — against a GitHub answering from
//! `127.0.0.1`.
//!
//! The commit path derives a missing identity from this answer and persists it into the vault
//! record. The fetch takes its `api_base` as a value, so a test stands a fake in for GitHub and
//! production names `https://api.github.com` — never the environment.

use std::net::SocketAddr;

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use pretty_assertions::assert_eq;
use tddy_github::provider::GitHubUser;

/// A GitHub whose `/user` answers Ada (user 101) when the request bears her token, and `401`
/// otherwise.
async fn a_github_answering_user() -> SocketAddr {
    async fn user(headers: HeaderMap) -> impl IntoResponse {
        let authorized = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            == Some("Bearer ghp_ada_token");
        if !authorized {
            return (
                StatusCode::UNAUTHORIZED,
                r#"{"message":"Bad credentials"}"#.to_string(),
            );
        }
        (
            StatusCode::OK,
            r#"{"login":"ada","id":101,"avatar_url":"https://example.invalid/ada.png","name":"Ada"}"#
                .to_string(),
        )
    }
    let app = Router::new().route("/user", get(user));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

fn api_base_of(github: SocketAddr) -> String {
    format!("http://{github}")
}

/// Ask who `token` belongs to, the way the sync commit path does: on a blocking thread, so
/// the fake keeps the runtime's thread to answer on. (Calling the blocking fetch directly from
/// this single-threaded test runtime deadlocks: the fetch waits for curl, curl waits for the
/// fake, and the fake waits for the thread the fetch holds.)
async fn fetch_as_the_commit_path_does(api_base: &str, token: &str) -> Result<GitHubUser, String> {
    let (api_base, token) = (api_base.to_string(), token.to_string());
    tokio::task::spawn_blocking(move || {
        tddy_github::user_api::fetch_authenticated_user(&api_base, &token)
    })
    .await
    .expect("the fetch finishes")
}

#[tokio::test]
async fn fetch_returns_the_user_the_token_belongs_to() {
    // Given a GitHub answering /user
    let github = a_github_answering_user().await;

    // When the commit path asks who Ada's token belongs to
    let user = fetch_as_the_commit_path_does(&api_base_of(github), "ghp_ada_token")
        .await
        .expect("GitHub answers");

    // Then it is Ada, user 101
    assert_eq!(user.id, 101);
    assert_eq!(user.login, "ada");
}

#[tokio::test]
async fn a_rejected_token_is_an_error_naming_the_status() {
    // Given a GitHub answering /user
    let github = a_github_answering_user().await;

    // When the commit path asks with a token GitHub rejects
    let failure = fetch_as_the_commit_path_does(&api_base_of(github), "ghp_revoked")
        .await
        .expect_err("GitHub refuses");

    // Then the refusal names the status, for the derivation message to carry
    assert!(
        failure.contains("401"),
        "the refusal does not name the status: {failure}"
    );
}

#[test]
fn a_blank_token_is_refused_before_curl_runs() {
    // Given no credential at all
    // When the commit path asks with it
    let failure = tddy_github::user_api::fetch_authenticated_user("http://127.0.0.1:9", "")
        .expect_err("a blank token must not reach the network, not even a fake's address");

    // Then it is refused
    assert!(!failure.is_empty());
}
