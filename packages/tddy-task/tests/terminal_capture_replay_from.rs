//! Acceptance tests: `TerminalCapture::replay_from` serves older history FORWARD from a given
//! offset, bounded above by the anchor (`until_offset`) learned from the initial replay frame.
//!
//! Why this matters: the scroll-up viewport integration fills a second, append-only ghostty-web
//! terminal with older output. ghostty-web can only append (no insert-at-top) and resets are
//! forbidden, so older history must arrive oldest-first and be written forward. `replay_from`
//! returns one forward chunk per call; the client advances `from_offset` to the previous chunk's
//! `end_offset` and calls again until `at_end` (reached `until_offset` or the capture tip).

use tddy_task::{CaptureChunk, TerminalCapture};

/// Byte used for bulk output that carries no escape sequences.
const FILLER_BYTE: u8 = b'A';

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

struct TerminalCaptureBuilder {
    writes: Vec<Vec<u8>>,
}

fn a_terminal_capture() -> TerminalCaptureBuilder {
    TerminalCaptureBuilder { writes: Vec::new() }
}

impl TerminalCaptureBuilder {
    fn with_output(mut self, text: &str) -> Self {
        self.writes.push(text.as_bytes().to_vec());
        self
    }

    fn with_filler_bytes(mut self, count: usize) -> Self {
        self.writes.push(vec![FILLER_BYTE; count]);
        self
    }

    fn with_output_far_exceeding_the_capture_limit(self) -> Self {
        self.with_filler_bytes(3 * TerminalCapture::CAPTURE_LIMIT_BYTES)
    }

