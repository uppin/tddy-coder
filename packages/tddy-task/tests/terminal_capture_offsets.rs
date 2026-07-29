//! Acceptance tests: the capture ring tracks absolute byte offsets so a late subscriber can be
//! shown the current last frame immediately and lazily load older history as it scrolls up.
//!
//! Why this matters: dumping the entire ring on reconnect produces a long flash of stale bytes.
//! The lazy replay model sends only the last screen first, then serves older byte ranges on
//! demand as the user scrolls up — terminating when the ring's oldest retained byte is reached.

use tddy_task::{CaptureChunk, TerminalCapture};

/// Byte used for bulk output that carries no escape sequences.
const FILLER_BYTE: u8 = b'A';

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

struct TerminalCaptureBuilder {
    writes: Vec<Vec<u8>>,
}

/// A capture that has seen nothing yet — the state of a freshly spawned PTY.
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

    /// Enough plain output that the ring has to evict everything written before it.
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
    fn assert_data_is_entirely(&self, byte: u8) -> &Self;
    fn assert_data_is(&self, expected: &[u8]) -> &Self;
    fn assert_offsets(&self, start: u64, end: u64) -> &Self;
    fn assert_at_oldest(&self) -> &Self;
    fn assert_not_at_oldest(&self) -> &Self;
    fn assert_empty(&self) -> &Self;
}

