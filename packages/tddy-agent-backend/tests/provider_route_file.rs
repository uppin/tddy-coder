//! The route file a tool session's `tddy-coder` is handed: round-trips a route, and is refused
//! when anyone but its owner can read it.
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-10-18-provider-route-tool-route.md

use std::os::unix::fs::PermissionsExt;

use tddy_agent_backend::provider_route::{HarnessRoute, RouteFile, RouteSecret};
use tddy_agent_backend::provider_route_file::{
    load_route_file, route_from_json, route_to_json, PROVIDER_ROUTE_FILENAME,
};

fn a_route() -> HarnessRoute {
    HarnessRoute {
        env: vec![(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://api.meta.ai".to_string(),
        )],
        secret_env: vec![(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            RouteSecret::new("a-meta-key"),
        )],
        files: vec![RouteFile {
            name: "opencode.json".to_string(),
            contents: "{}".to_string(),
        }],
        args: vec!["--model".to_string(), "muse-wonder-1.0".to_string()],
    }
}

fn written_with_mode(mode: u32) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a session directory");
    let path = dir.path().join(PROVIDER_ROUTE_FILENAME);
    std::fs::write(&path, route_to_json(&a_route())).expect("written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    (dir, path)
}

#[test]
fn a_route_round_trips_through_its_file_json() {
    // When
    let read = route_from_json(&route_to_json(&a_route())).expect("parses");

    // Then
    assert_eq!(read, a_route());
}

#[test]
fn an_owner_only_route_file_loads() {
    // Given
    let (_dir, path) = written_with_mode(0o600);

    // When
    let loaded = load_route_file(&path).expect("loads");

    // Then
    assert_eq!(loaded, a_route());
}

#[test]
fn tddy_coder_refuses_a_route_file_others_can_read() {
    // Given
    let (_dir, path) = written_with_mode(0o644);

    // When
    let refusal = load_route_file(&path).expect_err("refused");

    // Then
    assert!(refusal.contains("readable"), "{refusal}");
}
