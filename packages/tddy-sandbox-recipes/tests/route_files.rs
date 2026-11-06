//! A routed jail's route files are written owner-only into its scratch, and no key shows in the
//! plan's debug output. The variable that names the first file is the launch's
//! (`coding_agent.route.config_env`), so no recipe sets it.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-sandboxed-routes.md

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tddy_core::provider_route::{HarnessRoute, RouteFile, RouteSecret};
use tddy_sandbox::{SandboxError, SandboxPlan};
use tddy_sandbox_recipes::{build_runner_plan, RunnerPlanRequest};

const KEY: &str = "a-meta-key";

fn a_route_with_a_config_file(name: &str) -> HarnessRoute {
    HarnessRoute {
        files: vec![RouteFile {
            name: name.to_string(),
            contents: r#"{"provider":"meta"}"#.to_string(),
        }],
        secret_env: vec![("META_API_KEY".to_string(), RouteSecret::new(KEY))],
        ..HarnessRoute::default()
    }
}

fn a_routed_plan(scratch: &Path, route: Option<HarnessRoute>) -> Result<SandboxPlan, SandboxError> {
    build_runner_plan(RunnerPlanRequest {
        project_root: scratch.join("repo"),
        scratch_dir: scratch.join("scratch"),
        egress_dir: scratch.join("egress"),
        profile_path: scratch.join("profile.sb"),
        runner_argv: vec!["/opt/tddy-sandbox-runner".to_string()],
        env: BTreeMap::new(),
        loopback_allow_ports: Vec::new(),
        ipc_socket: None,
        mounts: Vec::new(),
        recipe: None,
        host_home: None,
        route,
        context_dir: None,
        stdio: false,
    })
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("the path exists")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn a_routed_session_writes_its_files_owner_only() {
    // Given
    let dir = tempfile::tempdir().expect("dir");

    // When
    a_routed_plan(
        dir.path(),
        Some(a_route_with_a_config_file("opencode.json")),
    )
    .expect("a plan");

    // Then
    let route_dir = dir.path().join("scratch/provider-route");
    let config = route_dir.join("opencode.json");
    assert_eq!(
        std::fs::read_to_string(&config).expect("the config is written"),
        r#"{"provider":"meta"}"#
    );
    assert_eq!(mode_of(&route_dir), 0o700);
    assert_eq!(mode_of(&config), 0o600);
}

#[test]
fn a_route_file_named_outside_its_directory_is_refused_before_anything_is_written() {
    // Given
    let dir = tempfile::tempdir().expect("dir");

    // When
    let refusal = a_routed_plan(
        dir.path(),
        Some(a_route_with_a_config_file("../escape.json")),
    )
    .expect_err("a name that leaves the route directory is refused");

    // Then
    assert!(
        matches!(refusal, SandboxError::InvalidSpec(_)),
        "{refusal:?}"
    );
    assert!(!dir.path().join("scratch/provider-route").exists());
    assert!(!dir.path().join("scratch/escape.json").exists());
}

#[test]
fn a_plan_built_again_for_the_same_session_replaces_its_route_files() {
    // Given a session whose plan has already written its route files
    let dir = tempfile::tempdir().expect("dir");
    a_routed_plan(
        dir.path(),
        Some(a_route_with_a_config_file("opencode.json")),
    )
    .expect("a plan");

    // When the session is relaunched and its plan built again
    a_routed_plan(
        dir.path(),
        Some(a_route_with_a_config_file("opencode.json")),
    )
    .expect("a second plan");

    // Then
    let files: Vec<_> = std::fs::read_dir(dir.path().join("scratch/provider-route"))
        .expect("the route files directory")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(files, vec![std::ffi::OsString::from("opencode.json")]);
}

#[test]
fn a_routed_plan_shows_no_key_in_its_debug_output() {
    // Given
    let dir = tempfile::tempdir().expect("dir");

    // When
    let plan = a_routed_plan(
        dir.path(),
        Some(a_route_with_a_config_file("opencode.json")),
    )
    .expect("a plan");

    // Then
    assert!(!format!("{plan:?}").contains(KEY));
}
