//! Length-prefixed frame codec shared by any byte-stream transport (stdio pipes today; anything
//! else that isn't already message-oriented like LiveKit's data channel). A `kind` byte
//! discriminates `Request` vs `Response` frames on a single duplex channel, which is what lets
//! one peer be both an RPC client and server over the same pipe. Fails to compile until
//! `tddy_rpc::transport` exists. See `docs/dev/1-WIP/rpc-multi-transport.md`.

use tddy_rpc::transport::{encode_frame, FrameDecoder, FrameKind};

#[test]
fn encodes_and_decodes_a_single_frame_round_trip() {
    // Given a single encoded Request frame
    let bytes = encode_frame(FrameKind::Request, b"hello");

    // When feeding it into a fresh decoder
    let mut decoder = FrameDecoder::new();
    decoder.feed(&bytes);

    // Then exactly one frame comes out, with the original kind and payload
    let (kind, payload) = decoder.next_frame().expect("expected a decoded frame");
    assert_eq!(kind, FrameKind::Request);
    assert_eq!(payload, b"hello");
    assert!(decoder.next_frame().is_none());
}

#[test]
fn decodes_multiple_frames_delivered_in_one_buffer() {
    // Given two encoded frames concatenated into a single buffer, as a reader might receive them
    let mut buffer = encode_frame(FrameKind::Request, b"first");
    buffer.extend(encode_frame(FrameKind::Response, b"second"));

    // When feeding the whole buffer at once
    let mut decoder = FrameDecoder::new();
    decoder.feed(&buffer);

    // Then both frames are decoded in order, each with its own kind and payload
    let (first_kind, first_payload) = decoder.next_frame().expect("first frame missing");
    let (second_kind, second_payload) = decoder.next_frame().expect("second frame missing");
    assert_eq!(first_kind, FrameKind::Request);
    assert_eq!(first_payload, b"first");
    assert_eq!(second_kind, FrameKind::Response);
    assert_eq!(second_payload, b"second");
    assert!(decoder.next_frame().is_none());
}

#[test]
fn decodes_a_frame_split_across_multiple_reads() {
    // Given one encoded frame, delivered in two separate reads (as a pipe may fragment it)
    let bytes = encode_frame(FrameKind::Response, b"split-payload");
    let (first_half, second_half) = bytes.split_at(bytes.len() / 2);

    // When feeding only the first half
    let mut decoder = FrameDecoder::new();
    decoder.feed(first_half);

    // Then no complete frame is available yet
    assert!(decoder.next_frame().is_none());

    // When feeding the remaining bytes
    decoder.feed(second_half);

    // Then the complete frame is now available, unchanged
    let (kind, payload) = decoder
        .next_frame()
        .expect("frame missing after full delivery");
    assert_eq!(kind, FrameKind::Response);
    assert_eq!(payload, b"split-payload");
}
