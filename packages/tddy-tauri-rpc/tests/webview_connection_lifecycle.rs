//! Connection lifecycle for the webview-IPC flavour: what the host does when the page it is
//! answering goes away, and what a second page inherits.
//!
//! See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.

mod support;

use support::{
    a_recording_sink, a_request_frame, a_sink_whose_peer_is_gone, a_webview_rpc_host,
    ResponseAssertions,
};
use tddy_tauri_rpc::FrameError;

#[tokio::test]
async fn releases_the_connection_once_the_sink_reports_its_peer_is_gone() {
    // Given a connected webview whose peer has gone away
    let host = a_webview_rpc_host();
    host.connect(a_sink_whose_peer_is_gone(), 1).await;

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

#[tokio::test]
async fn serves_a_second_page_on_the_same_host_after_the_first_one_is_gone() {
    // Given a host whose first page went away mid-call
    let host = a_webview_rpc_host();
    host.connect(a_sink_whose_peer_is_gone(), 1).await;
    host.handle_request_frame(&a_request_frame().with_id(1).build())
        .await
        .expect("the frame was not accepted");

    // When a new page connects and calls
    let (sink, mut frames) = a_recording_sink();
    host.connect(sink, 2).await;
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
