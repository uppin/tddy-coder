//! Connection lifecycle for the webview-IPC flavour: what the host does when the page it is
//! answering goes away, and what a second page inherits.
//!
//! Departure and refusal are owed by both hosts, so those run against both. What *connecting*
//! does to a connection already open is where the two deliberately part — `WebviewRpcHost`
//! displaces, `MultiConnectionHost` adds — and the test that pins the displacement stays on the
//! host that has it.
//!
//! See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.

#[macro_use]
mod support;

use support::{
    a_multi_connection_host, a_recording_sink, a_request_frame, a_sink_whose_peer_is_gone,
    a_webview_rpc_host, ResponseAssertions, WebviewHost,
};
use tddy_tauri_rpc::FrameError;

against_both_hosts! {
    async fn releases_the_connection_once_the_sink_reports_its_peer_is_gone(host) {
        // Given a connected webview whose peer has gone away
        host.connect_page(a_sink_whose_peer_is_gone(), 1).await;

        // When a call arrives and cannot be answered, and another follows it
        host.handle_request_frame(&a_request_frame().with_id(1).build())
            .await
            .expect("the frame was not accepted");
        let after = host
            .handle_request_frame(&a_request_frame().with_id(2).build())
            .await;

        // Then the host has stopped pretending to serve a page that is not there
        assert_eq!(after, Err(FrameError::NotConnected));
    }

    async fn serves_a_second_page_on_the_same_host_after_the_first_one_is_gone(host) {
        // Given a host whose first page went away mid-call
        host.connect_page(a_sink_whose_peer_is_gone(), 1).await;
        host.handle_request_frame(&a_request_frame().with_id(1).build())
            .await
            .expect("the frame was not accepted");

        // When a new page connects and calls
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 2).await;
        host.handle_request_frame(
            &a_request_frame()
                .with_id(1)
                .with_epoch(2)
                .with_payload(b"second-page")
                .build(),
        )
        .await
        .expect("the frame was not accepted");

        // Then it is served — one page's departure does not retire the host
        frames
            .next_response()
            .await
            .assert_answers(1, "Echo")
            .assert_epoch(2)
            .assert_message(b"second-page")
            .assert_complete();
    }

    async fn refuses_a_frame_whose_epoch_names_no_connection_and_names_the_one_that_exists(host) {
        // Given a host serving exactly one page connection
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 2).await;

        // When a frame stamped with an epoch nothing here answers for arrives
        let refused = host
            .handle_request_frame(&a_request_frame().with_id(1).with_epoch(1).build())
            .await;

        // Then it is refused, naming the connection that does exist, rather than dispatched onto
        // it — where the epoch would not match, the answer would be dropped, and the caller that
        // sent it would wait for an answer that can never arrive
        assert_eq!(
            refused,
            Err(FrameError::StaleConnection {
                connected: 2,
                frame: 1
            })
        );

        // And the connection that does exist is still served
        host.handle_request_frame(
            &a_request_frame()
                .with_id(1)
                .with_epoch(2)
                .with_payload(b"live")
                .build(),
        )
        .await
        .expect("the connected page's frame was not accepted");
        frames
            .next_response()
            .await
            .assert_answers(1, "Echo")
            .assert_message(b"live")
            .assert_complete();
    }
}

/// `WebviewRpcHost` only: reconnecting is what makes the first connection stale here.
///
/// The refusal itself is shared and runs against both hosts above; what cannot be shared is this
/// *setup*. Connecting twice leaves the single-slot host with one connection, the second, so a
/// frame from the first is stale — while on `MultiConnectionHost` it leaves **two**, both live,
/// and the same frame is answered. A reused epoch is the only thing that host refuses at connect,
/// and that is covered in `tests/concurrent_webview_connections.rs`.
#[tokio::test]
async fn refuses_a_frame_from_a_connection_the_webview_has_already_replaced() {
    // Given a page that opened a call, then reloaded and reconnected
    let host = a_webview_rpc_host();
    let (first_sink, _first_frames) = a_recording_sink();
    host.connect(first_sink, 1).await;
    let (second_sink, mut second_frames) = a_recording_sink();
    host.connect(second_sink, 2).await;

    // When a frame from the page that was replaced arrives late
    let stale = host
        .handle_request_frame(&a_request_frame().with_id(1).with_epoch(1).build())
        .await;

    // Then it is refused rather than answered onto the new page's channel, where its epoch would
    // not match and it would be dropped — leaving the caller waiting for an answer forever
    assert_eq!(
        stale,
        Err(FrameError::StaleConnection {
            connected: 2,
            frame: 1
        })
    );

    // And the page that is actually connected is still served
    host.handle_request_frame(
        &a_request_frame()
            .with_id(1)
            .with_epoch(2)
            .with_payload(b"live")
            .build(),
    )
    .await
    .expect("the connected page's frame was not accepted");
    second_frames
        .next_response()
        .await
        .assert_answers(1, "Echo")
        .assert_message(b"live")
        .assert_complete();
}
