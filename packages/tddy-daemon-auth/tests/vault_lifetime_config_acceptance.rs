//! Acceptance: what `github.pending_login_ttl_seconds` and `github.open_vault_idle_ttl_seconds`
//! mean, decided where the credential vaults are built.
//!
//! The daemon's config (`tddy-daemon-kernel`) reads both as plain optional whole numbers of
//! seconds. Their meaning is applied here, as auth is wired at startup:
//!
//! - absent: ten minutes for a pending sign-in; the refresh-token lifetime (seven days) for an open
//!   vault nobody uses;
//! - `0`: never;
//! - past the refresh-token lifetime: **the daemon does not start**, and the error names the
//!   setting and its limit — whether or not `auth_storage` is configured.

use std::path::Path;
use std::time::Duration;

use tddy_daemon_auth::build_auth_entries;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_github::REFRESH_TOKEN_TTL;

const SEVEN_DAYS_AND_A_SECOND: u64 = 7 * 24 * 60 * 60 + 1;

#[test]
fn a_github_block_naming_neither_lifetime_waits_ten_minutes_and_idles_out_after_seven_days() {
    // Given a daemon whose `github:` block names neither lifetime
    let daemon = a_daemon_configured_with("", true);

    // When auth is wired
    let lifetimes = the_vault_lifetimes_of(&daemon);

    // Then
    assert_eq!(
        lifetimes,
        (Some(Duration::from_secs(600)), Some(REFRESH_TOKEN_TTL))
    );
}

#[test]
fn a_lifetime_of_zero_means_never_for_either() {
    // Given
    let daemon = a_daemon_configured_with(
        "  pending_login_ttl_seconds: 0\n  open_vault_idle_ttl_seconds: 0\n",
        true,
    );

    // When
    let lifetimes = the_vault_lifetimes_of(&daemon);

    // Then
    assert_eq!(lifetimes, (None, None));
}

#[test]
fn explicit_lifetimes_are_taken_as_given() {
    // Given
    let daemon = a_daemon_configured_with(
        "  pending_login_ttl_seconds: 120\n  open_vault_idle_ttl_seconds: 3600\n",
        true,
    );

    // When
    let lifetimes = the_vault_lifetimes_of(&daemon);

    // Then
    assert_eq!(
        lifetimes,
        (
            Some(Duration::from_secs(120)),
            Some(Duration::from_secs(3600))
        )
    );
}

#[test]
fn a_pending_lifetime_longer_than_a_week_stops_the_daemon_naming_the_setting_and_its_limit() {
    // Given
    let daemon = a_daemon_configured_with(
        &format!("  pending_login_ttl_seconds: {SEVEN_DAYS_AND_A_SECOND}\n"),
        true,
    );

    // When auth is wired at startup
    let refused = startup_error_of(&daemon);

    // Then
    assert_eq!(
        (
            refused.contains("github.pending_login_ttl_seconds"),
            refused.contains("604800")
        ),
        (true, true),
        "expected a refusal naming the setting and its limit, got {refused:?}"
    );
}

#[test]
fn an_idle_lifetime_longer_than_a_week_stops_the_daemon_naming_the_setting_and_its_limit() {
    // Given
    let daemon = a_daemon_configured_with(
        &format!("  open_vault_idle_ttl_seconds: {SEVEN_DAYS_AND_A_SECOND}\n"),
        true,
    );

    // When auth is wired at startup
    let refused = startup_error_of(&daemon);

    // Then
    assert_eq!(
        (
            refused.contains("github.open_vault_idle_ttl_seconds"),
            refused.contains("604800")
        ),
        (true, true),
        "expected a refusal naming the setting and its limit, got {refused:?}"
    );
}

#[test]
fn an_out_of_range_lifetime_stops_the_daemon_even_with_no_auth_storage() {
    // Given a daemon that keeps no credential vaults, with an idle lifetime past its limit
    let daemon = a_daemon_configured_with(
        &format!("  open_vault_idle_ttl_seconds: {SEVEN_DAYS_AND_A_SECOND}\n"),
        false,
    );

    // When auth is wired at startup
    let refused = startup_error_of(&daemon);

    // Then the setting is refused all the same
    assert!(
        refused.contains("github.open_vault_idle_ttl_seconds"),
        "expected a refusal naming the setting, got {refused:?}"
    );
}

#[test]
fn a_negative_lifetime_in_yaml_stops_the_config_load_naming_the_setting() {
    // Given a config file with a negative idle lifetime
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = a_config_file(dir.path(), "  open_vault_idle_ttl_seconds: -1\n", false);

    // When the daemon loads it
    let refused = DaemonConfig::load(&path)
        .err()
        .map(|e| format!("{e:#}"))
        .unwrap_or_default();

    // Then
    assert!(
        refused.contains("open_vault_idle_ttl_seconds"),
        "expected a refusal naming the setting, got {refused:?}"
    );
}

/// A daemon's loaded config, with `github_lines` in its stub `github:` block — and an
/// `auth_storage` when `keeps_vaults`, otherwise only a `tddy_data_dir` for its signing key. The
/// directory lives as long as the value.
struct AConfiguredDaemon {
    config: DaemonConfig,
    _dir: tempfile::TempDir,
}

fn a_daemon_configured_with(github_lines: &str, keeps_vaults: bool) -> AConfiguredDaemon {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = a_config_file(dir.path(), github_lines, keeps_vaults);
    AConfiguredDaemon {
        config: DaemonConfig::load(&path).expect("the config loads"),
        _dir: dir,
    }
}

fn a_config_file(dir: &Path, github_lines: &str, keeps_vaults: bool) -> std::path::PathBuf {
    let where_state_lives = if keeps_vaults {
        format!("auth_storage: \"{}\"\n", dir.join("auth").display())
    } else {
        format!("tddy_data_dir: \"{}\"\n", dir.join("data").display())
    };
    let yaml = format!(
        "users:\n  - github_user: \"u\"\n    os_user: \"u\"\ngithub:\n  stub: true\n{github_lines}\
         {where_state_lives}"
    );
    let path = dir.join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    path
}

/// How long the built vaults hold a pending sign-in, and an open vault nobody uses.
fn the_vault_lifetimes_of(daemon: &AConfiguredDaemon) -> (Option<Duration>, Option<Duration>) {
    let vaults = build_auth_entries(&daemon.config, "127.0.0.1", 0)
        .expect("the daemon starts")
        .credential_vaults
        .expect("an auth_storage builds the credential vaults");
    (vaults.pending_lifetime(), vaults.idle_lifetime())
}

/// Why wiring auth refused to start the daemon — `""` when it started.
fn startup_error_of(daemon: &AConfiguredDaemon) -> String {
    build_auth_entries(&daemon.config, "127.0.0.1", 0)
        .err()
        .map(|e| format!("{e:#}"))
        .unwrap_or_default()
}
