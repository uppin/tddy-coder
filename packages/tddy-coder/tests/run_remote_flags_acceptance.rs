//! Acceptance tests: `run_remote` full dispatch and new remote flags (Gap D).
//!
//! AC: `--remote-daemon-url`, `--remote-session-token`, and `--remote-daemon-id` flags must
//!     appear in `tddy-coder --help`.
//! AC: `--remote --remote-daemon-url <url> --recipe free-prompting` routes to `run_remote`
//!     instead of the normal workflow. Without a live daemon it should fail with a meaningful
//!     error about "remote session" or "relay", not the generic "not fully implemented" message.
//! AC: `--remote` without `--remote-daemon-url` fails with a clear error about the missing URL
//!     (not about "recipe" or "not fully implemented").

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

fn tddy_coder_bin() -> Command {
    cargo_bin_cmd!("tddy-coder")
}

// ── help / flag presence ──────────────────────────────────────────────────────────────────────────

/// AC: `tddy-coder --help` lists `--remote-daemon-url` as an accepted flag.
///
/// Currently the flag does not exist — it must be added to `Args`/`CoderArgs`.
#[test]
fn help_lists_remote_daemon_url_flag() {
    // When
    let output = tddy_coder_bin()
        .arg("--help")
        .output()
        .expect("tddy-coder --help must not crash");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Then
    assert!(
        stdout.contains("remote-daemon-url"),
        "--remote-daemon-url must appear in --help output; got: {}",
        stdout
    );
}

/// AC: `tddy-coder --help` lists `--remote-session-token` as an accepted flag.
#[test]
fn help_lists_remote_session_token_flag() {
    // When
    let output = tddy_coder_bin()
        .arg("--help")
        .output()
        .expect("tddy-coder --help must not crash");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Then
    assert!(
        stdout.contains("remote-session-token"),
        "--remote-session-token must appear in --help output; got: {}",
        stdout
    );
}

/// AC: `tddy-coder --help` lists `--remote-daemon-id` as an accepted flag.
#[test]
fn help_lists_remote_daemon_id_flag() {
    // When
    let output = tddy_coder_bin()
        .arg("--help")
        .output()
        .expect("tddy-coder --help must not crash");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Then
    assert!(
        stdout.contains("remote-daemon-id"),
        "--remote-daemon-id must appear in --help output; got: {}",
        stdout
    );
}

// ── routing: --remote dispatches to run_remote ───────────────────────────────────────────────────

/// AC: `--remote --remote-daemon-url <url> --recipe free-prompting` dispatches to `run_remote`
/// and fails with an error about the remote connection (not "not fully implemented").
///
/// Without a real daemon, `run_remote` should exit non-zero with an error message mentioning
/// the daemon, the relay, or the session — NOT the old placeholder text.
#[test]
fn remote_with_daemon_url_dispatches_to_run_remote_not_placeholder() {
    // When
    let output = tddy_coder_bin()
        .args([
            "--remote",
            "--remote-daemon-url",
            "http://127.0.0.1:19999", // unreachable — no daemon running here
            "--remote-session-token",
            "test-token",
            "--recipe",
            "free-prompting",
            "--prompt",
            "test prompt",
        ])
        .output()
        .expect("tddy-coder --remote --remote-daemon-url must not crash");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");

    // Then — must fail (no real daemon).
    assert!(
        !output.status.success(),
        "tddy-coder --remote with unreachable daemon must exit non-zero"
    );

    // The flag --remote-daemon-url must be ACCEPTED (not rejected by clap as "unexpected argument").
    // If the flag doesn't exist yet, clap would say "unexpected argument '--remote-daemon-url'" —
    // that would mean the flag hasn't been added to Args yet.
    assert!(
        !combined.contains("unexpected argument") && !combined.contains("unrecognized option"),
        "--remote-daemon-url must be a recognized flag; got (clap rejection): {}",
        combined
    );

    // Must NOT be the old placeholder message.
    assert!(
        !combined.contains("not yet fully implemented"),
        "error must not contain the stub message 'not yet fully implemented'; got: {}",
        combined
    );

    // Must not panic.
    assert!(
        !combined.contains("panicked at"),
        "tddy-coder --remote must not panic; got: {}",
        combined
    );
}

/// AC: `--remote` without `--remote-daemon-url` fails with a clear error about the missing URL.
///
/// Currently this falls through to the normal free-prompting workflow (or fails with a
/// confusing error). After the fix, it should immediately error about missing daemon URL.
#[test]
fn remote_without_daemon_url_fails_with_missing_url_error() {
    // When
    let output = tddy_coder_bin()
        .args(["--remote", "--recipe", "free-prompting", "--prompt", "test"])
        .output()
        .expect("tddy-coder --remote without --remote-daemon-url must not crash");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");

    // Then — must fail.
    assert!(
        !output.status.success(),
        "tddy-coder --remote without --remote-daemon-url must exit non-zero"
    );

    // Error must reference the daemon URL requirement.
    assert!(
        combined.contains("remote-daemon-url")
            || combined.contains("daemon url")
            || combined.contains("daemon URL"),
        "error must mention missing --remote-daemon-url; got: {}",
        combined
    );

    // Must not panic.
    assert!(
        !combined.contains("panicked at"),
        "must not panic; got: {}",
        combined
    );
}
