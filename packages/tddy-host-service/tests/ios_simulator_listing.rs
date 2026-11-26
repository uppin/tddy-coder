//! What the host service reads from `tddy-ios-emulator --list-devices`.

use pretty_assertions::assert_eq;
use tddy_host_service::ios_simulators::{parse_listed_devices, BootedSimulator};

#[test]
fn the_bridge_listing_reads_as_booted_simulators() {
    // Given
    let stdout = r#"[{"udid":"UDID-IPHONE","name":"iPhone 16 Pro","runtime":"iOS 18.2"}]
"#;

    // When
    let booted = parse_listed_devices(stdout).unwrap();

    // Then
    assert_eq!(
        booted,
        vec![BootedSimulator {
            udid: "UDID-IPHONE".into(),
            name: "iPhone 16 Pro".into(),
            runtime: "iOS 18.2".into(),
        }]
    );
}

#[test]
fn an_unreadable_listing_is_an_error() {
    // Given
    let stdout = "xcrun: error: unable to find utility \"simctl\"";

    // When
    let result = parse_listed_devices(stdout);

    // Then
    assert!(result.is_err(), "expected an error, got {result:?}");
}