impl CaptureChunkAssertions for CaptureChunk {
    fn assert_data_is_entirely(&self, byte: u8) -> &Self {
        let stray = self.data.iter().enumerate().find(|(_, b)| **b != byte);
        assert_eq!(
            stray.map(|(i, b)| format!("'{}' at offset {i}", *b as char)),
            None,
            "chunk data must be nothing but '{}'",
            byte as char
        );
        self
    }

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
// Offset tracking
// ---------------------------------------------------------------------------

#[test]
fn append_advances_end_offset_by_the_written_byte_count() {
    // Given a fresh capture
    let mut capture = TerminalCapture::new();

    // When two writes land
    capture.append(b"hello");
    capture.append(b" world");

    // Then the end offset is the total bytes written, and nothing has been evicted yet
    assert_eq!(capture.start_offset(), 0);
    assert_eq!(capture.end_offset(), 11);
}

#[test]
fn eviction_advances_start_offset_by_the_drained_byte_count() {
    // Given a capture whose ring is full
    let mut capture = TerminalCapture::new();
    capture.append(&vec![FILLER_BYTE; TerminalCapture::CAPTURE_LIMIT_BYTES]);
    let start_before = capture.start_offset();

    // When enough new output arrives to evict the oldest 10 bytes
    capture.append(&[FILLER_BYTE; 10]);

    // Then the start offset has advanced by exactly the evicted count
    assert_eq!(capture.start_offset(), start_before + 10);
    assert_eq!(
        capture.end_offset(),
        TerminalCapture::CAPTURE_LIMIT_BYTES as u64 + 10
    );
}

// ---------------------------------------------------------------------------
// replay_last: the "current last frame" a reconnecting client sees first
// ---------------------------------------------------------------------------

#[test]
fn replay_last_returns_the_tail_bytes_with_their_absolute_offsets() {
    // Given a capture holding 8 bytes
    let capture = a_terminal_capture().with_output("01234567").build();

    // When the last 5 bytes are requested
    let chunk = capture.replay_last(5);

    // Then the chunk is the tail of the buffer, offset to its absolute position
    chunk
        .assert_data_is(b"34567")
        .assert_offsets(3, 8)
        .assert_not_at_oldest();
}

#[test]
fn replay_last_clamps_to_the_full_buffer_when_more_is_requested_than_retained() {
    // Given a capture holding 4 bytes
    let capture = a_terminal_capture().with_output("abcd").build();

    // When the last 10 bytes are requested (more than the ring holds)
    let chunk = capture.replay_last(10);

    // Then the whole buffer is returned, and the chunk reaches the ring's oldest byte
    chunk
        .assert_data_is(b"abcd")
        .assert_offsets(0, 4)
        .assert_at_oldest();
}

#[test]
fn replay_last_of_an_empty_capture_returns_an_at_oldest_empty_chunk() {
    // Given a capture that has seen no output
    let capture = TerminalCapture::new();

    // When the last screen is requested
    let chunk = capture.replay_last(1024);

    // Then the chunk is empty and signals there is no older history to load
    chunk.assert_empty().assert_offsets(0, 0).assert_at_oldest();
}

#[test]
fn replay_last_after_eviction_offsets_relative_to_the_surviving_tail() {
    // Given a capture that has evicted its earliest bytes
    let capture = a_terminal_capture()
        .with_output("0123456789")
        .with_output_far_exceeding_the_capture_limit()
        .build();

    // When the last screen is requested
    let chunk = capture.replay_last(64);

    // Then the chunk's end offset is the total bytes ever written, its start offset is
    // `end - 64`, and it does not reach the oldest retained byte (the ring holds far more)
    chunk
        .assert_data_is_entirely(FILLER_BYTE)
        .assert_offsets(capture.end_offset() - 64, capture.end_offset())
        .assert_not_at_oldest();
}

// ---------------------------------------------------------------------------
// replay_before: the older chunk a client loads by scrolling up
// ---------------------------------------------------------------------------

#[test]
fn replay_before_returns_the_chunk_ending_just_before_the_requested_offset() {
    // Given a capture holding 10 bytes
    let capture = a_terminal_capture().with_output("0123456789").build();

    // When the 4 bytes ending just before offset 8 are requested
    let chunk = capture.replay_before(8, 4);

    // Then the chunk is bytes 4..8, and older bytes remain below it
    chunk
        .assert_data_is(b"4567")
        .assert_offsets(4, 8)
        .assert_not_at_oldest();
}

#[test]
fn replay_before_clamps_to_the_ring_start_when_max_bytes_exceeds_available_history() {
    // Given a capture holding 6 bytes
    let capture = a_terminal_capture().with_output("abcdef").build();

    // When the 100 bytes before offset 6 are requested (more than the ring holds)
    let chunk = capture.replay_before(6, 100);

    // Then the whole buffer is returned and the chunk reaches the oldest retained byte
    chunk
        .assert_data_is(b"abcdef")
        .assert_offsets(0, 6)
        .assert_at_oldest();
}

#[test]
fn replay_before_clamps_before_offset_down_to_the_live_tip_when_it_exceeds_it() {
    // Given a capture holding 5 bytes
    let capture = a_terminal_capture().with_output("abcde").build();

    // When a before_offset past the live tip is requested
    let chunk = capture.replay_before(999, 3);

    // Then the chunk ends at the live tip, not at the requested offset
    chunk
        .assert_data_is(b"cde")
        .assert_offsets(2, 5)
        .assert_not_at_oldest();
}

#[test]
fn replay_before_returns_an_at_oldest_empty_chunk_when_before_offset_reaches_the_ring_start() {
    // Given a capture holding 4 bytes
    let capture = a_terminal_capture().with_output("abcd").build();

    // When the client scrolls all the way up and asks for bytes before the ring's start
    let chunk = capture.replay_before(0, 64);

    // Then the chunk is empty and signals the infinite scroll has terminated
    chunk.assert_empty().assert_at_oldest();
}

#[test]
fn replay_before_terminates_the_scroll_when_the_ring_has_evicted_older_bytes() {
    // Given a capture that has evicted its earliest 10 bytes
    let mut capture = TerminalCapture::new();
    capture.append(b"0123456789");
    capture.append(&vec![FILLER_BYTE; TerminalCapture::CAPTURE_LIMIT_BYTES]);

    // When the client asks for bytes before the oldest retained offset
    let chunk = capture.replay_before(capture.start_offset(), 64);

    // Then the chunk is empty and at_oldest — the eviction means no older history exists
    chunk.assert_empty().assert_at_oldest();
}

#[test]
fn replay_before_walks_backwards_through_the_ring_in_successive_chunks() {
    // Given a capture holding 10 bytes
    let capture = a_terminal_capture().with_output("0123456789").build();

    // When a client first loads the last 4 bytes, then the 4 before them, then the rest
    let first = capture.replay_last(4);
    let second = capture.replay_before(first.start_offset, 4);
    let third = capture.replay_before(second.start_offset, 64);

    // Then the chunks tile the buffer back-to-back and the final one reaches the oldest byte
    first
        .assert_data_is(b"6789")
        .assert_offsets(6, 10)
        .assert_not_at_oldest();
    second
        .assert_data_is(b"2345")
        .assert_offsets(2, 6)
        .assert_not_at_oldest();
    third
        .assert_data_is(b"01")
        .assert_offsets(0, 2)
        .assert_at_oldest();
}
