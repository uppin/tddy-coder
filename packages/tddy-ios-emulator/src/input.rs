//! The input port: synthetic mouse and keyboard events posted at screen coordinates.

use crate::geometry::ScreenPoint;

/// One event posted to the Simulator window.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// Pointer moved with no button held.
    MouseMove(ScreenPoint),
    /// Left button pressed: a touch-down on the device.
    MouseDown(ScreenPoint),
    /// Pointer moved with the left button held: a touch move.
    MouseDrag(ScreenPoint),
    /// Left button released: a touch-up.
    MouseUp(ScreenPoint),
    /// Printable text typed on key-down.
    Text(String),
    /// A non-printable key, pressed or released.
    Key { key: SpecialKey, pressed: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialKey {
    Return,
    Backspace,
    Escape,
    Tab,
    Left,
    Right,
    Up,
    Down,
}

/// What an X11 keysym means to the simulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStroke {
    Text(char),
    Special(SpecialKey),
}

/// Map an X11 keysym (what the viewer sends) to a keystroke; `None` for keys the simulator ignores.
pub fn key_for_keysym(keysym: u32) -> Option<KeyStroke> {
    let _ = keysym;
    todo!("key_for_keysym")
}

pub trait InputSink: Send + Sync {
    fn post(&self, event: InputEvent) -> anyhow::Result<()>;
}
