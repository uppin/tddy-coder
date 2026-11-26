//! `IosSimulatorClient` over in-memory ports: which devices it connects to, the frame it streams,
//! and where viewer input lands.

mod common;

use common::*;
use pretty_assertions::assert_eq;
use tddy_ios_emulator::{DeviceState, InputEvent, ScreenPoint, SimulatorError, SpecialKey};
use tddy_screenshare::ScreenSharingClient;

const LEFT_BUTTON: u32 = 1;
const NO_BUTTON: u32 = 0;
const XK_A_LOWER: u32 = 0x61;
const XK_RETURN: u32 = 0xff0d;

fn simulator_error(
    result: anyhow::Result<tddy_ios_emulator::IosSimulatorClient>,
) -> SimulatorError {
    result
        .err()
        .expect("expected connect to fail")
        .downcast::<SimulatorError>()
        .expect("expected a SimulatorError")
}

#[tokio::test]
async fn connect_to_a_shutdown_device_fails_naming_its_udid_and_state() {
    // Given
    let host = a_simulator_host().with_device(a_device(IPHONE_UDID, DeviceState::Shutdown));

    // When
    let error = simulator_error(host.connect(IPHONE_UDID).await);

    // Then
    assert_eq!(
        error,
        SimulatorError::DeviceNotBooted {
            udid: IPHONE_UDID.into(),
            state: DeviceState::Shutdown,
        }
    );
    assert_eq!(
        error.to_string(),
        format!("iOS Simulator {IPHONE_UDID} is Shutdown, not booted")
    );
}

#[tokio::test]
async fn connect_to_an_unknown_udid_fails_naming_it() {
    // Given
    let host = a_simulator_host().with_booted_iphone();

    // When
    let error = simulator_error(host.connect("00000000-DEAD-BEEF-0000-000000000000").await);

    // Then
    assert_eq!(
        error,
        SimulatorError::DeviceNotFound {
            udid: "00000000-DEAD-BEEF-0000-000000000000".into(),
        }
    );
}

#[tokio::test]
async fn connect_to_a_booted_device_without_a_window_fails_naming_the_device() {
    // Given
    let host = a_simulator_host().with_booted_iphone().without_window();

    // When
    let error = simulator_error(host.connect(IPHONE_UDID).await);

    // Then
    assert_eq!(
        error,
        SimulatorError::WindowNotFound {
            udid: IPHONE_UDID.into(),
            name: IPHONE_NAME.into(),
        }
    );
}

#[tokio::test]
async fn connect_to_a_booted_device_reports_the_device_screen_size_not_the_window() {
    // Given
    let host = a_simulator_host().with_booted_iphone();

    // When
    let client = host.connect(IPHONE_UDID).await.unwrap();

    // Then: an 8×10 pixel window minus its 2-row title bar
    assert_eq!(client.framebuffer_dimensions(), (8, 8));
}

#[tokio::test]
async fn a_polled_frame_is_the_device_area_with_the_title_bar_cropped_off() {
    // Given
    let blue = [0, 0, 255, 255];
    let host = a_simulator_host()
        .with_booted_iphone()
        .showing_frames(vec![a_window_frame(blue)]);
    let mut client = host.connect(IPHONE_UDID).await.unwrap();

    // When
    client.poll_events().await.unwrap();

    // Then
    assert_eq!(client.get_rgba_frame(), a_device_frame_of(blue).as_slice());
}

#[tokio::test]
async fn a_polled_frame_is_only_the_device_area_of_a_window_with_a_sidebar() {
    // Given
    let blue = [0, 0, 255, 255];
    let host = a_simulator_host()
        .with_booted_iphone_in(a_small_retina_window_with_a_sidebar())
        .showing_frames(vec![a_window_frame_with_a_sidebar(blue)]);
    let mut client = host.connect(IPHONE_UDID).await.unwrap();

    // When
    client.poll_events().await.unwrap();

    // Then
    assert_eq!(client.framebuffer_dimensions(), (8, 8));
    assert_eq!(client.get_rgba_frame(), a_device_frame_of(blue).as_slice());
}

