//! How a scroll becomes RDP wheel rotation.

use ironrdp_pdu::input::mouse::PointerFlags;
use pretty_assertions::assert_eq;
use tddy_rdp::rdp_client::wheel_mouse_pdus;

#[test]
fn a_scroll_down_turns_the_vertical_wheel_one_notch_towards_the_user() {
    // When
    let pdus = wheel_mouse_pdus(10, 20, 0, 40);

    // Then
    assert_eq!(pdus.len(), 1);
    assert_eq!(pdus[0].flags, PointerFlags::VERTICAL_WHEEL);
    assert_eq!(pdus[0].number_of_wheel_rotation_units, -120);
    assert_eq!((pdus[0].x_position, pdus[0].y_position), (10, 20));
}

#[test]
fn a_scroll_right_turns_the_horizontal_wheel_one_notch() {
    // When
    let pdus = wheel_mouse_pdus(0, 0, 7, 0);

    // Then
    assert_eq!(pdus.len(), 1);
    assert_eq!(pdus[0].flags, PointerFlags::HORIZONTAL_WHEEL);
    assert_eq!(pdus[0].number_of_wheel_rotation_units, 120);
}

#[test]
fn no_scroll_turns_no_wheel() {
    // When / Then
    assert!(wheel_mouse_pdus(0, 0, 0, 0).is_empty());
}
