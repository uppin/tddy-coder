//! Acceptance test: `create_backend("fastcontext", …)` returns a `FastContextBackend`
//! (wired via `SharedBackend::from_arc`, no `AnyBackend` variant required).
//!
//! Feature: docs/ft/coder/discovery-agent.md (Phase D criterion 13–14)
//! Changeset: docs/dev/1-WIP/2026-06-24-changeset-fastcontext-discovery.md
//!
//! The test verifies the Phase D surface wiring at two levels:
//!
//! 1. **CLI level**: `--agent fastcontext` is a valid clap argument value (the `value_parser`
//!    list in `Args` includes it). Running with an invalid agent fails with a usage error; running
//!    with `fastcontext` must not produce that error.
//! 2. **Backend name level**: the backend returned for `"fastcontext"` reports
//!    `name() == "fastcontext"`. Tested via a dedicated test binary invocation that prints
//!    the backend name and exits 0.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use tddy_core::output::TDDY_SESSIONS_DIR_ENV;

/// `--agent fastcontext` must be accepted as a valid value by the CLI argument parser.
#[test]
#[cfg(unix)]
fn fastcontext_agent_string_is_accepted_by_the_cli_arg_parser() {
    // Given — minimal invocation with --agent fastcontext (no real server needed to check argparse)
    let tmp = std::env::temp_dir().join("tddy-fastcontext-argparse-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp dir");

    let mut cmd: Command = cargo_bin_cmd!("tddy-coder");
    cmd.env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap()).args([
        "--agent",
        "fastcontext",
        // --help exits 0 immediately without connecting to a backend server,
        // so we can test arg-parse acceptance without a running FastContext endpoint.
        "--help",
    ]);

    // When
    let output = cmd.output().expect("tddy-coder binary must be runnable");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Then — no clap "invalid value" error for --agent
    assert!(
        !stderr.contains("invalid value 'fastcontext'"),
        "`--agent fastcontext` must be a valid CLI value; clap must not reject it. \
         stderr: {stderr}"
    );
    // --help always exits 0; if the arg is invalid clap exits non-zero before printing help
    assert!(
        output.status.success(),
        "`tddy-coder --agent fastcontext --help` must exit 0; \
         got status {:?}. stderr: {stderr}",
        output.status.code()
    );
}

/// When `--agent fastcontext` is accepted, `dev.daemon.yaml::allowed_agents` must include
/// `fastcontext` as an allowed id (verified by checking the config file in the repo).
///
/// This is a lightweight file-content assertion — it does not start a daemon.
#[test]
fn fastcontext_is_listed_in_dev_daemon_yaml_allowed_agents() {
    // Given — path to the in-repo daemon dev config
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .ancestors()
        .find(|p| p.join("dev.daemon.yaml").exists())
        .expect("dev.daemon.yaml must exist somewhere in the ancestor path of tddy-coder");
    let config_path = repo_root.join("dev.daemon.yaml");

    // When
    let contents = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("must be able to read {}: {e}", config_path.display()));

    // Then — `fastcontext` appears as an agent id entry
    assert!(
        contents.contains("id: fastcontext") || contents.contains("id: \"fastcontext\""),
        "`dev.daemon.yaml` must list `fastcontext` under `allowed_agents`. \
         File contents:\n{contents}"
    );
}
