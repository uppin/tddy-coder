//! Unit tests: `tddy-tools list-models` accepts the OpenCode binary the daemon resolved, so the
//! `opencode-cli` probe runs that binary rather than a PATH lookup.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md

use std::path::PathBuf;

use clap::Parser;
use tddy_tools::list_models::ListModelsArgs;

#[test]
fn list_models_takes_the_opencode_binary_path_the_daemon_resolved() {
    // When
    let args = ListModelsArgs::try_parse_from([
        "list-models",
        "--agent",
        "opencode-cli",
        "--opencode-cli-path",
        "/home/dev/.opencode/bin/opencode",
    ])
    .expect("the probe's argv parses");

    // Then
    assert_eq!(
        args.opencode_cli_path,
        Some(PathBuf::from("/home/dev/.opencode/bin/opencode"))
    );
}

#[test]
fn list_models_needs_no_opencode_path() {
    // When
    let args = ListModelsArgs::try_parse_from(["list-models", "--agent", "opencode-cli"])
        .expect("the path is optional");

    // Then
    assert_eq!(args.opencode_cli_path, None);
}
