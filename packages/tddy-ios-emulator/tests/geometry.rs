//! `DeviceViewport`: cropping a Simulator.app window to the device and mapping pixels to screen.

use pretty_assertions::assert_eq;
use tddy_ios_emulator::{DeviceViewport, Rect, ScreenPoint};

/// An iPhone window at (100, 200), 400×868 points with a 28-point title bar, on a Retina display.
fn an_iphone_window_viewport() -> DeviceViewport {
    DeviceViewport::within_window(
        Rect {
            x: 100.0,
            y: 200.0,
            width: 400.0,
            height: 868.0,
        },
        28.0,
        2.0,
    )
}

#[test]
fn the_device_area_is_the_window_below_its_title_bar() {
    // Given
    let viewport = an_iphone_window_viewport();

    // When
    let area = viewport.device_area();

    // Then
    assert_eq!(
        area,
        Rect {
            x: 100.0,
            y: 228.0,
            width: 400.0,
            height: 840.0,
        }
    );
}

#[test]
fn the_streamed_frame_is_the_device_area_in_pixels() {
    // Given
    let viewport = an_iphone_window_viewport();

    // When
    let size = viewport.pixel_size();

    // Then
    assert_eq!(size, (800, 1680));
}

#[test]
fn the_title_bar_covers_the_top_rows_of_a_captured_window_frame() {
    // Given
    let viewport = an_iphone_window_viewport();

    // When
    let rows = viewport.title_bar_rows(1736);

    // Then
    assert_eq!(rows, 56);
}

#[test]
fn device_corners_map_to_the_corners_of_the_device_area_on_screen() {
    // Given
    let viewport = an_iphone_window_viewport();

    // When
    let corners = [viewport.to_screen(0, 0), viewport.to_screen(799, 1679)];

    // Then
    assert_eq!(
        corners,
        [
            ScreenPoint { x: 100.0, y: 228.0 },
            ScreenPoint { x: 499.5, y: 1067.5 },
        ]
    );
}
