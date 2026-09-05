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
