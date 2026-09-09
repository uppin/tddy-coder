use super::{chunk_terminal_output, TERMINAL_OUTPUT_FRAME_MAX_BYTES};

/// A terminal capture with a recognizable, order-sensitive byte pattern so that a bug which
/// drops, duplicates, or reorders a chunk is caught on reassembly.
fn a_terminal_capture_of(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// Concatenate the emitted frames back into a single buffer.
fn reassembled(frames: &[bytes::Bytes]) -> Vec<u8> {
    frames.iter().flat_map(|f| f.iter().copied()).collect()
}

#[test]
fn returns_no_frames_for_an_empty_capture() {
    // Given — a freshly attached session whose capture buffer is empty
    let capture = a_terminal_capture_of(0);

    // When
    let frames = chunk_terminal_output(&capture, 4);

    // Then — nothing to replay means no frame is published
    assert_eq!(frames, Vec::<bytes::Bytes>::new());
}

#[test]
fn keeps_a_capture_that_fits_within_the_limit_as_one_frame() {
    // Given — a 3-byte capture and a 4-byte frame limit
    let capture = a_terminal_capture_of(3);

    // When
    let frames = chunk_terminal_output(&capture, 4);

    // Then — a single frame carrying the whole capture verbatim
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].as_ref(), capture.as_slice());
}

#[test]
fn keeps_a_capture_exactly_at_the_limit_as_one_frame() {
    // Given — a capture whose length equals the frame limit
    let capture = a_terminal_capture_of(4);

    // When
    let frames = chunk_terminal_output(&capture, 4);

    // Then — the boundary case is not split
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].as_ref(), capture.as_slice());
}

#[test]
fn splits_a_capture_larger_than_the_limit_into_multiple_frames() {
    // Given — a 10-byte capture and a 4-byte frame limit
    let capture = a_terminal_capture_of(10);

    // When
    let frames = chunk_terminal_output(&capture, 4);

    // Then — 4 + 4 + 2 = three frames, never one oversized frame
    assert_eq!(frames.len(), 3);
}

#[test]
fn never_emits_a_frame_larger_than_the_limit() {
    // Given — a 10-byte capture and a 4-byte frame limit
    let capture = a_terminal_capture_of(10);

    // When
    let frames = chunk_terminal_output(&capture, 4);

    // Then — every frame, including the remainder, respects the limit
    let sizes: Vec<usize> = frames.iter().map(|f| f.len()).collect();
    assert_eq!(sizes, vec![4, 4, 2]);
}

#[test]
fn reassembling_the_frames_reproduces_the_original_capture() {
    // Given — a 10-byte capture and a 4-byte frame limit
    let capture = a_terminal_capture_of(10);

    // When
    let frames = chunk_terminal_output(&capture, 4);

    // Then — chunking preserves byte content and order exactly
    assert_eq!(reassembled(&frames), capture);
}

#[test]
fn applies_the_default_frame_limit_so_a_long_session_is_not_replayed_as_one_frame() {
    // Given — a one-megabyte capture, representative of a long-lived interactive session
    let capture = a_terminal_capture_of(1024 * 1024);

    // When — chunked at the production default limit
    let frames = chunk_terminal_output(&capture, TERMINAL_OUTPUT_FRAME_MAX_BYTES);

    // Then — the history is split into fixed-size frames that reassemble losslessly
    let expected_frame_count = capture.len().div_ceil(TERMINAL_OUTPUT_FRAME_MAX_BYTES);
    assert_eq!(frames.len(), expected_frame_count);
    assert!(frames
        .iter()
        .all(|f| f.len() <= TERMINAL_OUTPUT_FRAME_MAX_BYTES));
    assert_eq!(reassembled(&frames), capture);
}

#[test]
fn the_default_frame_limit_is_small_enough_to_chunk_a_megabyte_capture() {
    // A one-megabyte session history must yield more than one frame — the whole point of the
    // change is that an oversized single frame can exceed the data-channel message limit and
    // never reach the browser. This guards the constant against being set unhelpfully large.
    const _: () = {
        assert!(TERMINAL_OUTPUT_FRAME_MAX_BYTES > 0);
        assert!(TERMINAL_OUTPUT_FRAME_MAX_BYTES < 1024 * 1024);
    };
}
