//! Against a real booted Simulator. Ignored by default: needs macOS, Xcode, a booted device whose
//! udid is in `TDDY_IOS_SIM_UDID`, its window on screen, and Screen Recording + Accessibility
//! granted to the process running the test.
//!
//! Run: `TDDY_IOS_SIM_UDID=<udid> cargo test -p tddy-ios-emulator --test simulator_production -- --ignored`

use pretty_assertions::assert_ne;
use tddy_ios_emulator::IosSimulatorClient;
use tddy_screenshare::{ConnectTarget, Credentials, ScreenSharingClient};

#[tokio::test]
#[ignore = "needs a booted iOS Simulator on a macOS host"]
async fn a_booted_simulator_streams_a_non_blank_device_screen() {
    // Given
    let udid = std::env::var("TDDY_IOS_SIM_UDID").expect("TDDY_IOS_SIM_UDID names a booted device");
    let target = ConnectTarget {
        host: "localhost".into(),
        port: 0,
        device: Some(udid),
    };
    let mut client = IosSimulatorClient::connect(&target, &Credentials::default())
        .await
        .unwrap();

    // When
    client.poll_events().await.unwrap();

    // Then
    let (width, height) = client.framebuffer_dimensions();
    assert_ne!((width, height), (0, 0));
    assert_ne!(
        client.get_rgba_frame().iter().copied().max(),
        Some(0),
        "the device screen should not be blank"
    );
}
