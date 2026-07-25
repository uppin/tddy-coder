//! Acceptance tests: a late terminal subscriber must be able to restore the mouse-tracking
//! modes the application enabled at startup, even after the bounded capture ring has trimmed
//! past the bytes that enabled them.
//!
//! Why this matters: the web terminal forwards mouse events only while its own VT instance has
//! seen a mouse-tracking DECSET (`GhosttyTerminal` gates every handler on `hasMouseTracking()`).
//! A browser attaching to a long-running PTY never sees those bytes once they have been evicted
//! from the capture ring, so clicks, drags and scrolls are silently dropped for every session
//! except the freshest one.

use bytes::Bytes;
use tddy_task::{ChannelKind, TaskChannel, TerminalCapture};

// ---------------------------------------------------------------------------
// Domain vocabulary
// ---------------------------------------------------------------------------

/// DECSET 1000 — report button press/release only.
const NORMAL_MOUSE_TRACKING: u16 = 1000;
/// DECSET 1002 — report button press/release plus drag motion.
const BUTTON_EVENT_TRACKING: u16 = 1002;
/// DECSET 1006 — encode mouse reports in SGR form (`ESC[<b;x;yM`).
const SGR_MOUSE_ENCODING: u16 = 1006;
/// DECSET 25 — cursor visibility; sticky terminal state, but not mouse tracking.
const CURSOR_VISIBILITY: u16 = 25;
/// DECSET 1049 — alternate screen buffer; sticky terminal state, but not mouse tracking.
const ALTERNATE_SCREEN: u16 = 1049;

/// Byte used for bulk output that carries no escape sequences.
const FILLER_BYTE: u8 = b'A';

/// The bytes a terminal application writes to turn a private mode on.
fn mode_on(mode: u16) -> Vec<u8> {
    format!("\x1b[?{mode}h").into_bytes()
}

/// The bytes a terminal application writes to turn a private mode off.
fn mode_off(mode: u16) -> Vec<u8> {
    format!("\x1b[?{mode}l").into_bytes()
}

/// The prologue a late subscriber must receive for `modes` to be in effect on its own VT.
fn prologue_enabling(modes: &[u16]) -> Vec<u8> {
    modes.iter().flat_map(|mode| mode_on(*mode)).collect()
}