    fn build(self) -> TerminalCapture {
        let mut capture = TerminalCapture::new();
        for write in &self.writes {
            capture.append(write);
        }
        capture
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait CaptureChunkAssertions {
    fn assert_data_is(&self, expected: &[u8]) -> &Self;
    fn assert_offsets(&self, start: u64, end: u64) -> &Self;
    fn assert_at_oldest(&self) -> &Self;
    fn assert_not_at_oldest(&self) -> &Self;
    fn assert_at_end(&self) -> &Self;
    fn assert_not_at_end(&self) -> &Self;
    fn assert_empty(&self) -> &Self;
}

impl CaptureChunkAssertions for CaptureChunk {
    fn assert_data_is(&self, expected: &[u8]) -> &Self {
        assert_eq!(self.data, expected, "chunk data mismatch");
        self
    }

    fn assert_offsets(&self, start: u64, end: u64) -> &Self {
        assert_eq!(self.start_offset, start, "chunk start_offset mismatch");
        assert_eq!(self.end_offset, end, "chunk end_offset mismatch");
        self
    }

    fn assert_at_oldest(&self) -> &Self {
        assert!(
            self.at_oldest,
            "expected chunk to be at the ring's oldest byte"
        );
        self
    }

    fn assert_not_at_oldest(&self) -> &Self {
        assert!(
            !self.at_oldest,
            "expected chunk to have older retained bytes below it"
        );
        self
    }

    fn assert_at_end(&self) -> &Self {
        assert!(
            self.at_end,
            "expected chunk to terminate the forward fill (at_end)"
        );
        self
    }

    fn assert_not_at_end(&self) -> &Self {
        assert!(
            !self.at_end,
            "expected chunk to NOT terminate the forward fill (more history to load)"
        );
        self
    }

    fn assert_empty(&self) -> &Self {
        assert!(
            self.data.is_empty(),
            "expected an empty chunk, got {} bytes",
            self.data.len()
        );
        self
    }
}

// ---------------------------------------------------------------------------
// replay_from: forward fill of older history
// ---------------------------------------------------------------------------

#[test]
fn replay_from_returns_the_chunk_starting_at_the_requested_offset_going_forward() {
    // Given a capture holding 10 bytes
    let capture = a_terminal_capture().with_output("0123456789").build();

    // When the first 4 bytes forward from offset 0 are requested, bounded by the anchor at 10
    let chunk = capture.replay_from(0, 10, 4);

    // Then the chunk is bytes 0..4, reaches the oldest retained byte, and does not terminate
    chunk
        .assert_data_is(b"0123")
        .assert_offsets(0, 4)
        .assert_at_oldest()
        .assert_not_at_end();
}

#[test]
fn replay_from_advances_forward_through_the_ring_in_successive_chunks_until_the_anchor() {
    // Given a capture holding 10 bytes
    let capture = a_terminal_capture().with_output("0123456789").build();

    // When a client walks forward from 0 to the anchor at 10, four bytes at a time
    let first = capture.replay_from(0, 10, 4);
    let second = capture.replay_from(first.end_offset, 10, 4);
    let third = capture.replay_from(second.end_offset, 10, 4);

    // Then the chunks tile the buffer front-to-back and the final one reaches the anchor
    first
        .assert_data_is(b"0123")
        .assert_offsets(0, 4)
        .assert_at_oldest()
        .assert_not_at_end();
    second
        .assert_data_is(b"4567")
        .assert_offsets(4, 8)
        .assert_not_at_oldest()
        .assert_not_at_end();
    third
        .assert_data_is(b"89")
        .assert_offsets(8, 10)
        .assert_not_at_oldest()
        .assert_at_end();
}

#[test]
fn replay_from_clamps_from_offset_up_to_the_ring_start_when_older_bytes_were_evicted() {
    // Given a capture that has evicted its earliest 10 bytes
    let capture = a_terminal_capture()
        .with_output("0123456789")
        .with_output_far_exceeding_the_capture_limit()
        .build();
    let oldest = capture.start_offset();

    // When the client asks for bytes forward from offset 0 (below the retained start)
    let chunk = capture.replay_from(0, capture.end_offset(), 64);

    // Then the chunk starts at the ring's oldest retained byte (clamped up) and signals at_oldest
    chunk
        .assert_offsets(oldest, oldest + 64)
        .assert_at_oldest()
        .assert_not_at_end();
}

#[test]
fn replay_from_truncates_the_final_chunk_at_until_offset_so_it_meets_the_live_terminal() {
    // Given a capture holding 10 bytes
    let capture = a_terminal_capture().with_output("0123456789").build();

    // When a large forward chunk is requested from offset 6, bounded by the anchor at 10
    let chunk = capture.replay_from(6, 10, 100);

    // Then the chunk is truncated to the anchor (bytes 6..10) and terminates the fill
    chunk
        .assert_data_is(b"6789")
        .assert_offsets(6, 10)
        .assert_not_at_oldest()
        .assert_at_end();
}

#[test]
fn replay_from_terminates_with_an_empty_at_end_chunk_once_from_offset_reaches_the_anchor() {
    // Given a capture holding 10 bytes
    let capture = a_terminal_capture().with_output("0123456789").build();

    // When the client asks for bytes forward from the anchor itself (nothing left to fill)
    let chunk = capture.replay_from(10, 10, 4);

    // Then the chunk is empty and signals the forward fill is complete
    chunk.assert_empty().assert_at_end();
}

#[test]
fn replay_from_with_until_offset_zero_runs_all_the_way_to_the_capture_tip() {
    // Given a capture holding 10 bytes
    let capture = a_terminal_capture().with_output("0123456789").build();

    // When the client asks for 4 bytes forward from offset 8 with no upper bound (until_offset = 0)
    let chunk = capture.replay_from(8, 0, 4);

    // Then the chunk runs to the capture tip and terminates the fill
    chunk
        .assert_data_is(b"89")
        .assert_offsets(8, 10)
        .assert_not_at_oldest()
        .assert_at_end();
}

#[test]
fn replay_from_of_an_empty_capture_returns_an_empty_at_oldest_at_end_chunk() {
    // Given a capture that has seen no output
    let capture = TerminalCapture::new();

    // When a forward chunk is requested
    let chunk = capture.replay_from(0, 0, 1024);

    // Then the chunk is empty, at the oldest retained byte, and terminates the fill
    chunk
        .assert_empty()
        .assert_offsets(0, 0)
        .assert_at_oldest()
        .assert_at_end();
}
