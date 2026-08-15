//! Acceptance: the command line itself — AC23 of `docs/ft/daemon/session-worktree-sync.md`.
//!
//! This binary is configured from the environment (and the repo-root `.env` beneath it), so `--help`
//! runs with a daemon refresh token set. Printing its *value* — which clap does by default for an
//! `env`-backed argument — leaks a 7-day credential into any terminal, screen share or CI log that
//! ever asks for usage.

use std::process::{Command, Output};

const BINARY: &str = env!("CARGO_BIN_EXE_tddy-session-sync");

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
fn never_prints_the_value_of_a_token_it_found_in_the_environment() {
    // Given a configured refresh token — the credential this tool is meant to be configured with
    let output = run(
        &["--help"],
        &[("TDDY_REFRESH_TOKEN", "gho-secret-refresh-value")],
    );

    // When
    let usage = stdout_and_stderr(&output);

    // Then the variable is documented by name, and its value is nowhere in the output.
    assert!(
        usage.contains("TDDY_REFRESH_TOKEN"),
        "--help must name the environment variable, got:\n{usage}"
    );
    assert!(
        !usage.contains("gho-secret-refresh-value"),
        "--help must not print the token it found in the environment, got:\n{usage}"
    );
}
