//! Acceptance tests: relay daemon runtime mode (Phase 3 follow-up).
//!
//! AC: `tddy-daemon --relay` CLI flag is accepted; in relay mode `web_bundle_path` is not
//! required; the idle-timeout tracker correctly reports when the daemon should shut down.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

fn tddy_daemon_bin() -> Command {
    cargo_bin_cmd!("tddy-daemon")
}

/// Phase 3 AC: `tddy-daemon --help` lists `--relay` as an accepted flag.
#[test]
fn relay_flag_appears_in_daemon_help() {
    let output = tddy_daemon_bin()
        .arg("--help")
        .output()
        .expect("tddy-daemon --help must not crash");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--relay"),
        "--relay must appear in tddy-daemon --help output; got: {}",
        stdout
    );
}

/// Phase 3 AC: `DaemonConfig::validate_for_relay` (or equivalent logic) succeeds when
/// `relay` is set and `web_bundle_path` is absent — relay mode does not serve static files.
#[test]
fn relay_mode_config_validate_does_not_require_web_bundle() {
    use tddy_daemon::config::DaemonConfig;

    let yaml = r#"
relay:
  idle_timeout_secs: 300
listen:
  web_port: 0
daemon_instance_id: "relay-test"
"#;
    let cfg: DaemonConfig = serde_yaml::from_str(yaml).expect("must parse");
    assert!(cfg.relay.is_some(), "relay must be set");
    assert!(cfg.web_bundle_path.is_none(), "web_bundle_path must be absent");

    // validate_for_relay must return Ok — no web_bundle_path required in relay mode.
    cfg.validate_for_relay()
        .expect("validate_for_relay must succeed without web_bundle_path");
}

/// Phase 3 AC: `DaemonConfig::validate_for_relay` returns Err when called on a non-relay config.
#[test]
fn non_relay_config_validate_for_relay_returns_err() {
    use tddy_daemon::config::DaemonConfig;

    let yaml = r#"
listen:
  web_port: 0
"#;
    let cfg: DaemonConfig = serde_yaml::from_str(yaml).expect("must parse");
    assert!(cfg.relay.is_none(), "relay must be absent");

    let result = cfg.validate_for_relay();
    assert!(
        result.is_err(),
        "validate_for_relay must return Err when relay section is absent"
    );
}

/// Phase 3 AC: `IdleTimeoutTracker` reports `should_shutdown()` = false when activity is recent.
#[test]
fn idle_timeout_tracker_not_expired_when_recently_active() {
    use tddy_daemon::relay_idle::IdleTimeoutTracker;
    use std::time::Duration;

    let tracker = IdleTimeoutTracker::new(Duration::from_secs(300));
    tracker.record_activity();

    assert!(
        !tracker.should_shutdown(),
        "should_shutdown must be false immediately after activity"
    );
}

/// Phase 3 AC: `IdleTimeoutTracker` reports `should_shutdown()` = true when idle past the timeout.
#[test]
fn idle_timeout_tracker_expired_after_timeout_duration() {
    use tddy_daemon::relay_idle::IdleTimeoutTracker;
    use std::time::Duration;

    // Use a 1ms timeout — any real code path will exceed it.
    let tracker = IdleTimeoutTracker::new(Duration::from_millis(1));
    std::thread::sleep(Duration::from_millis(10));

    assert!(
        tracker.should_shutdown(),
        "should_shutdown must be true after idle timeout expires"
    );
}
