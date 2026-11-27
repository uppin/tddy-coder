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

/// Writes the frame the bridge would stream to `$TDDY_IOS_SIM_DUMP_DIR/streamed.bmp`, so it can be
/// compared by eye with `xcrun simctl io <udid> screenshot`: which stage loses quality — capture
/// and crop, or encoding — is read off the two pictures.
///
/// Run: `TDDY_IOS_SIM_UDID=<udid> TDDY_IOS_SIM_DUMP_DIR=<dir> cargo test -p tddy-ios-emulator
/// --test simulator_production -- --ignored the_streamed_frame_can_be_compared`
#[tokio::test]
#[ignore = "needs a booted iOS Simulator on a macOS host, and writes a file to look at"]
async fn the_streamed_frame_can_be_compared_with_a_native_screenshot() {
    // Given
    let udid = std::env::var("TDDY_IOS_SIM_UDID").expect("TDDY_IOS_SIM_UDID names a booted device");
    let dir =
        std::env::var("TDDY_IOS_SIM_DUMP_DIR").expect("TDDY_IOS_SIM_DUMP_DIR names a directory");
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
    let path = std::path::Path::new(&dir).join("streamed.bmp");
    std::fs::write(&path, bmp_of(client.get_rgba_frame(), width, height)).unwrap();
    assert!(path.exists());
}

/// An uncompressed 32-bit BMP of an RGBA frame: readable by Preview and `sips`, with no image crate.
fn bmp_of(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    const HEADERS: u32 = 14 + 40;
    let pixels = width * height * 4;
    let mut bmp = Vec::with_capacity((HEADERS + pixels) as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(HEADERS + pixels).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&HEADERS.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    // Negative height: rows top to bottom, as the frame holds them.
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&32u16.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&pixels.to_le_bytes());
    bmp.extend_from_slice(&[0; 16]);
    for pixel in rgba.chunks_exact(4) {
        bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    bmp
}

/// Taps the Maps widget through the device's `dtuhidd` and expects the screen to change, then
/// presses Home. Needs no Screen Recording: the evidence is `simctl io screenshot`.
///
/// Run: `TDDY_IOS_SIM_UDID=<udid> cargo test -p tddy-ios-emulator --test simulator_production --
/// --ignored a_tap_reaches_the_device`
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a booted iOS Simulator, on its home screen, on a macOS host"]
fn a_tap_reaches_the_device_through_dtuhidd() {
    use tddy_ios_emulator::{DtuhidInput, InputEvent, InputSink, Rect, ScreenPoint};

    // Given: input addressed in device-ratio space (a unit device area)
    let udid = std::env::var("TDDY_IOS_SIM_UDID").expect("TDDY_IOS_SIM_UDID names a booted device");
    let unit = Rect {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
    let input = DtuhidInput::connect(&udid, unit).unwrap();
    let before = screenshot(&udid);

    // When: tap the Maps widget, top-left of the home screen
    let widget = ScreenPoint { x: 0.17, y: 0.2 };
    input.post(InputEvent::MouseDown(widget)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(60));
    input.post(InputEvent::MouseUp(widget)).unwrap();
    // The app opens with an animation: poll for a changed screen rather than guess its length.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let after = loop {
        let now = screenshot(&udid);
        if now != before || std::time::Instant::now() >= deadline {
            break now;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    };
    input.press_home().unwrap();

    // Then
    assert_ne!(before, after, "the tap should have opened Maps");
}

#[cfg(target_os = "macos")]
fn screenshot(udid: &str) -> Vec<u8> {
    let path = std::env::temp_dir().join(format!("tddy-ios-{}.png", std::process::id()));
    let status = std::process::Command::new("/usr/bin/xcrun")
        .args(["simctl", "io", udid, "screenshot"])
        .arg(&path)
        .env_remove("DEVELOPER_DIR")
        .env_remove("SDKROOT")
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "simctl io screenshot failed");
    std::fs::read(&path).unwrap()
}
