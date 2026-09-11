//! Acceptance tests that `GetTerminalHistory` at the new coordinate answers with the same frames,
//! at the same offsets, as the old one.
//!
//! `connection.ConnectionService::GetTerminalHistory` does two things: it authenticates, and it
//! calls `serve_get_terminal_history_with` over a `TerminalSessionStore`, copying the chunk
//! field-for-field into its own proto. So the oracle each test compares against is that same bridge
//! call over the same capture ring — which makes these tests about the *plumbing*: a request field
//! the new coordinate forgot to forward, or a chunk field it dropped on the way out, is the way
//! scroll-up silently starts returning the wrong bytes.
//!
//! The frame budget is deliberately tiny (four bytes) so a ten-byte ring exercises the chunking a
//! real ring only reaches after kilobytes.

mod support;

use std::sync::Arc;

use support::{a_history_request, StubStore, StubTerminal, TerminalServiceHost};
use tddy_terminal_rpc::proto::terminal_session::{GetTerminalHistoryRequest, TerminalHistoryChunk};
use tddy_terminal_rpc::serve_get_terminal_history_with;

/// The frame budget both the coordinate and the oracle are given.
const FRAME_BUDGET_BYTES: usize = 4;

/// A terminal holding `output`, plus the host serving it and an oracle store over the same ring.
fn a_terminal_holding(output: &[u8]) -> (TerminalServiceHost, StubStore) {
    let terminal = Arc::new(StubTerminal::new());
    terminal.write(output);
    let host = TerminalServiceHost::sharing_terminal(Arc::clone(&terminal));
    (host, StubStore::sharing(terminal))
}

/// What the old coordinate's bridge call answers for the same request.
async fn the_old_coordinates_chunk(
    store: &StubStore,
    request: GetTerminalHistoryRequest,
) -> TerminalHistoryChunk {
    let mut rx = serve_get_terminal_history_with(store, request, FRAME_BUDGET_BYTES)
        .await
        .expect("the bridge resolved the terminal");
    rx.recv()
        .await
        .expect("the bridge sent a chunk")
        .expect("the chunk is not an error")
}

#[tokio::test]
async fn opens_the_forward_fill_at_the_oldest_retained_byte_as_the_old_coordinate_did() {
    // Given a terminal holding ten bytes, and a client starting its scroll-up fill at zero
    let (host, oracle) = a_terminal_holding(b"0123456789");
    let request = a_history_request();

    // When the new coordinate answers, and the old coordinate's bridge call answers the same request
    let served: Vec<TerminalHistoryChunk> =
        host.all_frames("GetTerminalHistory", request.clone()).await;

    // Then they agree, on the bytes and on the offsets that anchor them
    assert_eq!(
        served,
        vec![the_old_coordinates_chunk(&oracle, request).await]
    );
    assert_eq!(
        served,
        vec![TerminalHistoryChunk {
            data: b"0123".to_vec(),
            start_offset: 0,
            end_offset: 4,
            at_oldest: true,
            at_end: false,
        }]
    );
}

#[tokio::test]
async fn resumes_the_forward_fill_from_the_previous_chunks_end_offset_as_the_old_coordinate_did() {
    // Given a terminal holding ten bytes, and a client resuming where its last chunk ended
    let (host, oracle) = a_terminal_holding(b"0123456789");
    let request = GetTerminalHistoryRequest {
        from_offset: 4,
        ..a_history_request()
    };

    // When the new coordinate answers, and the old coordinate's bridge call answers the same request
    let served: Vec<TerminalHistoryChunk> =
        host.all_frames("GetTerminalHistory", request.clone()).await;

    // Then they agree — the middle of the ring is neither re-sent from the start nor skipped
    assert_eq!(
        served,
        vec![the_old_coordinates_chunk(&oracle, request).await]
    );
    assert_eq!(
        served,
        vec![TerminalHistoryChunk {
            data: b"4567".to_vec(),
            start_offset: 4,
            end_offset: 8,
            at_oldest: false,
            at_end: false,
        }]
    );
}

