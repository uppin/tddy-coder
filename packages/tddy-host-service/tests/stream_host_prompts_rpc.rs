//! Integration tests for the `StreamHostPrompts` RPC.
//!
//! The registry's own guarantees — expiry, single-use, unknown-id rejection — are pinned as unit
//! tests in `tddy_host_service::host_prompts`, which is where that logic lives. What only an integration
//! test can pin is the handler's contract with the transport.
//!
//! ⚠ **The teardown test here is not optional.** `packages/tddy-codegen/docs/server-streaming.md`
//! requires a handler whose stream can be *silent* to `tokio::select!` on `tx.closed()` as well as
//! breaking on a send error; otherwise its task leaks, one per subscription, forever. A prompt feed
//! is silent almost all the time — that is its normal state — so this is precisely the leaking case.
//! `StreamHostStats` escapes it only because it emits unconditionally every five seconds, which is
//! why no equivalent test exists for that RPC.
//!
//! Feature: `docs/ft/web/hosts-screen-add-key.md`

use futures_util::StreamExt;
use std::time::Duration;
use tddy_host_service::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::{Code, Request};
use tddy_service::proto::host::{HostService, StreamHostPromptsRequest};
use tempfile::TempDir;

/// How long a test gives the prompt pump to notice its subscriber left.
const TEARDOWN_WINDOW: Duration = Duration::from_millis(300);

fn a_request(token: &str) -> Request<StreamHostPromptsRequest> {
    Request::new(StreamHostPromptsRequest {
        session_token: token.to_string(),
        daemon_instance_id: String::new(),
    })
}

#[tokio::test]
async fn stream_host_prompts_rejects_an_invalid_token() {
    let dir = TempDir::new().unwrap();
    let service = test_service(dir.path());

    let result = service.stream_host_prompts(a_request("not-a-token")).await;

    let status = result.err().expect("an invalid session must be refused");
    assert_eq!(status.code, Code::Unauthenticated);
}

/// A subscriber that goes away must take the server task with it.
///
/// Without the `tx.closed()` arm the pump parks forever waiting on a prompt that will never come,
/// holding its task and its registry subscription for the life of the daemon — once per browser tab
/// that ever opened the Hosts screen. Nothing else in the suite can observe that: the stream is
/// meant to be silent, so silence proves nothing on its own.
#[tokio::test]
async fn stops_the_prompt_pump_once_the_subscriber_is_gone() {
    // Given a subscriber being served by a live prompt pump
    let dir = TempDir::new().unwrap();
    let service = test_service(dir.path());
    let stream = service
        .stream_host_prompts(a_request(TEST_TOKEN))
        .await
        .expect("a valid session subscribes")
        .into_inner();

    // The count is taken before the drop, and taken synchronously: `PumpCount::running` increments
    // in the handler itself, before the task is spawned, so a pump that is running is already
    // counted by the time the subscription is handed back. Without this, `== 0` below also holds
    // for a service that counts nothing at all — and the leak it exists to catch is invisible
    // again.
    assert_eq!(
        service.pending_prompt_pump_count(),
        1,
        "the subscription is being pumped"
    );

    // When the subscriber goes away without ever having been sent anything
    drop(stream);
    tokio::time::sleep(TEARDOWN_WINDOW).await;

    // Then the pump has finished rather than parked on a prompt that will never arrive.
    assert_eq!(
        service.pending_prompt_pump_count(),
        0,
        "a prompt pump outlived its subscriber — the handler is missing its tx.closed() arm, and \
         leaks one task per subscription"
    );
}

/// A stream that *completes* reads to the browser as the daemon dropping the feed, so an idle host
/// must hold the subscription open rather than closing it.
#[tokio::test]
async fn keeps_an_idle_subscription_open_rather_than_completing_it() {
    let dir = TempDir::new().unwrap();
    let service = test_service(dir.path());
    let mut stream = service
        .stream_host_prompts(a_request(TEST_TOKEN))
        .await
        .expect("subscribe")
        .into_inner();

    let settled = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;

    assert!(
        settled.is_err(),
        "an idle prompt stream must stay open and silent, not complete"
    );
}
