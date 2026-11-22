//! The conditional transport under the daemon's GitHub inbox (`#workqueue` 2/14), against a GitHub
//! that answers from `127.0.0.1` (REST) and a scripted `gh api -i` (the host's `gh` login).
//!
//! A `304` costs no quota, so it is the inbox's common answer — it must come back as `NotModified`
//! with the poll interval, never as an error. Every answer's rate-limit headers are what lets the
//! inbox slow down before the quota runs out.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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

/// A GitHub serving `repos/acme/app/commits` that records the `Authorization` header of every request.
async fn a_github_recording_who_reads() -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);
    let app = Router::new().route(
        "/repos/acme/app/commits",
        get(move |headers: HeaderMap| {
            let recorded = Arc::clone(&recorded);
            async move {
                let authorization = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                recorded.lock().unwrap().push(authorization);
                (StatusCode::OK, "[]".to_string())
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (addr, seen)
}

#[tokio::test]
async fn the_rest_reader_sends_the_account_token_resolved_afresh_on_every_read() {
    // Given — a vault whose token is rotated between two reads
    let (github, seen) = a_github_recording_who_reads().await;
    let reads = Arc::new(AtomicUsize::new(0));
    let vault = Arc::clone(&reads);
    let reader = RestConditionalRead::new(
        format!("http://{github}"),
        Arc::new(move || {
            Ok(format!(
                "token-{}",
                vault.fetch_add(1, Ordering::SeqCst) + 1
            ))
        }),
    );

    // When
    for _ in 0..2 {
        reader
            .get("repos/acme/app/commits", None)
            .await
            .expect("a 200 is an answer");
    }

    // Then — each read carried the token the vault held at that moment
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["Bearer token-1".to_string(), "Bearer token-2".to_string()]
    );
}

#[tokio::test]
async fn the_rest_reader_fails_a_read_whose_token_cannot_be_resolved() {
    // Given — a project whose vault holds no usable token
    let (github, seen) = a_github_recording_who_reads().await;
    let reader = RestConditionalRead::new(
        format!("http://{github}"),
        Arc::new(|| {
            Err(TrunkReadError(
                "no token for the project's account".to_string(),
            ))
        }),
    );

    // When
    let answer = reader.get("repos/acme/app/commits", None).await;

    // Then — nothing reached GitHub, and the failure names why
    let error = answer.expect_err("no token, no read");
    assert!(error.message.contains("no token"), "{error:?}");
    assert!(seen.lock().unwrap().is_empty());
}

/// A `gh api -i` that prints `stdout` and exits `exit_code`, recording what it was asked.
struct AGh {
    stdout: String,
    exit_code: i32,
    asked: Mutex<Vec<(String, Option<String>)>>,
}

#[async_trait]
impl GhApi for AGh {
    async fn api_with_headers(
        &self,
        path: &str,
        if_none_match: Option<&str>,
    ) -> Result<GhApiOutput, TrunkReadError> {
        self.asked
            .lock()
            .unwrap()
            .push((path.to_string(), if_none_match.map(str::to_string)));
        Ok(GhApiOutput {
            stdout: self.stdout.clone(),
            exit_code: self.exit_code,
        })
    }
}

#[tokio::test]
async fn gh_api_dash_i_200_answers_its_body_and_etag_and_is_sent_the_held_etag() {
    // Given — gh prints a 200 with an ETag
    let body = r#"[{"sha":"b"}]"#;
    let gh = Arc::new(AGh {
        stdout: format!("HTTP/2.0 200 OK\r\nEtag: \"e2\"\r\nX-Poll-Interval: 60\r\n\r\n{body}"),
        exit_code: 0,
        asked: Mutex::default(),
    });

    // When — it reads holding an older ETag
    let answer = GhCliConditionalRead::new(Arc::clone(&gh) as Arc<dyn GhApi>)
        .get(
            "repos/acme/app/commits?sha=main&per_page=100",
            Some("\"e1\""),
        )
        .await
        .expect("a 200 is an answer");

    // Then
    assert_eq!(
        answer,
        ConditionalAnswer::Modified {
            body: body.to_string(),
            etag: Some("\"e2\"".to_string()),
            poll_interval_ms: Some(60_000),
            rate_limit: None,
        }
    );
    assert_eq!(
        *gh.asked.lock().unwrap(),
        vec![(
            "repos/acme/app/commits?sha=main&per_page=100".to_string(),
            Some("\"e1\"".to_string())
        )]
    );
}
