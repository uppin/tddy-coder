//! A routed jail plan carries the account key as a secret reference, never in the jail's
//! environment, and injects no host OAuth token beside it.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-sandboxed-routes.md

use std::collections::BTreeMap;

use tddy_core::provider_route::{HarnessRoute, RouteSecret};
use tddy_sandbox_recipes::{build_runner_plan, RunnerPlanRequest, SandboxRecipe};

fn a_meta_route() -> HarnessRoute {
    HarnessRoute {
        env: vec![(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://api.meta.ai".to_string(),
        )],
        secret_env: vec![(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            RouteSecret::new("a-meta-key"),
        )],
        ..HarnessRoute::default()
    }
}

fn a_claude_cli_plan(route: Option<HarnessRoute>) -> tddy_sandbox::SandboxPlan {
    let dir = tempfile::tempdir().expect("dir");
    build_runner_plan(RunnerPlanRequest {
        project_root: dir.path().join("repo"),
        scratch_dir: dir.path().join("scratch"),
        egress_dir: dir.path().join("egress"),
        profile_path: dir.path().join("profile.sb"),
        runner_argv: vec![
            "/opt/tddy-sandbox-runner".to_string(),
            "--claude-binary".to_string(),
            "/opt/claude".to_string(),
        ],
        env: BTreeMap::new(),
        loopback_allow_ports: Vec::new(),
        ipc_socket: None,
        mounts: Vec::new(),
        recipe: Some(SandboxRecipe::ClaudeCli),
        host_home: None,
        route,
    })
    .expect("a plan")
}

#[test]
fn a_routed_jail_plan_carries_the_key_as_a_secret_ref_not_in_env() {
    // When
    let plan = a_claude_cli_plan(Some(a_meta_route()));

    // Then
    let env = &plan.spec.env;
    assert!(
        env.contains_key("TDDY_SECRET_ANTHROPIC_AUTH_TOKEN"),
        "{env:?}"
    );
    assert!(
        env.values().all(|v| v != "a-meta-key"),
        "the key is never a jail env value"
    );
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").map(String::as_str),
        Some("https://api.meta.ai")
    );
}

#[test]
fn a_routed_jail_does_not_inject_the_oauth_token() {
    // Given a daemon that exports a Claude OAuth token
    let _env = tddy_testing_commons::process_env::ProcessEnv::lock()
        .setting("CLAUDE_CODE_OAUTH_TOKEN", "oauth-from-the-daemon");

    // When
    let routed = a_claude_cli_plan(Some(a_meta_route()));
    let unrouted = a_claude_cli_plan(None);

    // Then
    assert!(!routed
        .spec
        .env
        .contains_key("TDDY_SECRET_CLAUDE_CODE_OAUTH_TOKEN"));
    assert!(unrouted
        .spec
        .env
        .contains_key("TDDY_SECRET_CLAUDE_CODE_OAUTH_TOKEN"));
}