#[tokio::test]
async fn terminates_the_forward_fill_at_the_anchor_the_stream_reported_as_the_old_coordinate_did() {
    // Given a terminal holding ten bytes, and a client filling up to the anchor its stream frame
    // reported rather than to the live tip
    let (host, oracle) = a_terminal_holding(b"0123456789");
    let request = GetTerminalHistoryRequest {
        from_offset: 4,
        until_offset: 6,
        ..a_history_request()
    };

    // When the new coordinate answers, and the old coordinate's bridge call answers the same request
    let served: Vec<TerminalHistoryChunk> =
        host.all_frames("GetTerminalHistory", request.clone()).await;

    // Then they agree that the fill is finished at the anchor — `until_offset` reached the bridge
    assert_eq!(
        served,
        vec![the_old_coordinates_chunk(&oracle, request).await]
    );
    assert_eq!(
        served,
        vec![TerminalHistoryChunk {
            data: b"45".to_vec(),
            start_offset: 4,
            end_offset: 6,
            at_oldest: false,
            at_end: true,
        }]
    );
}

#[tokio::test]
async fn honours_the_requests_own_frame_budget_as_the_old_coordinate_did() {
    // Given a terminal holding ten bytes, and a client asking for a two-byte frame
    let (host, oracle) = a_terminal_holding(b"0123456789");
    let request = GetTerminalHistoryRequest {
        max_bytes: 2,
        ..a_history_request()
    };

    // When the new coordinate answers, and the old coordinate's bridge call answers the same request
    let served: Vec<TerminalHistoryChunk> =
        host.all_frames("GetTerminalHistory", request.clone()).await;

    // Then they agree on the smaller frame — `max_bytes` is not overwritten by the host's default
    assert_eq!(
        served,
        vec![the_old_coordinates_chunk(&oracle, request).await]
    );
    assert_eq!(
        served,
        vec![TerminalHistoryChunk {
            data: b"01".to_vec(),
            start_offset: 0,
            end_offset: 2,
            at_oldest: true,
            at_end: false,
        }]
    );
}

#[tokio::test]
async fn clamps_a_from_offset_below_the_rings_oldest_byte_as_the_old_coordinate_did() {
    // Given a terminal whose oldest bytes have been evicted, and a client asking below the ring
    let evicted = vec![b'x'; tddy_task::TerminalCapture::CAPTURE_LIMIT_BYTES];
    let (host, oracle) = a_terminal_holding(&evicted);
    host.terminal().write(b"0123456789");
    let request = a_history_request();

    // When the new coordinate answers, and the old coordinate's bridge call answers the same request
    let served: Vec<TerminalHistoryChunk> =
        host.all_frames("GetTerminalHistory", request.clone()).await;

    // Then they agree that the fill starts at the ring's oldest retained byte, not at zero
    assert_eq!(
        served,
        vec![the_old_coordinates_chunk(&oracle, request).await]
    );
    assert_eq!(
        served.first().map(|chunk| chunk.start_offset),
        Some(10),
        "the ring dropped the first ten bytes of the eviction burst, so the fill opens at 10"
    );
}

#[tokio::test]
async fn refuses_a_history_request_whose_session_token_is_not_this_hosts() {
    // Given a terminal holding ten bytes
    let (host, _oracle) = a_terminal_holding(b"0123456789");
    let request = GetTerminalHistoryRequest {
        session_token: "a-token-from-another-host".to_string(),
        ..a_history_request()
    };

    // When a caller asks for history with a token this host does not know
    let status = host.refusal("GetTerminalHistory", request).await;

    // Then it is refused before the capture ring is read — the old coordinate authenticated first too
    assert_eq!(status.code(), tddy_rpc::Code::Unauthenticated);
}
