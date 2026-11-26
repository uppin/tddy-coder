//! The DTUHID wire model: what the simulator's `dtuhidd` daemon is sent, independent of how it is
//! sent.
//!
//! From Xcode 27 the simulator drops Indigo keyboard events and delivers Indigo touch unreliably;
//! input goes instead to `dtuhidd` inside the guest, as XPC dictionaries on the service
//! [`DIGITIZER_SERVICE`]. This module builds those dictionaries as plain values
//! ([`WireValue`]), so their exact keys and types are testable without XPC; the macOS adapter
//! (`crate::dtuhid_xpc`) turns them into `xpc_object_t`s and sends them.
//!
//! Ported from idb (<https://github.com/meta/idb>), MIT licence, Copyright (c) Meta Platforms, Inc.
//! and affiliates — see `THIRD_PARTY_NOTICES` in this crate. The message shapes, enum raw values and
//! the contact phase rule follow `FBSimulatorControl/HID/DTUHIDModels.swift`,
//! `SimulatorDTUHIDConnection.swift` (the liveness barrier), `SimulatorDigitizerHIDTransport.swift`
//! (`DigitizerContactTracker`), `SimulatorHIDButtonIdentity.swift` (the home button usage) and
//! `SimulatorIndigoHID.swift` (`screenRatio`); the XPC type of each value follows
//! `FBSimulatorControl/XPC/XPCEncoder.swift` (unsigned → `uint64`, `Double` → `double`).

use crate::geometry::{Rect, ScreenPoint};
use crate::input::SpecialKey;

/// The `dtuhidd` service that takes touch, keyboard and hardware-button events.
pub const DIGITIZER_SERVICE: &str = "com.apple.coredevice.feature.remote.hid.digitizer";

