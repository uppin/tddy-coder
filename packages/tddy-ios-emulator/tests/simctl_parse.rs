//! `parse_device_list` over a captured `xcrun simctl list devices --json`.

use pretty_assertions::assert_eq;
use tddy_ios_emulator::{parse_device_list, DeviceState, SimDevice};

const LIST_DEVICES: &str = include_str!("fixtures/simctl-list-devices.json");

#[test]
fn lists_available_devices_with_a_readable_runtime_and_their_state() {
    // Given: one booted iPhone and one shut-down iPad on iOS 18.2, an unavailable iPhone on iOS 16.4

    // When
    let devices = parse_device_list(LIST_DEVICES).unwrap();

    // Then
    assert_eq!(
        devices,
        vec![
            SimDevice {
                udid: "8A1C2E4F-1111-4222-8333-944455556666".into(),
                name: "iPhone 16 Pro".into(),
                runtime: "iOS 18.2".into(),
                state: DeviceState::Booted,
            },
            SimDevice {
                udid: "1B2C3D4E-2222-4333-8444-A55566667777".into(),
                name: "iPad Air 11-inch (M2)".into(),
                runtime: "iOS 18.2".into(),
                state: DeviceState::Shutdown,
            },
        ]
    );
}

#[test]
fn keeps_a_transitional_state_verbatim() {
    // Given
    let json = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-18-2":[
        {"udid":"U-1","name":"iPhone 16","state":"Shutting Down","isAvailable":true}
    ]}}"#;

    // When
    let devices = parse_device_list(json).unwrap();

    // Then
    assert_eq!(devices[0].state, DeviceState::Other("Shutting Down".into()));
}

#[test]
fn rejects_output_that_is_not_a_device_list() {
    // Given
    let json = r#"{"runtimes":[]}"#;

    // When
    let result = parse_device_list(json);

    // Then
    assert!(result.is_err(), "expected an error, got {result:?}");
}
