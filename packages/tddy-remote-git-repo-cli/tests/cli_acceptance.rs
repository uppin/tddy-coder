//! Acceptance: the command line itself — what `--help` prints, and what a rejected command line
//! exits with.
//!
//! PRD: docs/ft/daemon/remote-git-repo.md § Credentials, § Client AC5.
//!
//! This binary is configured through `GIT_SSH_COMMAND` with credentials in the environment, so
//! `--help` runs with those credentials set. Printing their *values* — which clap does by default
//! for an `env`-backed argument — leaks a daemon refresh token into any terminal, screen share or
//! CI log that ever asks for usage.

use std::process::{Command, Output};

const BINARY: &str = env!("CARGO_BIN_EXE_tddy-remote-git-repo");

/// Run the real binary with `env` set and nothing inherited that would confuse the assertion.
fn run(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(BINARY);
    command.args(args).env_clear();
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("the binary must run")
}

fn stdout_and_stderr(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn documents_the_refresh_token_flag_without_printing_the_token_currently_in_the_environment() {
    // Given a configured refresh token — the credential a `GIT_SSH_COMMAND` environment carries
    let output = run(
        &["--help"],
        &[("TDDY_REFRESH_TOKEN", "gho-secret-refresh-value")],
    );

    // When
    let usage = stdout_and_stderr(&output);

    // Then the variable is documented, and its value is not
    assert!(
        usage.contains("TDDY_REFRESH_TOKEN"),
        "--help must name the environment variable, got:\n{usage}"
    );
    assert!(
        !usage.contains("gho-secret-refresh-value"),
        "--help must not print the token it found in the environment, got:\n{usage}"
    );
}

#[test]
fn documents_the_session_token_flag_without_printing_the_token_currently_in_the_environment() {
    // Given a configured access token
    let output = run(
        &["--help"],
        &[("TDDY_SESSION_TOKEN", "secret-access-value")],
    );

    // When
    let usage = stdout_and_stderr(&output);

    // Then
    assert!(
        usage.contains("TDDY_SESSION_TOKEN"),
        "--help must name the environment variable, got:\n{usage}"
    );
    assert!(
        !usage.contains("secret-access-value"),
        "--help must not print the token it found in the environment, got:\n{usage}"
    );
}

#[test]
fn exits_zero_from_help_so_it_is_not_read_as_a_transport_failure() {
    // Given
    let output = run(&["--help"], &[]);

    // When / Then
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn exits_with_sshs_transport_failure_code_when_the_command_line_itself_is_rejected() {
    // Given a connect timeout the environment spells wrong, which clap refuses before this
    // binary's own credential resolution ever runs
    let output = run(
        &["udoo-1", "git-upload-pack 'my-app'"],
        &[
            ("TDDY_DAEMON_URL", "http://127.0.0.1:1"),
            ("TDDY_SESSION_TOKEN", "access-token"),
            ("TDDY_CONNECT_TIMEOUT_SECS", "thirty"),
        ],
    );

    // Then 255, not clap's own 2 — git reads 255 as "the remote could not be reached" and 2 as
    // nothing in particular
    assert_eq!(output.status.code(), Some(255));
    let message = stdout_and_stderr(&output);
    assert!(
        message.contains("TDDY_CONNECT_TIMEOUT_SECS"),
        "the rejection must name the setting at fault, got:\n{message}"
    );
}
