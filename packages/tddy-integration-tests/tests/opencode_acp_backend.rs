//! OpenCode as a tool-session agent over ACP, against `tddy-acp-stub` standing in for `opencode acp`.
//!
//! Feature: docs/ft/coder/opencode-acp-backend.md

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serial_test::serial;
use tddy_core::backend::{resolve_agent_models, BackendCliPaths};
use tddy_core::provider_route::{HarnessRoute, RouteFile, RouteSecret};
use tddy_core::{CodingBackend, OpenCodeAcpBackend};

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
    let backend = OpenCodeAcpBackend::with_agent_path(stub_agent_path());

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
    let models = OpenCodeAcpBackend::with_agent_path(stub_agent_path())
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

    // When
    let catalog = resolve_agent_models(
        "opencode",
        &BackendCliPaths {
            opencode_cli_path: Some(stub_agent_path()),
            ..BackendCliPaths::default()
        },
    )
    .await
    .expect("opencode is a known agent");

    // Then
    assert!(!catalog.models.is_empty());
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn a_routed_opencode_agent_gets_the_generated_config_and_key() {
    // Given an "agent" that records its arguments, its environment and the configuration file it
    // was pointed at, then exits
    let dir = tempfile::tempdir().expect("dir");
    let agent = dir.path().join("opencode");
    std::fs::write(
        &agent,
        format!(
            "#!/bin/sh\ncd \"{0}\"\necho \"$@\" > args.txt\nenv > env.txt\ncat \"$OPENCODE_CONFIG\" > config.txt\n",
            dir.path().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let route = HarnessRoute {
        secret_env: vec![(
            "TDDY_OPENCODE_PROVIDER_KEY".to_string(),
            RouteSecret::new("fw-key"),
        )],
        files: vec![RouteFile {
            name: "opencode.json".to_string(),
            contents: r#"{"model":"tddy-route/kimi"}"#.to_string(),
        }],
        args: vec!["--model".to_string(), "tddy-route/kimi".to_string()],
        ..HarnessRoute::default()
    };
    let mut request = common::stub_invoke_request("hello", "plan");
    request.working_dir = Some(dir.path().to_path_buf());

    // When
    let _ = OpenCodeAcpBackend::with_agent_path(agent)
        .with_provider_route(route)
        .invoke(request)
        .await;

    // Then the key is in the environment, the generated configuration is the file OPENCODE_CONFIG
    // names, and the agent is run as `opencode acp` with nothing else on its command line
    let recorded = |name: &str| std::fs::read_to_string(dir.path().join(name)).unwrap_or_default();
    let env = recorded("env.txt");
    assert!(env.contains("TDDY_OPENCODE_PROVIDER_KEY=fw-key"), "{env}");
    assert!(env.contains("OPENCODE_CONFIG="), "{env}");
    assert_eq!(recorded("config.txt"), r#"{"model":"tddy-route/kimi"}"#);
    assert_eq!(recorded("args.txt").trim(), "acp");
}