#[tokio::test]
async fn poll_events_reports_a_change_only_when_the_frame_differs() {
    // Given
    let (red, green) = ([255, 0, 0, 255], [0, 255, 0, 255]);
    let host = a_simulator_host().with_booted_iphone().showing_frames(vec![
        a_window_frame(red),
        a_window_frame(red),
        a_window_frame(green),
    ]);
    let mut client = host.connect(IPHONE_UDID).await.unwrap();

    // When
    let changes = [
        client.poll_events().await.unwrap(),
        client.poll_events().await.unwrap(),
        client.poll_events().await.unwrap(),
    ];

    // Then
    assert_eq!(changes, [true, false, true]);
}

#[tokio::test]
async fn a_left_drag_becomes_touch_down_move_and_up_at_the_mapped_screen_points() {
    // Given: device pixels map to window (100, 201) + pixel / 2
    let host = a_simulator_host().with_booted_iphone();
    let mut client = host.connect(IPHONE_UDID).await.unwrap();

    // When
    client.inject_pointer(0, 0, LEFT_BUTTON).await.unwrap();
    client.inject_pointer(6, 4, LEFT_BUTTON).await.unwrap();
    client.inject_pointer(6, 4, NO_BUTTON).await.unwrap();

    // Then
    assert_eq!(
        host.input.events(),
        vec![
            InputEvent::MouseDown(ScreenPoint { x: 100.0, y: 201.0 }),
            InputEvent::MouseDrag(ScreenPoint { x: 103.0, y: 203.0 }),
            InputEvent::MouseUp(ScreenPoint { x: 103.0, y: 203.0 }),
        ]
    );
}

#[tokio::test]
async fn a_printable_key_types_its_character_once_and_return_is_pressed_and_released() {
    // Given
    let host = a_simulator_host().with_booted_iphone();
    let mut client = host.connect(IPHONE_UDID).await.unwrap();

    // When
    client.inject_key(XK_A_LOWER, true).await.unwrap();
    client.inject_key(XK_A_LOWER, false).await.unwrap();
    client.inject_key(XK_RETURN, true).await.unwrap();
    client.inject_key(XK_RETURN, false).await.unwrap();

    // Then
    assert_eq!(
        host.input.events(),
        vec![
            InputEvent::Text("a".into()),
            InputEvent::Key {
                key: SpecialKey::Return,
                pressed: true,
            },
            InputEvent::Key {
                key: SpecialKey::Return,
                pressed: false,
            },
        ]
    );
}

#[tokio::test]
async fn a_simulator_advertises_no_sign_in_methods() {
    // Given
    let target = tddy_screenshare::ConnectTarget {
        host: "localhost".into(),
        port: 0,
        device: Some(IPHONE_UDID.into()),
    };

    // When
    let methods = tddy_ios_emulator::IosSimulatorClient::inspect(&target)
        .await
        .unwrap();

    // Then
    assert_eq!(methods, vec![]);
}

#[tokio::test]
async fn a_scroll_down_becomes_an_upward_swipe_from_the_pointer() {
    // Given
    let host = a_simulator_host().with_booted_iphone();
    let mut client = host.connect(IPHONE_UDID).await.unwrap();

    // When: scroll down by 4 device pixels over (4, 6)
    client.inject_wheel(4, 6, 0, 4).await.unwrap();

    // Then: the finger goes down at (4, 6) and lifts at (4, 2)
    let events = host.input.events();
    assert_eq!(
        events.first(),
        Some(&InputEvent::MouseDown(ScreenPoint { x: 102.0, y: 204.0 }))
    );
    assert_eq!(
        events.last(),
        Some(&InputEvent::MouseUp(ScreenPoint { x: 102.0, y: 202.0 }))
    );
    assert!(
        events[1..events.len() - 1]
            .iter()
            .all(|event| matches!(event, InputEvent::MouseDrag(_))),
        "a swipe moves between its ends: {events:?}"
    );
}
