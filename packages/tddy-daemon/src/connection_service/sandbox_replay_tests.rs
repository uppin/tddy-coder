use super::{sandbox_replay_frames, TERMINAL_OUTPUT_FRAME_MAX_BYTES};
use tddy_task::TerminalCapture;

/// DECSET 1006 — SGR mouse encoding, the mode `GhosttyTerminal` gates mouse forwarding on.
const SGR_MOUSE_ENCODING: &[u8] = b"\x1b[?1006h";

/// A sandbox session that turned on mouse reporting at startup and has since produced far
/// more output than the capture ring retains.
fn a_long_running_sandbox_capture() -> TerminalCapture {
    let mut capture = TerminalCapture::new();
    capture.append(SGR_MOUSE_ENCODING);
    capture.append(&vec![b'A'; 3 * TerminalCapture::CAPTURE_LIMIT_BYTES]);
    capture
}

#[test]
fn leads_the_sandbox_replay_with_the_mouse_modes_still_in_effect() {
    // Given a sandbox session whose ring has long since evicted the enabling DECSET
    let capture = a_long_running_sandbox_capture();

    // When a browser attaches and takes the replay frames
    let frames = sandbox_replay_frames(&capture, TERMINAL_OUTPUT_FRAME_MAX_BYTES);

    // Then the first frame re-enables mouse reporting, so the attaching terminal forwards
    // clicks and scrolls instead of silently dropping them
    assert_eq!(
        frames.first().map(|frame| frame
            .iter()
            .copied()
            .take(SGR_MOUSE_ENCODING.len())
            .collect::<Vec<u8>>()),
        Some(SGR_MOUSE_ENCODING.to_vec()),
    );
}
