//! Unit tests: a routed CLI launch writes its files owner-only and points the harness at them, and
//! keeps the session's model unless its route names the model itself.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md
//! Feature: docs/ft/daemon/muse-cli-session.md

use std::os::unix::fs::PermissionsExt;

use tddy_core::cli_harness::cli_harness;
use tddy_core::provider_route::{HarnessRoute, RouteFile, RouteSecret};

use crate::cli_route_launch::{cli_route_launch, launch_model_for, CliRouteLaunch};

fn a_fireworks_opencode_route() -> HarnessRoute {
    HarnessRoute {
        env: Vec::new(),
        secret_env: vec![(
            "TDDY_OPENCODE_PROVIDER_KEY".to_string(),
            RouteSecret::new("a-fireworks-key"),
        )],
        files: vec![RouteFile {
            name: "opencode.json".to_string(),
            contents: r#"{"model":"tddy-route/kimi-k2"}"#.to_string(),
        }],
        args: vec!["--model".to_string(), "tddy-route/kimi-k2".to_string()],
    }
}

fn value_of<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn an_opencode_session_on_fireworks_gets_a_config_naming_that_endpoint() {
    // Given
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let launch = cli_route_launch(
        opencode,
        Some(&a_fireworks_opencode_route()),
        session_dir.path(),
    )
    .expect("the route applies");

    // Then the config is written owner-only and named by OPENCODE_CONFIG
    let written = session_dir.path().join("opencode.json");
    assert_eq!(
        std::fs::read_to_string(&written).expect("written"),
        r#"{"model":"tddy-route/kimi-k2"}"#
    );
    let mode = std::fs::metadata(&written)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(
        value_of(&launch.env, "OPENCODE_CONFIG"),
        Some(written.to_str().expect("utf-8 path"))
    );
    assert_eq!(
        value_of(&launch.env, "TDDY_OPENCODE_PROVIDER_KEY"),
        Some("a-fireworks-key")
    );
    assert_eq!(launch.args, vec!["--model", "tddy-route/kimi-k2"]);
}

#[test]
fn an_unrouted_launch_adds_nothing_and_writes_nothing() {
    // Given
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let launch = cli_route_launch(opencode, None, session_dir.path()).expect("nothing to apply");

    // Then
    assert_eq!(launch, CliRouteLaunch::default());
    assert_eq!(
        std::fs::read_dir(session_dir.path()).expect("dir").count(),
        0
    );
}

#[test]
fn a_muse_launch_on_a_meta_route_keeps_the_sessions_model() {
    // Given — Muse Code's route is environment only: it hands over the key and names no model
    let muse = cli_harness("muse-cli").expect("registered");

    // When
    let model = launch_model_for(muse, true, "muse-wonder-1.0");

    // Then
    assert_eq!(model, "muse-wonder-1.0");
}

#[test]
fn an_opencode_launch_on_a_route_leaves_the_model_to_the_route() {
    // Given — OpenCode's route writes a config and names the model in its own `--model`
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let model = launch_model_for(opencode, true, "fireworks/kimi-k2");

    // Then
    assert_eq!(model, "");
}

#[test]
fn an_unrouted_muse_launch_keeps_the_sessions_model() {
    // Given
    let muse = cli_harness("muse-cli").expect("registered");

    // When
    let model = launch_model_for(muse, false, "muse-wonder-1.0");

    // Then
    assert_eq!(model, "muse-wonder-1.0");
}

#[test]
fn an_unrouted_opencode_launch_keeps_the_sessions_model() {
    // Given
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let model = launch_model_for(opencode, false, "fireworks/kimi-k2");

    // Then
    assert_eq!(model, "fireworks/kimi-k2");
}
