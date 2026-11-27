//! `key_for_keysym`: what each X11 keysym the viewer sends means to the simulator.

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_ios_emulator::{key_for_keysym, KeyStroke, SpecialKey};

#[rstest]
#[case::lowercase_letter(0x61, KeyStroke::Text('a'))]
#[case::uppercase_letter(0x5a, KeyStroke::Text('Z'))]
#[case::digit(0x37, KeyStroke::Text('7'))]
#[case::space(0x20, KeyStroke::Text(' '))]
#[case::at_sign(0x40, KeyStroke::Text('@'))]
#[case::return_key(0xff0d, KeyStroke::Special(SpecialKey::Return))]
#[case::backspace(0xff08, KeyStroke::Special(SpecialKey::Backspace))]
#[case::escape(0xff1b, KeyStroke::Special(SpecialKey::Escape))]
#[case::tab(0xff09, KeyStroke::Special(SpecialKey::Tab))]
#[case::left_arrow(0xff51, KeyStroke::Special(SpecialKey::Left))]
#[case::up_arrow(0xff52, KeyStroke::Special(SpecialKey::Up))]
#[case::right_arrow(0xff53, KeyStroke::Special(SpecialKey::Right))]
#[case::down_arrow(0xff54, KeyStroke::Special(SpecialKey::Down))]
fn a_keysym_maps_to_its_keystroke(#[case] keysym: u32, #[case] expected: KeyStroke) {
    // When
    let stroke = key_for_keysym(keysym);

    // Then
    assert_eq!(stroke, Some(expected));
}

#[rstest]
#[case::left_shift(0xffe1)]
#[case::left_control(0xffe3)]
#[case::f1(0xffbe)]
fn a_modifier_or_function_key_maps_to_nothing(#[case] keysym: u32) {
    // When
    let stroke = key_for_keysym(keysym);

    // Then
    assert_eq!(stroke, None);
}
