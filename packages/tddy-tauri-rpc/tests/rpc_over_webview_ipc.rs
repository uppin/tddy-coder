//! Acceptance tests for the webview-IPC RPC flavour: every call shape a Tauri webview issues
//! against its in-process daemon, driven through a fake frame sink instead of a real IPC bridge.
//!
//! Every call shape is pinned on **both** hosts. `WebviewRpcHost` and `MultiConnectionHost` decode
//! and dispatch a request frame in two separate implementations, not one delegating to the other,
//! so a shape that works on the host these tests were written for says nothing about the host the
//! desktop app runs. `against_both_hosts!` runs each body once per host and names which in the
//! output.
//!
//! See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.

#[macro_use]
mod support;

use support::{
    a_multi_connection_host, a_recording_sink, a_request_frame, a_webview_rpc_host,
    ResponseAssertions, WebviewHost, ECHO_SERVICE,
};
use tddy_tauri_rpc::FrameError;

against_both_hosts! {
    async fn calls_a_unary_method_and_returns_the_exact_response_bytes(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When it calls a unary method
        host.handle_request_frame(&a_request_frame().with_id(7).with_payload(b"ping").build())
            .await
            .expect("the frame was not accepted");

        // Then one terminal response carries the exact bytes the service produced
        frames
            .next_response()
            .await
            .assert_answers(7, "Echo")
            .assert_epoch(1)
            .assert_message(b"ping")
            .assert_complete();
    }

    async fn delivers_every_server_stream_message_in_the_order_the_service_produced_them(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When it calls a server-streaming method that produces three messages
        host.handle_request_frame(
            &a_request_frame()
                .with_id(3)
                .calling("EchoStream")
                .with_payload(b"first,second,third")
                .build(),
        )
        .await
        .expect("the frame was not accepted");

        // Then each message arrives on its own frame, in order, ahead of the closing frame
        let payloads = frames.stream_payloads_until_closed().await;
        assert_eq!(
            payloads,
            vec![b"first".to_vec(), b"second".to_vec(), b"third".to_vec()]
        );
    }

    async fn accepts_three_client_stream_messages_before_answering_with_one_response(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When it opens a client stream and sends three messages
        host.handle_request_frame(
            &a_request_frame()
                .with_id(4)
                .calling("Collect")
                .with_payload(b"one")
                .opening_a_request_stream()
                .build(),
        )
        .await
        .expect("the opening frame was not accepted");
        host.handle_request_frame(
            &a_request_frame()
                .with_id(4)
                .with_payload(b"two")
                .continuing_a_request_stream()
                .build(),
        )
        .await
        .expect("the continuation frame was not accepted");
        host.handle_request_frame(
            &a_request_frame()
                .with_id(4)
                .with_payload(b"three")
                .closing_a_request_stream()
                .build(),
        )
        .await
        .expect("the closing frame was not accepted");

        // Then the single response covers all three request messages
        frames
            .next_response()
            .await
            .assert_answers(4, "Collect")
            .assert_message(b"one|two|three")
            .assert_complete();
    }

    async fn answers_a_bidirectional_stream_with_one_response_per_request_message(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When it opens a bidirectional stream and sends two messages
        host.handle_request_frame(
            &a_request_frame()
                .with_id(9)
                .calling("EchoEach")
                .with_payload(b"alpha")
                .opening_a_request_stream()
                .build(),
        )
        .await
        .expect("the opening frame was not accepted");
        host.handle_request_frame(
            &a_request_frame()
                .with_id(9)
                .with_payload(b"beta")
                .closing_a_request_stream()
                .build(),
        )
        .await
        .expect("the closing frame was not accepted");

        // Then one response message comes back per request message
        let payloads = frames.stream_payloads_until_closed().await;
        assert_eq!(payloads, vec![b"ALPHA".to_vec(), b"BETA".to_vec()]);
    }

    async fn keeps_two_concurrent_unary_calls_separate_when_their_frames_interleave(host) {
        // Given a connected webview with two unary calls in flight
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When both request frames are sent before either response is read
        host.handle_request_frame(&a_request_frame().with_id(11).with_payload(b"alpha").build())
            .await
            .expect("the first frame was not accepted");
        host.handle_request_frame(&a_request_frame().with_id(12).with_payload(b"beta").build())
            .await
            .expect("the second frame was not accepted");

        // Then each response carries its own call's payload
        let mut responses = frames.next_responses(2).await;
        responses.sort_by_key(|response| response.request_id);
        responses[0]
            .assert_answers(11, "Echo")
            .assert_message(b"alpha")
            .assert_complete();
        responses[1]
            .assert_answers(12, "Echo")
            .assert_message(b"beta")
            .assert_complete();
    }

    async fn answers_an_unknown_service_with_a_not_found_status(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When it calls a service the host does not serve
        host.handle_request_frame(
            &a_request_frame()
                .with_id(5)
                .on_service("test.NoSuchService")
                .build(),
        )
        .await
        .expect("the frame was not accepted");

        // Then the call is refused rather than left unanswered
        frames
            .next_response()
            .await
            .assert_error("NOT_FOUND", "test.NoSuchService");
    }

    async fn echoes_the_client_epoch_and_call_metadata_back_on_every_response_frame(host) {
        // Given a webview connected as a second page connection
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 4242).await;

        // When it calls a server-streaming method
        host.handle_request_frame(
            &a_request_frame()
                .with_id(8)
                .with_epoch(4242)
                .calling("EchoStream")
                .with_payload(b"only")
                .build(),
        )
        .await
        .expect("the frame was not accepted");

        // Then the stream message and the closing frame both name the call and the connection
        let streamed = frames.next_response().await;
        streamed
            .assert_answers(8, "EchoStream")
            .assert_epoch(4242)
            .assert_message(b"only");
        frames
            .next_response()
            .await
            .assert_answers(8, "EchoStream")
            .assert_epoch(4242)
            .assert_complete();
    }

    async fn refuses_a_request_frame_that_arrives_before_the_webview_connects(host) {
        // Given a host no webview has connected to — the fixture hands it over with no page on it

        // When a request frame arrives
        let result = host.handle_request_frame(&a_request_frame().build()).await;

        // Then it is refused, because no sink exists to answer on
        assert_eq!(result, Err(FrameError::NotConnected));
    }

    async fn refuses_a_malformed_request_frame_and_keeps_serving_the_connection(host) {
        // Given a connected webview that sends bytes which are not an RpcRequest
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;
        let malformed = host.handle_request_frame(&[0xff, 0xff, 0xff, 0xff]).await;

        // When it then sends a well-formed frame
        host.handle_request_frame(
            &a_request_frame()
                .with_id(6)
                .with_payload(b"still-here")
                .build(),
        )
        .await
        .expect("the frame was not accepted");

        // Then the malformed frame was refused and the connection still serves calls
        assert!(
            matches!(malformed, Err(FrameError::Malformed(_))),
            "expected a malformed-frame refusal, got {malformed:?}"
        );
        frames
            .next_response()
            .await
            .assert_answers(6, "Echo")
            .assert_message(b"still-here")
            .assert_complete();
    }

    async fn serves_the_echo_service_under_the_name_the_daemon_registers_it_with(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When it calls a method that the registered service does not have
        host.handle_request_frame(
            &a_request_frame()
                .with_id(1)
                .on_service(ECHO_SERVICE)
                .calling("NoSuchMethod")
                .build(),
        )
        .await
        .expect("the frame was not accepted");

        // Then the service itself answers — the multiplexer routed by name
        frames
            .next_response()
            .await
            .assert_error("UNIMPLEMENTED", "NoSuchMethod");
    }
}