/// One XPC value, as `dtuhidd` decodes it.
#[derive(Debug, Clone, PartialEq)]
pub enum WireValue {
    String(String),
    Bool(bool),
    Double(f64),
    Uint64(u64),
    /// A dictionary, its keys in the order they are written.
    Dictionary(Vec<(&'static str, WireValue)>),
}

impl WireValue {
    /// The value under `key`, when this is a dictionary that has one.
    pub fn get(&self, key: &str) -> Option<&WireValue> {
        match self {
            WireValue::Dictionary(entries) => entries
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

/// The per-contact phase of an `IndigoDigitizerEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactPhase {
    Start = 0,
    Position = 1,
    End = 2,
}

/// `dtuhidd`'s 1-based `HIDButtonState`: `0` is rejected at decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Down = 1,
    Up = 2,
}

/// The envelope every `dtuhidd` message shares.
pub fn envelope(message_type: &str, is_barrier: bool, payload: WireValue) -> WireValue {
    WireValue::Dictionary(vec![
        ("messageType", WireValue::String(message_type.to_owned())),
        ("isBarrier", WireValue::Bool(is_barrier)),
        (
            "featureIdentifier",
            WireValue::String(DIGITIZER_SERVICE.to_owned()),
        ),
        ("payload", payload),
    ])
}

/// One touch contact at `(x, y)`, normalised to `0..=1` from the screen's top-left.
pub fn touch(x: f64, y: f64, phase: ContactPhase) -> WireValue {
    envelope(
        "IndigoDigitizerEvent",
        false,
        WireValue::Dictionary(vec![
            (
                "pointOne",
                WireValue::Dictionary(vec![
                    ("x", WireValue::Double(x)),
                    ("y", WireValue::Double(y)),
                ]),
            ),
            ("eventType", WireValue::Uint64(phase as u64)),
            ("edge", WireValue::Uint64(0)),
            ("target", WireValue::Uint64(0)),
        ]),
    )
}

/// A USB HID keyboard usage (page 0x07) pressed or released.
pub fn keyboard(usage: u16, state: ButtonState) -> WireValue {
    envelope(
        "IndigoKeyboardButtonEvent",
        false,
        keyboard_payload(usage, state),
    )
}

/// The liveness probe: a barrier carrying usage `0` ("no event"), which `dtuhidd` answers without
/// the guest seeing a key.
pub fn liveness_barrier() -> WireValue {
    envelope(
        "IndigoKeyboardButtonEvent",
        true,
        keyboard_payload(0, ButtonState::Up),
    )
}

/// The Home button (consumer page 0x0C, usage 0x40), pressed or released.
pub fn home_button(state: ButtonState) -> WireValue {
    envelope(
        "IndigoButtonEvent",
        false,
        WireValue::Dictionary(vec![
            ("usagePage", WireValue::Uint64(0x0C)),
            ("usageCode", WireValue::Uint64(0x40)),
            ("state", WireValue::Uint64(state as u64)),
        ]),
    )
}

fn keyboard_payload(usage: u16, state: ButtonState) -> WireValue {
    WireValue::Dictionary(vec![
        ("usageCode", WireValue::Uint64(u64::from(usage))),
        ("state", WireValue::Uint64(state as u64)),
    ])
}

/// Where `point` (screen points) lies on the device screen `device_area`, normalised to `0..=1` and
/// clamped to it — `dtuhidd`'s coordinate space, independent of where the window is.
pub fn screen_ratio(point: ScreenPoint, device_area: Rect) -> (f64, f64) {
    (
        ((point.x - device_area.x) / device_area.width).clamp(0.0, 1.0),
        ((point.y - device_area.y) / device_area.height).clamp(0.0, 1.0),
    )
}

/// The phase each event of one contact is sent with: the first down starts it, later downs move it,
/// and the up ends it (idb's `DigitizerContactTracker`).
#[derive(Debug, Default)]
pub struct ContactTracker {
    down: bool,
}

impl ContactTracker {
    /// The contact is down (or still down) at a new position.
    pub fn down(&mut self) -> ContactPhase {
        if std::mem::replace(&mut self.down, true) {
            ContactPhase::Position
        } else {
            ContactPhase::Start
        }
    }

    /// The contact lifted.
    pub fn up(&mut self) -> ContactPhase {
        self.down = false;
        ContactPhase::End
    }
}

/// Left shift, held around a character that needs it.
pub const LEFT_SHIFT: u16 = 0xE1;

/// The HID keyboard usage that types `c` on a US layout, and whether shift must be held for it.
pub fn usage_for_char(c: char) -> Option<(u16, bool)> {
    let plain = |usage: u16| Some((usage, false));
    let shifted = |usage: u16| Some((usage, true));
    match c {
        'a'..='z' => plain(0x04 + (c as u16 - 'a' as u16)),
        'A'..='Z' => shifted(0x04 + (c as u16 - 'A' as u16)),
        '1'..='9' => plain(0x1E + (c as u16 - '1' as u16)),
        '0' => plain(0x27),
        ' ' => plain(0x2C),
        '-' => plain(0x2D),
        '=' => plain(0x2E),
        '[' => plain(0x2F),
        ']' => plain(0x30),
        '\\' => plain(0x31),
        ';' => plain(0x33),
        '\'' => plain(0x34),
        '`' => plain(0x35),
        ',' => plain(0x36),
        '.' => plain(0x37),
        '/' => plain(0x38),
        '!' => shifted(0x1E),
        '@' => shifted(0x1F),
        '#' => shifted(0x20),
        '$' => shifted(0x21),
        '%' => shifted(0x22),
        '^' => shifted(0x23),
        '&' => shifted(0x24),
        '*' => shifted(0x25),
        '(' => shifted(0x26),
        ')' => shifted(0x27),
        '_' => shifted(0x2D),
        '+' => shifted(0x2E),
        '{' => shifted(0x2F),
        '}' => shifted(0x30),
        '|' => shifted(0x31),
        ':' => shifted(0x33),
        '"' => shifted(0x34),
        '~' => shifted(0x35),
        '<' => shifted(0x36),
        '>' => shifted(0x37),
        '?' => shifted(0x38),
        _ => None,
    }
}

/// The HID keyboard usage of a special key.
pub fn usage_for_special(key: SpecialKey) -> u16 {
    match key {
        SpecialKey::Return => 0x28,
        SpecialKey::Escape => 0x29,
        SpecialKey::Backspace => 0x2A,
        SpecialKey::Tab => 0x2B,
        SpecialKey::Right => 0x4F,
        SpecialKey::Left => 0x50,
        SpecialKey::Down => 0x51,
        SpecialKey::Up => 0x52,
    }
}

/// The keyboard messages that type `c`: its usage pressed and released, inside a held shift when it
/// needs one. Empty for a character with no usage on a US layout.
pub fn keystrokes_for_char(c: char) -> Vec<WireValue> {
    let Some((usage, shift)) = usage_for_char(c) else {
        return Vec::new();
    };
    let mut messages = Vec::with_capacity(4);
    if shift {
        messages.push(keyboard(LEFT_SHIFT, ButtonState::Down));
    }
    messages.push(keyboard(usage, ButtonState::Down));
    messages.push(keyboard(usage, ButtonState::Up));
    if shift {
        messages.push(keyboard(LEFT_SHIFT, ButtonState::Up));
    }
    messages
}
