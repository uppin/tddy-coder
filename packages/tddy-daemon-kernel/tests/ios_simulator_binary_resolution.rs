//! Where the daemon finds `tddy-ios-emulator`, the bridge that streams an iOS Simulator.
//!
//! Resolved like the VNC and RDP bridges: the configured path, else a sibling of the daemon.

use tddy_daemon_kernel::config::{
    resolve_ios_simulator_binary_path, DaemonConfig, ScreenSharingConfig,
};

fn a_daemon_config_naming_the_ios_bridge_at(path: &str) -> DaemonConfig {
    DaemonConfig {
        screen_sharing: Some(ScreenSharingConfig {
            ios_simulator_binary_path: path.to_string(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn resolves_the_ios_bridge_to_the_configured_path() {
    // Given
    let config = a_daemon_config_naming_the_ios_bridge_at("/opt/tddy/bin/tddy-ios-emulator");

    // When
    let resolved = resolve_ios_simulator_binary_path(&config);

    // Then
    assert_eq!(resolved, "/opt/tddy/bin/tddy-ios-emulator");
}

#[test]
fn resolves_an_unconfigured_ios_bridge_to_a_sibling_of_the_running_executable() {
    // Given
    let config = DaemonConfig::default();
    let sibling = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("tddy-ios-emulator");

    // When
    let resolved = resolve_ios_simulator_binary_path(&config);

    // Then
    assert_eq!(resolved, sibling.to_string_lossy());
}