/// `WebviewRpcHost` only: connecting is what abandons here.
///
/// A second `connect` displaces the first page and tears down everything it opened, which is the
/// single-slot host's defining behaviour and precisely what `MultiConnectionHost` exists not to do
/// — there a second connection is additional, and reusing the epoch is refused outright. There is
/// no counterpart to run this against; the multi-connection host's answer to a page reload is
/// `disconnect_all`, covered in `tests/concurrent_webview_connections.rs`.
#[tokio::test]
async fn abandons_streams_opened_by_a_previous_page_connection_when_the_webview_reconnects() {
    // Given a server stream opened by the first page connection
    let host = a_webview_rpc_host();
    let (first_sink, mut first_frames) = a_recording_sink();
    host.connect(first_sink, 1).await;
    host.handle_request_frame(
        &a_request_frame()
            .with_id(2)
            .calling("StreamAndHold")
            .with_payload(b"before-reload")
            .build(),
    )
    .await
    .expect("the frame was not accepted");
    first_frames
        .next_response()
        .await
        .assert_answers(2, "StreamAndHold")
        .assert_message(b"before-reload");

    // When the page reloads, reconnecting with a fresh epoch, and issues a call reusing id 2
    let (second_sink, mut second_frames) = a_recording_sink();
    host.connect(second_sink, 2).await;
    host.handle_request_frame(
        &a_request_frame()
            .with_id(2)
            .with_epoch(2)
            .with_payload(b"after-reload")
            .build(),
    )
    .await
    .expect("the frame was not accepted");

    // Then the first connection is over, and the new one sees only its own call
    assert_eq!(first_frames.closed().await, None);
    second_frames
        .next_response()
        .await
        .assert_answers(2, "Echo")
        .assert_epoch(2)
        .assert_message(b"after-reload")
        .assert_complete();
}
