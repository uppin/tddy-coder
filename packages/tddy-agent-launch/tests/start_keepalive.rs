//! A streamed start that waits on slow provisioning (a clone blocked on a passphrase prompt) keeps
//! its stream alive: a forwarded stream is torn down after 30 s without a frame, so the host
//! emits a keep-alive every few seconds for as long as the wait lasts, and not one after it.
//!
//! The clock is paused, so the tests are deterministic and take no wall time.

use std::time::Duration;

use tddy_agent_launch::start_keepalive::with_keepalive;
use tddy_rpc::Status;
use tddy_service::proto::session::start_session_event::Event;
use tddy_service::proto::session::StartSessionEvent;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

const INTERVAL: Duration = Duration::from_secs(5);

fn a_streaming_sink() -> (
    AttachmentProgressSink,
    UnboundedReceiver<Result<StartSessionEvent, Status>>,
) {
    let (tx, rx) = unbounded_channel();
    (AttachmentProgressSink::streaming(tx), rx)
}

fn drained(rx: &mut UnboundedReceiver<Result<StartSessionEvent, Status>>) -> Vec<Event> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .map(|item| {
            item.expect("an event, not a status")
                .event
                .expect("a variant")
        })
        .collect()
}

async fn takes(wait: Duration) -> &'static str {
    tokio::time::sleep(wait).await;
    "provisioned"
}

#[tokio::test(start_paused = true)]
async fn a_wait_longer_than_the_interval_emits_a_keep_alive_each_interval() {
    // Given a streamed start whose provisioning takes 12 s
    let (sink, mut rx) = a_streaming_sink();

    // When it is awaited with keep-alives every 5 s
    with_keepalive(&sink, INTERVAL, takes(Duration::from_secs(12))).await;

    // Then two keep-alives went out, at 5 s and at 10 s
    let events = drained(&mut rx);
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| matches!(e, Event::KeepAlive(_))));
}

#[tokio::test(start_paused = true)]
async fn the_keep_alive_names_what_the_host_is_doing() {
    let (sink, mut rx) = a_streaming_sink();

    with_keepalive(&sink, INTERVAL, takes(Duration::from_secs(6))).await;

    let events = drained(&mut rx);
    let [Event::KeepAlive(keep_alive)] = events.as_slice() else {
        panic!("expected exactly one keep-alive");
    };
    assert_eq!(keep_alive.activity, "provisioning");
}

#[tokio::test(start_paused = true)]
async fn a_wait_shorter_than_the_interval_emits_nothing() {
    let (sink, mut rx) = a_streaming_sink();

    with_keepalive(&sink, INTERVAL, takes(Duration::from_secs(4))).await;

    assert!(drained(&mut rx).is_empty());
}

#[tokio::test(start_paused = true)]
async fn no_keep_alive_follows_the_end_of_the_wait() {
    // Given a wait that has ended
    let (sink, mut rx) = a_streaming_sink();
    with_keepalive(&sink, INTERVAL, takes(Duration::from_secs(6))).await;
    drained(&mut rx);

    // When a minute passes
    tokio::time::sleep(Duration::from_secs(60)).await;

    // Then nothing more was emitted
    assert!(drained(&mut rx).is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_awaited_work_is_returned_untouched() {
    let (sink, _rx) = a_streaming_sink();

    let result = with_keepalive(&sink, INTERVAL, takes(Duration::from_secs(12))).await;

    assert_eq!(result, "provisioned");
}

#[tokio::test(start_paused = true)]
async fn a_start_with_nowhere_to_report_just_waits() {
    // Given the unary path's sink, which reports nowhere
    let sink = AttachmentProgressSink::discarding();

    // When it is awaited past several intervals
    let result = with_keepalive(&sink, INTERVAL, takes(Duration::from_secs(12))).await;

    // Then the work simply completes
    assert_eq!(result, "provisioned");
}
