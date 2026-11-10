//! OpenCode as a tool-session agent over ACP, against `tddy-acp-stub` standing in for `opencode acp`.
//!
//! Feature: docs/ft/coder/opencode-acp-backend.md

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serial_test::serial;
use tddy_core::provider_route::{HarnessRoute, RouteSecret};
use tddy_core::CodingBackend;

/// What this test set for the stub (its scenario and record paths), handed to the agent on
/// purpose: an ACP agent starts from exactly its command's environment, never this process's
/// (`#agent-tools 28/28`). Read when the backend is built, so set the variables first.
fn the_stub_env() -> Vec<(String, String)> {
    ["TDDY_ACP_SCENARIO", "TDDY_ACP_STUB_RECORD"]
        .into_iter()
        .filter_map(|name| Some((name.to_string(), std::env::var(name).ok()?)))
        .collect()
}

/// A spec backend for the stub agent, as the agent's spec builds it: its routed agent (from
/// `opencode.yaml`, so a provider route is accepted) and a command that spawns the stub. The spec
/// names no hooks adapter: a route reaches the agent inline, in its environment
/// (`route.shape: inline_config`, `#agent-tools 29/32`).
fn a_spec_backend(path: std::path::PathBuf) -> tddy_core::SpecAcpBackend {
    let spec = tddy_coding_agent_specs::embedded_spec("opencode").expect("the opencode spec loads");
    assert_eq!(
        spec.acp().and_then(|acp| acp.hooks_adapter.as_deref()),
        None,
        "opencode's spec names no hooks adapter"
    );
    tddy_core::SpecAcpBackend::new(
        tddy_coding_agent_specs::routed_agent(&spec),
        tddy_core::AcpAgentCommand {
            program: path,
            // The spec's `acp.command`: the binary run as `opencode acp`.
            args: vec!["acp".to_string()],
            env: the_stub_env(),
            cwd: None,
        },
    )
}

fn stub_agent_path() -> PathBuf {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    workspace_root.join("target/debug/tddy-acp-stub")
}

fn with_an_empty_turn_scenario() {
    let dir = std::env::temp_dir().join("tddy-opencode-acp-test");
    std::fs::create_dir_all(&dir).expect("scenario dir");
    let scenario = dir.join("empty.json");
    std::fs::write(
        &scenario,
        r#"{"responses":[{"chunks":[],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#,
    )
    .unwrap();
    std::env::set_var("TDDY_ACP_SCENARIO", &scenario);
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn tddy_coder_completes_a_turn_with_opencode_over_acp() {
    // Given
    assert!(stub_agent_path().exists(), "build tddy-acp-stub first");
    with_an_empty_turn_scenario();
    let backend = a_spec_backend(stub_agent_path());

    // When
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        backend.invoke(common::stub_invoke_request("hello", "plan")),
    )
    .await
    .expect("no hang");

    // Then
    assert!(response.is_ok(), "{:?}", response.err());
    assert_eq!(backend.name(), "opencode");
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn opencode_lists_the_models_its_agent_advertises() {
    // Given
    assert!(stub_agent_path().exists(), "build tddy-acp-stub first");
    with_an_empty_turn_scenario();

    // When
    let models = a_spec_backend(stub_agent_path())
        .list_models()
        .await
        .expect("models");

    // Then
    assert!(!models.models.is_empty());
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn list_models_asks_the_opencode_binary_it_is_given() {
    // Given
    assert!(stub_agent_path().exists(), "build tddy-acp-stub first");
    with_an_empty_turn_scenario();

    // When — the backend built for the binary it is given lists that agent's models
    let catalog = a_spec_backend(stub_agent_path())
        .list_models()
        .await
        .expect("opencode is a known agent");

    // Then
    assert!(!catalog.models.is_empty());
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn a_routed_opencode_agent_gets_the_generated_config_inline_and_its_key() {
    // Given an "agent" that records its arguments and its environment, then exits; and a route as
    // OpenCode's spec renders it (`route.shape: inline_config`, `#agent-tools 29/32`): the
    // configuration inline in OPENCODE_CONFIG_CONTENT, no file
    let dir = tempfile::tempdir().expect("dir");
    let agent = dir.path().join("opencode");
    std::fs::write(
        &agent,
        format!(
            "#!/bin/sh\ncd \"{0}\"\necho \"$@\" > args.txt\nenv > env.txt\n",
            dir.path().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let route = HarnessRoute {
        env: vec![(
            "OPENCODE_CONFIG_CONTENT".to_string(),
            r#"{"model":"tddy-route/kimi"}"#.to_string(),
        )],
        secret_env: vec![(
            "OPENCODE_PROVIDER_KEY".to_string(),
            RouteSecret::new("fw-key"),
        )],
        files: Vec::new(),
        args: vec!["--model".to_string(), "tddy-route/kimi".to_string()],
    };
    let mut request = common::stub_invoke_request("hello", "plan");
    request.working_dir = Some(dir.path().to_path_buf());

    // When
    let _ = a_spec_backend(agent)
        .with_provider_route(route)
        .expect("opencode's spec declares a route, so a provider route is accepted")
        .invoke(request)
        .await;

    // Then the key and the generated configuration are in the environment, no variable names a
    // route file, and the agent is run as `opencode acp` with nothing else on its command line
    let recorded = |name: &str| std::fs::read_to_string(dir.path().join(name)).unwrap_or_default();
    let env = recorded("env.txt");
    assert!(env.contains("OPENCODE_PROVIDER_KEY=fw-key"), "{env}");
    assert!(
        env.contains(r#"OPENCODE_CONFIG_CONTENT={"model":"tddy-route/kimi"}"#),
        "{env}"
    );
    assert!(!env.contains("OPENCODE_CONFIG="), "{env}");
    assert_eq!(recorded("args.txt").trim(), "acp");
}
