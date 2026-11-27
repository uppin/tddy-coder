//! `tddy-ios-emulator --list-devices`: the booted simulators, as the host service reads them.

mod common;

use common::*;
use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_ios_emulator::{booted_devices_json, list_booted_devices, DeviceState, XcrunSimctl};

#[test]
fn only_booted_devices_are_listed_with_their_udid_name_and_runtime() {
    // Given
    let devices = [
        a_device(IPHONE_UDID, DeviceState::Booted),
        a_device(
            "1B2C3D4E-2222-4333-8444-A55566667777",
            DeviceState::Shutdown,
        ),
    ];

    // When
    let listed: serde_json::Value = serde_json::from_str(&booted_devices_json(&devices)).unwrap();

    // Then
    assert_eq!(
        listed,
        json!([{ "udid": IPHONE_UDID, "name": IPHONE_NAME, "runtime": "iOS 18.2" }])
    );
}

#[tokio::test]
async fn listing_through_an_xcrun_that_does_not_exist_fails_naming_the_command() {
    // Given
    let simctl = XcrunSimctl::at("/nonexistent/xcrun");

    // When
    let error = list_booted_devices(&simctl)
        .await
        .expect_err("listing through a missing xcrun must fail");

    // Then
    assert!(
        format!("{error:#}").contains("xcrun simctl list devices"),
        "the error should name what failed, got: {error:#}"
    );
}
