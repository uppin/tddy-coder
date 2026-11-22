//! The conditional transport under the daemon's GitHub inbox (`#workqueue` 2/14), against a GitHub
//! that answers from `127.0.0.1` (REST) and a scripted `gh api -i` (the host's `gh` login).
//!
//! A `304` costs no quota, so it is the inbox's common answer — it must come back as `NotModified`
//! with the poll interval, never as an error. Every answer's rate-limit headers are what lets the
//! inbox slow down before the quota runs out.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use pretty_assertions::assert_eq;
use tddy_github::check_runs::parse_check_runs;
use tddy_github::conditional::{
    ConditionalAnswer, ConditionalRead, GhCliConditionalRead, RateLimit, RestConditionalRead,
};
use tddy_github::pr_api::{CheckRun, CheckStatus};
use tddy_github::trunk_feed::{GhApi, GhApiOutput, TrunkReadError};

const THE_ETAG: &str = "W/\"etag-of-pull-7\"";

/// A GitHub serving `repos/acme/app/pulls/7`: `304` when the request carries [`THE_ETAG`], else the
/// PR with that ETag. Every answer carries a poll interval and rate-limit headers.
async fn a_github_serving_one_pull() -> SocketAddr {
    async fn pull(headers: HeaderMap) -> impl IntoResponse {
        let mut out = HeaderMap::new();
        out.insert("x-poll-interval", HeaderValue::from_static("90"));
        out.insert("x-ratelimit-remaining", HeaderValue::from_static("42"));
        out.insert("x-ratelimit-reset", HeaderValue::from_static("1795000000"));
        if headers.get("if-none-match").and_then(|v| v.to_str().ok()) == Some(THE_ETAG) {
            return (StatusCode::NOT_MODIFIED, out, String::new());
        }
        out.insert("etag", HeaderValue::from_static(THE_ETAG));
        (StatusCode::OK, out, r#"{"number":7}"#.to_string())
    }
    let app = Router::new().route("/repos/acme/app/pulls/7", get(pull));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

fn a_rest_reader(github: SocketAddr) -> RestConditionalRead {
    RestConditionalRead::new(
        format!("http://{github}"),
        Arc::new(|| Ok("a-token".to_string())),
    )
}

#[tokio::test]
async fn a_304_is_not_modified_and_keeps_the_poll_interval() {
    // Given
    let github = a_github_serving_one_pull().await;

    // When
    let answer = a_rest_reader(github)
        .get("repos/acme/app/pulls/7", Some(THE_ETAG))
        .await
        .expect("a 304 is an answer");

    // Then
    assert_eq!(
        answer,
        ConditionalAnswer::NotModified {
            poll_interval_ms: Some(90_000),
            rate_limit: Some(RateLimit {
                remaining: 42,
                reset_at_unix_s: 1_795_000_000
            }),
        }
    );
}

#[tokio::test]
async fn rate_limit_headers_are_read_from_every_answer() {
    // Given
    let github = a_github_serving_one_pull().await;

    // When
    let answer = a_rest_reader(github)
        .get("repos/acme/app/pulls/7", None)
        .await
        .expect("a 200 is an answer");

    // Then
    assert_eq!(
        answer,
        ConditionalAnswer::Modified {
            body: r#"{"number":7}"#.to_string(),
            etag: Some(THE_ETAG.to_string()),
            poll_interval_ms: Some(90_000),
            rate_limit: Some(RateLimit {
                remaining: 42,
                reset_at_unix_s: 1_795_000_000
            }),
        }
    );
}

/// `gh api -i` as the host's `gh` prints it: status line, headers, blank line, body.
struct AScriptedGh {
    stdout: &'static str,
    exit_code: i32,
}

#[async_trait]
impl GhApi for AScriptedGh {
    async fn api_with_headers(
        &self,
        _path: &str,
        _if_none_match: Option<&str>,
    ) -> Result<GhApiOutput, TrunkReadError> {
        Ok(GhApiOutput {
            stdout: self.stdout.to_string(),
            exit_code: self.exit_code,
        })
    }
}

#[tokio::test]
async fn gh_api_dash_i_304_reads_as_not_modified_despite_its_exit_code() {
    // Given — gh exits 1 on a 304
    let gh = Arc::new(AScriptedGh {
        stdout: "HTTP/2.0 304 Not Modified\r\nX-Poll-Interval: 60\r\nX-Ratelimit-Remaining: 4999\r\nX-Ratelimit-Reset: 1795000000\r\n\r\n",
        exit_code: 1,
    });

    // When
    let answer = GhCliConditionalRead::new(gh)
        .get("repos/acme/app/pulls/7", Some(THE_ETAG))
        .await
        .expect("a 304 is an answer whatever gh's exit code");

    // Then
    assert_eq!(
        answer,
        ConditionalAnswer::NotModified {
            poll_interval_ms: Some(60_000),
            rate_limit: Some(RateLimit {
                remaining: 4999,
                reset_at_unix_s: 1_795_000_000
            }),
        }
    );
}

#[test]
fn check_runs_carry_status_timing_and_details_url() {
    // Given
    let body = r#"{"total_count":2,"check_runs":[
        {"id":11,"name":"build","status":"completed","conclusion":"success",
         "started_at":"2026-11-22T10:00:00Z","completed_at":"2026-11-22T10:04:30Z",
         "details_url":"https://github.com/acme/app/runs/11"},
        {"id":12,"name":"e2e","status":"in_progress","conclusion":null,
         "started_at":"2026-11-22T10:01:00Z","completed_at":null,
         "details_url":"https://github.com/acme/app/runs/12"}]}"#;

    // When
    let runs = parse_check_runs(body).expect("a check-runs body parses");

    // Then
    assert_eq!(
        runs,
        vec![
            CheckRun {
                id: 11,
                name: "build".to_string(),
                status: CheckStatus::Completed,
                conclusion: "success".to_string(),
                started_at: Some("2026-11-22T10:00:00Z".to_string()),
                completed_at: Some("2026-11-22T10:04:30Z".to_string()),
                details_url: "https://github.com/acme/app/runs/11".to_string(),
            },
            CheckRun {
                id: 12,
                name: "e2e".to_string(),
                status: CheckStatus::InProgress,
                conclusion: String::new(),
                started_at: Some("2026-11-22T10:01:00Z".to_string()),
                completed_at: None,
                details_url: "https://github.com/acme/app/runs/12".to_string(),
            },
        ]
    );
}
