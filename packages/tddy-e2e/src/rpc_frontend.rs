//! RPC frontend encoding: crossterm events → bytes for VirtualTui.
//!
//! Used by rpc_demo and tests. VirtualTui expects:
//! - Keys: raw bytes (Enter=\r, Up=ESC[A, etc.)
//! - Mouse: SGR format ESC[<pb;px;py M/m
//! - Resize: OSC format ESC]resize;cols;rows BEL

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

/// Encode a crossterm resize event into OSC bytes for VirtualTui.
/// Format: \x1b]resize;{cols};{rows}\x07
pub fn encode_resize(cols: u16, rows: u16) -> Vec<u8> {
    format!("\x1b]resize;{cols};{rows}\x07").into_bytes()
}

/// Convert a crossterm event to bytes for VirtualTui input stream.
/// Returns None for events that should not be sent (e.g. FocusGained/Lost).
pub fn event_to_bytes(event: &Event) -> Option<Vec<u8>> {
    match event {
        Event::Resize(cols, rows) => Some(encode_resize(*cols, *rows)),
        Event::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return None;
            }
            let bytes = match key.code {
                KeyCode::Enter => vec![b'\r'],
                KeyCode::Up => vec![0x1b, b'[', b'A'],
                KeyCode::Down => vec![0x1b, b'[', b'B'],
                KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
                KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
                KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    vec![c as u8 & 0x1f]
                }
                KeyCode::Char(c) => {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    s.as_bytes().to_vec()
                }
                KeyCode::Backspace => vec![0x7f],
                KeyCode::Esc => vec![0x1b],
                KeyCode::Tab => vec![b'\t'],
                _ => return None,
            };
            if bytes.is_empty() {
                None
            } else {
                Some(bytes)
            }
        }
        Event::Mouse(ev) => {
            use crossterm::event::{MouseButton, MouseEventKind};
            let (pb, release) = match ev.kind {
                MouseEventKind::Down(MouseButton::Left) => (0u8, false),
                MouseEventKind::Up(MouseButton::Left) => (0u8, true),
                MouseEventKind::ScrollUp => (64, false),
                MouseEventKind::ScrollDown => (65, false),
                _ => return None,
            };
            let px = ev.column.saturating_add(1);
            let py = ev.row.saturating_add(1);
            let end = if release { b'm' } else { b'M' };
            Some(format!("\x1b[<{pb};{px};{py}{}", end as char).into_bytes())
        }
        _ => None,
    }
}
