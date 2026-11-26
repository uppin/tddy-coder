//! The messages the simulator's `dtuhidd` is sent: their envelope, keys and value types, the touch
//! coordinate space, the contact phases, and the keyboard usages that type each character.

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_ios_emulator::dtuhid::{
    keyboard, keystrokes_for_char, liveness_barrier, screen_ratio, touch, usage_for_char,
    usage_for_special, ButtonState, ContactPhase, ContactTracker, WireValue, DIGITIZER_SERVICE,
    LEFT_SHIFT,
};
use tddy_ios_emulator::{Rect, ScreenPoint, SpecialKey};

fn string(value: &str) -> WireValue {
    WireValue::String(value.into())
}

#[test]
fn a_touch_start_is_a_digitizer_event_in_the_shared_envelope() {
    // When
    let message = touch(0.25, 0.75, ContactPhase::Start);

    // Then
    assert_eq!(
        message,
        WireValue::Dictionary(vec![
            ("messageType", string("IndigoDigitizerEvent")),
            ("isBarrier", WireValue::Bool(false)),
            ("featureIdentifier", string(DIGITIZER_SERVICE)),
            (
                "payload",
                WireValue::Dictionary(vec![
                    (
                        "pointOne",
                        WireValue::Dictionary(vec![
                            ("x", WireValue::Double(0.25)),
                            ("y", WireValue::Double(0.75)),
                        ]),
                    ),
                    ("eventType", WireValue::Uint64(0)),
                    ("edge", WireValue::Uint64(0)),
                    ("target", WireValue::Uint64(0)),
                ]),
            ),
        ])
    );
}

#[test]
fn a_key_down_carries_its_usage_and_a_one_based_state() {
    // When
    let message = keyboard(0x04, ButtonState::Down);

    // Then
    assert_eq!(
        message.get("messageType"),
        Some(&string("IndigoKeyboardButtonEvent"))
    );
    assert_eq!(
        message.get("payload"),
        Some(&WireValue::Dictionary(vec![
            ("usageCode", WireValue::Uint64(0x04)),
            ("state", WireValue::Uint64(1)),
        ]))
    );
}

#[test]
fn the_liveness_probe_is_an_inert_keyboard_barrier() {
    // When
    let message = liveness_barrier();

    // Then
    assert_eq!(message.get("isBarrier"), Some(&WireValue::Bool(true)));
    assert_eq!(
        message
            .get("payload")
            .and_then(|payload| payload.get("usageCode")),
        Some(&WireValue::Uint64(0))
    );
}

#[test]
fn a_point_is_sent_as_its_place_on_the_device_screen() {
    // Given: the device area of a Device Hub window
    let device_area = Rect {
        x: 862.0,
        y: 256.0,
        width: 297.0,
        height: 646.0,
    };

    // When
    let corners = [
        screen_ratio(ScreenPoint { x: 862.0, y: 256.0 }, device_area),
        screen_ratio(
            ScreenPoint {
                x: 1010.5,
                y: 579.0,
            },
            device_area,
        ),
        screen_ratio(ScreenPoint { x: 2000.0, y: 0.0 }, device_area),
    ];

    // Then: origin, centre, and a point off the screen clamped onto its edge
    assert_eq!(corners, [(0.0, 0.0), (0.5, 0.5), (1.0, 0.0)]);
}

#[test]
fn a_drag_starts_moves_and_ends_one_contact_and_the_next_starts_afresh() {
    // Given
    let mut contact = ContactTracker::default();

    // When
    let phases = [
        contact.down(),
        contact.down(),
        contact.down(),
        contact.up(),
        contact.down(),
    ];

    // Then
    assert_eq!(
        phases,
        [
            ContactPhase::Start,
            ContactPhase::Position,
            ContactPhase::Position,
            ContactPhase::End,
            ContactPhase::Start,
        ]
    );
}

#[rstest]
#[case::lowercase('a', (0x04, false))]
#[case::last_letter('z', (0x1D, false))]
#[case::uppercase('Q', (0x14, true))]
#[case::one('1', (0x1E, false))]
#[case::zero('0', (0x27, false))]
#[case::space(' ', (0x2C, false))]
#[case::at_sign('@', (0x1F, true))]
#[case::question_mark('?', (0x38, true))]
#[case::full_stop('.', (0x37, false))]
fn a_character_types_with_its_us_keyboard_usage(#[case] c: char, #[case] expected: (u16, bool)) {
    // When
    let usage = usage_for_char(c);

    // Then
    assert_eq!(usage, Some(expected));
}

#[test]
fn a_shifted_character_is_typed_inside_a_held_shift() {
    // When
    let strokes = keystrokes_for_char('A');

    // Then
    assert_eq!(
        strokes,
        vec![
            keyboard(LEFT_SHIFT, ButtonState::Down),
            keyboard(0x04, ButtonState::Down),
            keyboard(0x04, ButtonState::Up),
            keyboard(LEFT_SHIFT, ButtonState::Up),
        ]
    );
}

#[test]
fn a_character_with_no_us_usage_types_nothing() {
    // When
    let strokes = keystrokes_for_char('é');

    // Then
    assert_eq!(strokes, vec![]);
}

#[rstest]
#[case::return_key(SpecialKey::Return, 0x28)]
#[case::escape(SpecialKey::Escape, 0x29)]
#[case::backspace(SpecialKey::Backspace, 0x2A)]
#[case::tab(SpecialKey::Tab, 0x2B)]
#[case::right(SpecialKey::Right, 0x4F)]
#[case::left(SpecialKey::Left, 0x50)]
#[case::down(SpecialKey::Down, 0x51)]
#[case::up(SpecialKey::Up, 0x52)]
fn a_special_key_has_its_hid_usage(#[case] key: SpecialKey, #[case] usage: u16) {
    // When / Then
    assert_eq!(usage_for_special(key), usage);
}
