//! Where the daemon finds `x11vnc`, the program that serves an X11 display to the VNC bridge.
//!
//! Unlike the two bridge resolvers beside it, there is no current-exe sibling step: x11vnc is a
//! system package and is never shipped beside tddy, so an unset path is a PATH lookup.

use std::path::PathBuf;

use tddy_daemon_kernel::config::{resolve_x11vnc_binary_path, ScreenSharingConfig};

fn a_screen_sharing_config_naming_x11vnc_at(path: &str) -> ScreenSharingConfig {
    ScreenSharingConfig {
        x11vnc_binary_path: path.to_string(),
        ..Default::default()
    }
}

#[test]
fn resolves_x11vnc_to_the_configured_path() {
    // Given
    let config = a_screen_sharing_config_naming_x11vnc_at("/opt/x11vnc/bin/x11vnc");

    // When
    let resolved = resolve_x11vnc_binary_path(&config);

    // Then
    assert_eq!(resolved, PathBuf::from("/opt/x11vnc/bin/x11vnc"));
}

#[test]
fn resolves_an_unconfigured_x11vnc_to_a_path_lookup_of_its_bare_name() {
    // Given
    let config = ScreenSharingConfig::default();

    // When
    let resolved = resolve_x11vnc_binary_path(&config);

    // Then
    assert_eq!(resolved, PathBuf::from("x11vnc"));
}
