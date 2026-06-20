//! Acceptance tests: relay daemon mode (PRD: docs/ft/daemon/remote-codebase-mode.md, Phase 3).
//!
//! AC: `tddy-daemon --relay` starts without a web_bundle_path requirement, exposes a
//! `relay:` config section with `idle_timeout_secs`, and the daemon's config parses it correctly.

use tddy_daemon::config::DaemonConfig;

/// Phase 3 AC: DaemonConfig accepts a `relay:` section with `idle_timeout_secs`.
/// The section is optional — normal daemon configs without it should still parse.
#[test]
fn relay_config_section_parses_with_idle_timeout() {
    // Given
    let yaml = r#"
relay:
  idle_timeout_secs: 300
listen:
  web_port: 0
"#;

    // When
    let cfg: DaemonConfig =
        serde_yaml::from_str(yaml).expect("DaemonConfig must accept a `relay:` section");

    // Then
    let relay = cfg
        .relay
        .as_ref()
        .expect("relay field must be populated from YAML");
    assert_eq!(
        relay.idle_timeout_secs, 300,
        "idle_timeout_secs must be 300 as specified in YAML"
    );
}

/// Phase 3 AC: DaemonConfig without a `relay:` section still parses (field is optional).
#[test]
fn relay_config_section_defaults_to_none_when_absent() {
    // Given
    let yaml = r#"
listen:
  web_port: 0
"#;

    // When / Then
    let cfg: DaemonConfig =
        serde_yaml::from_str(yaml).expect("DaemonConfig must parse without a relay section");
    assert!(
        cfg.relay.is_none(),
        "relay must default to None when not configured"
    );
}

/// Phase 3 AC: in relay mode, `web_bundle_path` is not required.
/// This verifies the DaemonConfig validates as valid even without web_bundle_path when relay is set.
/// (The runtime `main.rs` must skip the web_bundle_path existence check in relay mode.)
#[test]
fn relay_mode_config_is_valid_without_web_bundle_path() {
    // Given
    let yaml = r#"
relay:
  idle_timeout_secs: 1800
listen:
  web_port: 0
livekit:
  url: "ws://localhost:7880"
  api_key: "devkey"
  api_secret: "devsecret"
  common_room: "test-room"
daemon_instance_id: "relay-local"
"#;

    // When / Then
    let cfg: DaemonConfig = serde_yaml::from_str(yaml)
        .expect("relay-mode DaemonConfig must be valid without web_bundle_path");

    assert!(
        cfg.web_bundle_path.is_none(),
        "web_bundle_path must be None in relay mode"
    );
    assert!(cfg.relay.is_some(), "relay section must be populated");
}

/// Phase 3 AC: RelayConfig has a sensible default idle_timeout_secs (e.g. 1800).
#[test]
fn relay_config_default_idle_timeout_is_sensible() {
    // Given / When
    use tddy_daemon::config::RelayConfig;
    let cfg = RelayConfig::default();

    // Then
    assert!(
        cfg.idle_timeout_secs >= 60,
        "default idle_timeout_secs must be at least 60 seconds, got {}",
        cfg.idle_timeout_secs
    );
}
