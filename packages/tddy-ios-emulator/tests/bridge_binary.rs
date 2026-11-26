//! The `tddy-ios-emulator` binary's start report, as the daemon reads it.

use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_screenshare::{read_readiness, BridgeReadiness, FailureKind};

const UNKNOWN_UDID: &str = "00000000-DEAD-BEEF-0000-000000000000";

/// Run the bridge with a config naming `device`, and return its first stdout line.
fn start_report_for_device(device: &str) -> String {
    let config = json!({
        "host": "localhost",
        "port": 0,
        "password": "",
        "livekit_url": "ws://127.0.0.1:1",
        "livekit_token": "unused",
        "livekit_room": "room",
        "livekit_identity": "bridge",
        "track_name": "screenshare:sim",
        "width": 0,
        "height": 0,
        "target_id": "sim",
        "device": device,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_tddy-ios-emulator"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tddy-ios-emulator");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(config.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn an_unknown_device_is_reported_as_a_connect_failure_naming_its_udid() {
    // Given
    let device = UNKNOWN_UDID;

    // When
    let report = read_readiness(&start_report_for_device(device)).expect("a readiness line");

    // Then
    let BridgeReadiness::Failed { kind, message } = report else {
        panic!("expected a failed start, got {report:?}");
    };
    assert_eq!(kind, FailureKind::Connect);
    assert!(
        message.contains(UNKNOWN_UDID),
        "the failure should name the device, got: {message}"
    );
}
