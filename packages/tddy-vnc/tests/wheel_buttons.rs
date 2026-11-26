//! How a scroll becomes RFB wheel-button clicks.

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_vnc::vnc_client::wheel_button_masks;

#[rstest]
#[case::down(0, 40, vec![1 << 4])]
#[case::up(0, -3, vec![1 << 3])]
#[case::right(12, 0, vec![1 << 6])]
#[case::left(-1, 0, vec![1 << 5])]
#[case::diagonal(5, -5, vec![1 << 3, 1 << 6])]
#[case::nothing(0, 0, vec![])]
fn a_scroll_clicks_the_wheel_button_of_each_axis_it_moved(
    #[case] delta_x: i32,
    #[case] delta_y: i32,
    #[case] expected: Vec<u8>,
) {
    // When
    let masks = wheel_button_masks(delta_x, delta_y);

    // Then
    assert_eq!(masks, expected);
}
