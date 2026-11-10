//! Unit tests: a routed CLI launch writes its files owner-only and points the harness at them, and
//! keeps the session's model unless its route names the model itself.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md
//! Feature: docs/ft/daemon/muse-cli-session.md

use std::os::unix::fs::PermissionsExt;

use tddy_coding_agent_specs::{load_agent_registry, CodingAgentSpec};
use tddy_core::provider_route::{HarnessRoute, RouteFile, RouteSecret};

use crate::cli_route_launch::{
    cli_route_launch, launch_model_for, routed_launch_model, CliRouteLaunch,
};

/// OpenCode's spec as an operator override that routes through a JSON configuration file
/// (`route.shape: json_config_file`) rather than inline, as the built-in does.
fn opencode_routed_through_a_file() -> CodingAgentSpec {
    let tools = tempfile::tempdir().expect("a tools directory");
    let built_in_slot = "      shape: inline_config\n      config_env: OPENCODE_CONFIG_CONTENT\n";
    assert!(tddy_coding_agent_specs::OPENCODE_SPEC_YAML.contains(built_in_slot));
    std::fs::write(
        tools.path().join("opencode.yaml"),
        tddy_coding_agent_specs::OPENCODE_SPEC_YAML.replace(
            built_in_slot,
            "      shape: json_config_file\n      config_env: OPENCODE_CONFIG\n      files_dir: provider-route\n",
        ),
    )
    .expect("write the override");
    let registry = load_agent_registry(tools.path()).expect("the override loads");
    CodingAgentSpec::of(registry.get("opencode").expect("registered")).expect("a coding agent")
}

/// The built-in spec of the coding agent `id`, as a host loads it.
fn the_spec(id: &str) -> CodingAgentSpec {
    let tools = tempfile::tempdir().expect("a tools directory");
    let registry = load_agent_registry(tools.path()).expect("the built-in agents load");
    CodingAgentSpec::of(registry.get(id).expect("registered")).expect("a coding agent")
}

fn a_fireworks_opencode_route() -> HarnessRoute {
    HarnessRoute {
        env: Vec::new(),
        secret_env: vec![(
            "OPENCODE_PROVIDER_KEY".to_string(),
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
fn a_file_routed_session_on_fireworks_gets_a_config_file_naming_that_endpoint() {
    // Given — guard: `route.shape: json_config_file` still writes its file and names it
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = opencode_routed_through_a_file();

    // When
    let launch = cli_route_launch(
        &opencode,
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
        value_of(&launch.env, "OPENCODE_PROVIDER_KEY"),
        Some("a-fireworks-key")
    );
    assert_eq!(launch.args, vec!["--model", "tddy-route/kimi-k2"]);
}

#[test]
fn an_unrouted_launch_adds_nothing_and_writes_nothing() {
    // Given
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = the_spec("opencode");

    // When
    let launch = cli_route_launch(&opencode, None, session_dir.path()).expect("nothing to apply");

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
    let muse = the_spec("muse");

    // When
    let model = launch_model_for(&muse, true, "muse-wonder-1.0");

    // Then
    assert_eq!(model, "muse-wonder-1.0");
}

#[test]
fn an_opencode_launch_on_a_route_leaves_the_model_to_the_route() {
    // Given — OpenCode's route writes a config and names the model in its own `--model`
    let opencode = the_spec("opencode");

    // When
    let model = launch_model_for(&opencode, true, "fireworks/kimi-k2");

    // Then
    assert_eq!(model, "");
}

#[test]
fn an_unrouted_muse_launch_keeps_the_sessions_model() {
    // Given
    let muse = the_spec("muse");

    // When
    let model = launch_model_for(&muse, false, "muse-wonder-1.0");

    // Then
    assert_eq!(model, "muse-wonder-1.0");
}

#[test]
fn an_unrouted_opencode_launch_keeps_the_sessions_model() {
    // Given
    let opencode = the_spec("opencode");

    // When
    let model = launch_model_for(&opencode, false, "fireworks/kimi-k2");

    // Then
    assert_eq!(model, "fireworks/kimi-k2");
}

#[test]
fn a_routed_launch_whose_route_names_the_model_leaves_the_argv_without_one() {
    // Given — a dropped-in agent whose argv renders `--model {model}` and whose env route also
    // appends `--model <model>`: the launch would name the model twice
    let acme = the_spec("muse");
    let route_args = ["--model".to_string(), "pinned".to_string()];

    // When
    let (anthropic_env, codex_like, key_only) = (
        routed_launch_model(&acme, Some(&route_args), "sonnet"),
        routed_launch_model(
            &acme,
            Some(&["-m".to_string(), "pinned".to_string()]),
            "sonnet",
        ),
        routed_launch_model(&acme, Some(&[]), "sonnet"),
    );

    // Then — the route's argument is the one model on the command line, and a route that names
    // none (Muse Code's key-only route) leaves the session's own
    assert_eq!((anthropic_env, codex_like, key_only), ("", "", "sonnet"));
}

#[test]
fn an_unrouted_launch_keeps_the_sessions_model_whatever_the_agent() {
    // Given
    let muse = the_spec("muse");

    // Then
    assert_eq!(routed_launch_model(&muse, None, "sonnet"), "sonnet");
}

#[test]
fn an_inline_routed_opencode_session_writes_no_file_and_sets_no_opencode_config() {
    // Given — the built-in spec: `route.shape: inline_config`, whose route carries its configuration
    // in its environment (`#agent-tools 29/32`)
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = the_spec("opencode");
    let route = HarnessRoute {
        env: vec![(
            "OPENCODE_CONFIG_CONTENT".to_string(),
            r#"{"model":"tddy-route/kimi-k2"}"#.to_string(),
        )],
        files: Vec::new(),
        ..a_fireworks_opencode_route()
    };

    // When
    let launch =
        cli_route_launch(&opencode, Some(&route), session_dir.path()).expect("the route applies");

    // Then — guard: the configuration reaches the child inline, and nothing is written
    assert_eq!(
        (
            value_of(&launch.env, "OPENCODE_CONFIG_CONTENT"),
            value_of(&launch.env, "OPENCODE_CONFIG"),
            std::fs::read_dir(session_dir.path()).expect("dir").count(),
        ),
        (Some(r#"{"model":"tddy-route/kimi-k2"}"#), None, 0)
    );
}
