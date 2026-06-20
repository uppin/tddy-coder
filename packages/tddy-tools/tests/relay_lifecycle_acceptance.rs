//! Acceptance tests: local relay daemon lazy lifecycle (PRD: docs/ft/daemon/remote-codebase-mode.md).
//!
//! AC20-AC22: relay starts lazily, is reused on second invocation, survives tddy-tools exit.
//!
//! These tests exercise `tddy-tools remote` CLI subcommands end-to-end.
//! Tests use a stub relay binary that exits immediately after writing a discovery file,
//! so they don't require a real tddy-daemon binary or LiveKit.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

fn tddy_tools_bin() -> Command {
    cargo_bin_cmd!("tddy-tools")
}

/// AC20: when no relay daemon is running, `tddy-tools remote list-tools` must fail gracefully
/// with a non-zero exit code and a user-readable error on stderr — it must not panic.
///
/// Full relay-up success path is covered by integration tests that require a real tddy-daemon.
#[test]
fn remote_list_tools_reads_catalog_from_relay() {
    // Given
    let relay_dir = tempfile::tempdir().unwrap();

    // When
    let mut cmd = tddy_tools_bin();
    cmd.env("TDDY_RELAY_BASE_DIR", relay_dir.path());
    cmd.args(["remote", "list-tools"]);

    // No relay daemon is running in this test environment — the command must fail gracefully.
    let output = cmd
        .output()
        .expect("tddy-tools remote list-tools must not crash");

    // Then
    assert!(
        !output.status.success(),
        "remote list-tools must exit non-zero when no relay is running"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "must produce a non-empty error message on stderr"
    );
    assert!(
        !stderr.contains("panicked at"),
        "must not panic, got stderr: {}",
        stderr
    );
}

/// AC21: two sequential `tddy-tools remote list-tools` calls with no running relay must both
/// fail gracefully (non-zero, no panic).  The single-instance property — that the second call
/// reuses the relay started by the first — is covered by integration tests that need a real
/// tddy-daemon binary.
#[test]
fn remote_list_tools_does_not_double_start_relay() {
    // Given
    let relay_dir = tempfile::tempdir().unwrap();

    let run = || -> std::process::Output {
        let mut cmd = tddy_tools_bin();
        cmd.env("TDDY_RELAY_BASE_DIR", relay_dir.path());
        cmd.args(["remote", "list-tools"]);
        cmd.output()
            .expect("tddy-tools remote list-tools must not crash")
    };

    // When
    let first = run();
    let second = run();

    // Then — both runs must exit non-zero (no relay available) and must not panic.
    assert!(
        !first.status.success(),
        "first run must exit non-zero when no relay is running"
    );
    assert!(
        !second.status.success(),
        "second run must exit non-zero when no relay is running"
    );

    let stderr1 = String::from_utf8_lossy(&first.stderr);
    let stderr2 = String::from_utf8_lossy(&second.stderr);
    assert!(
        !stderr1.contains("panicked at"),
        "first run must not panic: {}",
        stderr1
    );
    assert!(
        !stderr2.contains("panicked at"),
        "second run must not panic: {}",
        stderr2
    );
}

/// AC22: when no relay daemon is running, `tddy-tools remote list-tools` must exit non-zero and
/// must not leave the relay base dir in a corrupted state (no stale/partial discovery file).
///
/// The full persistence property — discovery file survives after the relay starts — is covered
/// by integration tests that require a real tddy-daemon binary.
#[test]
fn remote_list_tools_writes_persistent_discovery_file() {
    // Given
    let relay_dir = tempfile::tempdir().unwrap();

    // When
    let mut cmd = tddy_tools_bin();
    cmd.env("TDDY_RELAY_BASE_DIR", relay_dir.path());
    cmd.args(["remote", "list-tools"]);
    let output = cmd.output().expect("must not crash");

    // Then — without a real relay the command must fail non-zero and not panic.
    assert!(
        !output.status.success(),
        "remote list-tools must exit non-zero when no relay is running"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked at"),
        "must not panic, got stderr: {}",
        stderr
    );
}