/// Render terminal bytes so assertion failures are readable (`ESC[?1006h` instead of `\x1b[...`).
fn readable(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| match byte {
            0x1b => "ESC".to_string(),
            b if b.is_ascii_graphic() || *b == b' ' => (*b as char).to_string(),
            b => format!("\\x{b:02x}"),
        })
        .collect()
}

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
    fn with_mode_enabled(mut self, mode: u16) -> Self {
        self.writes.push(mode_on(mode));
        self
    }

    fn with_mode_disabled(mut self, mode: u16) -> Self {
        self.writes.push(mode_off(mode));
        self
    }

    fn with_output(mut self, text: &str) -> Self {
        self.writes.push(text.as_bytes().to_vec());
        self
    }

    fn with_raw_output(mut self, bytes: &[u8]) -> Self {
        self.writes.push(bytes.to_vec());
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

trait TerminalBytesAssertions {
    fn assert_begins_with(&self, expected: &[u8]) -> &Self;
    fn assert_is(&self, expected: &[u8]) -> &Self;
    fn assert_is_entirely(&self, expected: u8) -> &Self;
    fn assert_byte_len(&self, expected: usize) -> &Self;
}

impl TerminalBytesAssertions for Vec<u8> {
    fn assert_begins_with(&self, expected: &[u8]) -> &Self {
        let head = &self[..expected.len().min(self.len())];
        assert_eq!(
            readable(head),
            readable(expected),
            "terminal bytes must begin with '{}'",
            readable(expected)
        );
        self
    }

    fn assert_is(&self, expected: &[u8]) -> &Self {
        assert_eq!(
            readable(self),
            readable(expected),
            "terminal bytes mismatch"
        );
        self
    }

    fn assert_is_entirely(&self, expected: u8) -> &Self {
        let stray = self.iter().enumerate().find(|(_, byte)| **byte != expected);
        assert_eq!(
            stray.map(|(offset, byte)| format!("'{}' at offset {offset}", readable(&[*byte]))),
            None,
            "terminal bytes must be nothing but '{}'",
            readable(&[expected])
        );
        self
    }

    fn assert_byte_len(&self, expected: usize) -> &Self {
        assert_eq!(self.len(), expected, "terminal byte count mismatch");
        self
    }
}

// ---------------------------------------------------------------------------
// TerminalCapture: sticky mouse-mode state survives ring eviction
// ---------------------------------------------------------------------------

#[test]
fn replays_the_mouse_tracking_modes_after_the_ring_has_trimmed_past_them() {
    // Given an application that enabled SGR mouse tracking at startup, then produced far more
    // output than the ring retains
    let capture = a_terminal_capture()
        .with_mode_enabled(BUTTON_EVENT_TRACKING)
        .with_mode_enabled(SGR_MOUSE_ENCODING)
        .with_output_far_exceeding_the_capture_limit()
        .build();

    // When a late subscriber takes the replay
    let replay = capture.replay();

    // Then it leads with the modes that are still in effect
    replay.assert_begins_with(&prologue_enabling(&[
        BUTTON_EVENT_TRACKING,
        SGR_MOUSE_ENCODING,
    ]));
}

#[test]
fn omits_a_mouse_mode_the_application_turned_back_off() {
    // Given an application that dropped drag motion reporting again
    let capture = a_terminal_capture()
        .with_mode_enabled(NORMAL_MOUSE_TRACKING)
        .with_mode_enabled(BUTTON_EVENT_TRACKING)
        .with_mode_enabled(SGR_MOUSE_ENCODING)
        .with_mode_disabled(BUTTON_EVENT_TRACKING)
        .build();

    // When the prologue is computed
    let prologue = capture.mode_prologue();

    // Then only the modes still on are re-enabled
    prologue.assert_is(&prologue_enabling(&[
        NORMAL_MOUSE_TRACKING,
        SGR_MOUSE_ENCODING,
    ]));
}

#[test]
fn enables_a_mouse_mode_once_when_the_application_re_emits_it() {
    // Given an application that set the same mode on every redraw
    let capture = a_terminal_capture()
        .with_mode_enabled(SGR_MOUSE_ENCODING)
        .with_output("drawing a frame")
        .with_mode_enabled(SGR_MOUSE_ENCODING)
        .build();

    // When the prologue is computed
    let prologue = capture.mode_prologue();

    // Then the mode appears exactly once
    prologue.assert_is(&prologue_enabling(&[SGR_MOUSE_ENCODING]));
}

#[test]
fn orders_the_prologue_by_mode_number_whatever_order_the_application_used() {
    // Given an application that enabled the modes highest-number first
    let capture = a_terminal_capture()
        .with_mode_enabled(SGR_MOUSE_ENCODING)
        .with_mode_enabled(BUTTON_EVENT_TRACKING)
        .with_mode_enabled(NORMAL_MOUSE_TRACKING)
        .build();

    // When the prologue is computed
    let prologue = capture.mode_prologue();

    // Then it is emitted in ascending mode order
    prologue.assert_is(&prologue_enabling(&[
        NORMAL_MOUSE_TRACKING,
        BUTTON_EVENT_TRACKING,
        SGR_MOUSE_ENCODING,
    ]));
}

#[test]
fn replays_the_output_unchanged_when_the_application_set_no_mouse_modes() {
    // Given a plain command that never touched mouse tracking
    let capture = a_terminal_capture().with_output("total 0\r\n").build();

    // When a late subscriber takes the replay
    let replay = capture.replay();

    // Then it is exactly the captured output, with no prologue in front of it
    replay.assert_is(b"total 0\r\n");
}

#[test]
fn ignores_private_modes_outside_the_mouse_tracking_family() {
    // Given an application that hid the cursor and entered the alternate screen
    let capture = a_terminal_capture()
        .with_mode_disabled(CURSOR_VISIBILITY)
        .with_mode_enabled(ALTERNATE_SCREEN)
        .with_mode_enabled(SGR_MOUSE_ENCODING)
        .build();

    // When the prologue is computed
    let prologue = capture.mode_prologue();

    // Then it carries only the mouse-tracking mode
    prologue.assert_is(&prologue_enabling(&[SGR_MOUSE_ENCODING]));
}

#[test]
fn keeps_the_buffered_output_at_the_capture_limit_when_output_far_exceeds_it() {
    // Given an application that produced three times the ring capacity
    let capture = a_terminal_capture()
        .with_mode_enabled(SGR_MOUSE_ENCODING)
        .with_output_far_exceeding_the_capture_limit()
        .build();

    // When the buffered size is measured
    let buffered = capture.buffered_bytes().len();

    // Then the ring holds exactly its limit — sticky mode state costs no ring space
    assert_eq!(
        buffered,
        TerminalCapture::CAPTURE_LIMIT_BYTES,
        "ring must stay bounded at its limit"
    );
}

#[test]
fn drops_the_escape_sequence_that_trimming_cut_in_half() {
    // Given output whose leading colour sequence is straddled by the trim point: the 5-byte
    // `ESC[31m` plus `limit - 2` filler bytes overflows the ring by exactly 3 bytes, so a naive
    // byte-count trim would leave the orphan fragment `1m` at the head of the replay.
    let capture = a_terminal_capture()
        .with_raw_output(b"\x1b[31m")
        .with_filler_bytes(TerminalCapture::CAPTURE_LIMIT_BYTES - 2)
        .build();

    // When a late subscriber takes the replay
    let replay = capture.replay();

    // Then the whole cut sequence is gone — no orphan `1m` fragment survives at the head
    replay
        .assert_is_entirely(FILLER_BYTE)
        .assert_byte_len(TerminalCapture::CAPTURE_LIMIT_BYTES - 2);
}

// ---------------------------------------------------------------------------
// TaskChannel: the replay every late subscriber actually reads
// ---------------------------------------------------------------------------

#[test]
fn a_task_channel_still_knows_the_mouse_modes_after_its_ring_has_trimmed_past_them() {
    // Given a PTY channel whose application enabled mouse tracking, then flooded the ring
    let channel = TaskChannel::output_only("0", "pty", ChannelKind::Pty);
    channel.write(Bytes::from(prologue_enabling(&[
        BUTTON_EVENT_TRACKING,
        SGR_MOUSE_ENCODING,
    ])));
    channel.write(Bytes::from(vec![
        FILLER_BYTE;
        3 * TerminalCapture::CAPTURE_LIMIT_BYTES
    ]));

    // When a terminal attaches and takes the capture's replay
    let replay = channel.capture_arc().lock().unwrap().replay();

    // Then it leads with the modes that are still in effect
    replay.assert_begins_with(&prologue_enabling(&[
        BUTTON_EVENT_TRACKING,
        SGR_MOUSE_ENCODING,
    ]));
}

#[test]
fn a_task_channel_replay_carries_only_what_the_process_wrote() {
    // Given a channel capturing a command that happened to enable mouse tracking
    let channel = TaskChannel::output_only("stdout", "stdout", ChannelKind::Stdout);
    channel.write(Bytes::from(prologue_enabling(&[SGR_MOUSE_ENCODING])));
    channel.write(Bytes::from_static(b"done\r\n"));

    // When a consumer collecting the command's output takes the channel replay
    let replay = channel.replay_capture();

    // Then it is the captured bytes alone — no mode prologue is prepended in front of them
    replay.assert_is(
        &[
            prologue_enabling(&[SGR_MOUSE_ENCODING]),
            b"done\r\n".to_vec(),
        ]
        .concat(),
    );
}
