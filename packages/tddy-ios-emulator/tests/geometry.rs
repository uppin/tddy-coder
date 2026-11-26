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
    let offset = viewport.crop_offset();

    // Then
    assert_eq!(offset, (0, 56));
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
            ScreenPoint {
                x: 499.5,
                y: 1067.5
            },
        ]
    );
}

/// A Device Hub window (Xcode 27) at (337, 175), 1100×800 points, whose device screen is shown
/// right of a sidebar and below a toolbar, on a Retina display.
fn a_device_hub_viewport() -> DeviceViewport {
    DeviceViewport::new(
        Rect {
            x: 337.0,
            y: 175.0,
            width: 1100.0,
            height: 800.0,
        },
        Rect {
            x: 862.0,
            y: 256.0,
            width: 297.0,
            height: 646.0,
        },
        2.0,
    )
}

#[test]
fn a_device_hub_window_streams_only_its_device_area() {
    // Given
    let viewport = a_device_hub_viewport();

    // When
    let size = viewport.pixel_size();

    // Then
    assert_eq!(size, (594, 1292));
}

#[test]
fn a_device_hub_window_is_cropped_at_its_device_area() {
    // Given
    let viewport = a_device_hub_viewport();

    // When
    let offset = viewport.crop_offset();

    // Then
    assert_eq!(offset, (1050, 162));
}

#[test]
fn a_device_hub_device_origin_maps_to_the_device_area_on_screen() {
    // Given
    let viewport = a_device_hub_viewport();

    // When
    let origin = viewport.to_screen(0, 0);

    // Then
    assert_eq!(origin, ScreenPoint { x: 862.0, y: 256.0 });
}

#[test]
fn a_swipe_path_runs_against_the_scroll_and_stays_on_screen() {
    // When: scroll down 40 px from near the top of a 100×100 screen
    let path = tddy_ios_emulator::swipe_path((50, 10), (0, 40), (100, 100), 3);

    // Then: it starts at the pointer and is clamped at the top edge
    assert_eq!(path, vec![(50, 10), (50, 0), (50, 0), (50, 0), (50, 0)]);
}
