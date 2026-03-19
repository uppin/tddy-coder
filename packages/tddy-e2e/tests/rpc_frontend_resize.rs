//! Reproduces bug: RPC frontend (rpc_demo, web terminal) does not send resize to VirtualTui.
//!
//! When the terminal is resized over RPC, the client must send the resize sequence
//! \x1b]resize;cols;rows\x07 so the VirtualTui can resize and redraw correctly.
//! Without this, the display shows artifacts and wrong dimensions.

use crossterm::event::Event;
use tddy_e2e::rpc_frontend::{encode_resize, event_to_bytes};

#[test]
fn encode_resize_produces_correct_osc_sequence() {
    let bytes = encode_resize(120, 30);
    let expected = b"\x1b]resize;120;30\x07";
    assert_eq!(
        bytes, expected,
        "encode_resize must produce OSC resize sequence"
    );
}

#[test]
fn event_to_bytes_returns_resize_sequence_for_resize_event() {
    let event = Event::Resize(120, 30);
    let bytes = event_to_bytes(&event);
    let expected = encode_resize(120, 30);
    assert!(
        bytes.as_ref() == Some(&expected),
        "event_to_bytes(Event::Resize) must return resize sequence so RPC client sends it to VirtualTui; got {:?}",
        bytes
    );
}
