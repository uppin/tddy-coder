//! Both webview-IPC hosts stamp every request they dispatch as arriving in-process, and nothing a
//! page writes into its request frame can change that.
//!
//! The stamp is what the daemon trusts to tell the person at this machine — the application's own
//! window — from a caller on any other transport: a desktop enrols its first sign-in only when that
//! sign-in arrives in-process. So the host must stamp it itself, and must never read it back from a
//! frame.

#[macro_use]
mod support;

use support::{
    a_multi_connection_host, a_recording_sink, a_request_frame, a_webview_rpc_host,
    ResponseAssertions, WebviewHost,
};

against_both_hosts! {
    async fn stamps_a_request_as_arriving_in_process(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When it asks which transport its call arrived over
        host.handle_request_frame(&a_request_frame().calling("WhichTransport").build())
            .await
            .expect("the frame was not accepted");

        // Then the service was told the in-process bridge
        frames
            .next_response()
            .await
            .assert_message(b"InProcess")
            .assert_complete();
    }

    async fn stamps_a_frame_claiming_another_transport_as_in_process_all_the_same(host) {
        // Given a connected webview
        let (sink, mut frames) = a_recording_sink();
        host.connect_page(sink, 1).await;

        // When its frame claims to have come over the LiveKit room
        host.handle_request_frame(
            &a_request_frame()
                .calling("WhichTransport")
                .claiming_to_come_over("LiveKit")
                .build(),
        )
        .await
        .expect("the frame was not accepted");

        // Then the claim is ignored: the host names the transport, not the frame
        frames
            .next_response()
            .await
            .assert_message(b"InProcess")
            .assert_complete();
    }
}
