//! Acceptance tests for the spec ACP backend (`codex-acp` over ACP, stubbed via `tddy-acp-stub`).
//!
//! Run with: `cargo build -p tddy-acp-stub` then `cargo test -p tddy-integration-tests codex_acp_ -- --test-threads=1`

mod common;

use std::path::PathBuf;

use serial_test::serial;
use tddy_core::CODEX_OAUTH_AUTHORIZE_URL_FILENAME;
use tddy_core::{CodingBackend, InvokeRequest, SessionMode};

/// What this test set for the stub (its scenario and record paths), handed to the agent on
/// purpose: an ACP agent starts from exactly its command's environment, never this process's
/// (`#agent-tools 28/28`). Read when the backend is built, so set the variables first.
fn the_stub_env() -> Vec<(String, String)> {
    ["TDDY_ACP_SCENARIO", "TDDY_ACP_STUB_RECORD"]
        .into_iter()
        .filter_map(|name| Some((name.to_string(), std::env::var(name).ok()?)))
        .collect()
}

/// A spec backend for the stub agent, as the agent's spec builds it: its routed agent and a
/// command that spawns the stub.
fn a_spec_backend(path: std::path::PathBuf) -> tddy_core::SpecAcpBackend {
    tddy_core::SpecAcpBackend::new(
        tddy_core::provider_route::RoutedAgent {
            id: "codex-acp".to_string(),
            product: "Codex".to_string(),
            support: tddy_core::provider_route::RouteSupport::default(),
            key_env: None,
        },
        tddy_core::AcpAgentCommand {
            program: path,
            args: Vec::new(),
            env: the_stub_env(),
            cwd: None,
        },
    )
}

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let base = PathBuf::from(manifest_dir);
    let workspace_root = base.join("../..");
    #[cfg(windows)]
    let stub = workspace_root.join("target/debug/tddy-acp-stub.exe");
    #[cfg(not(windows))]
    let stub = workspace_root.join("target/debug/tddy-acp-stub");
    stub
}

fn make_request(prompt: &str, session: Option<SessionMode>) -> InvokeRequest {
    let mut req = common::stub_invoke_request(prompt, "plan");
    req.session = session;
    req
}

#[tokio::test]
#[serial]
async fn codex_acp_backend_has_correct_name() {
    // Given
    let backend = a_spec_backend(std::path::PathBuf::from("codex-acp"));

    // Then
    assert_eq!(backend.name(), "codex-acp");
}

#[tokio::test]
#[serial]
async fn codex_acp_backend_with_stub_path_constructs() {
    // Given
    let path = stub_agent_path();
    let _backend = a_spec_backend(path);

    // Then
    assert_eq!(
        a_spec_backend(PathBuf::from("/tmp/no-such-stub")).name(),
        "codex-acp"
    );
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn codex_acp_backend_fresh_session_returns_session_id() {
    // Given
    let path = stub_agent_path();

    // Then
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    let scenario = r#"{"responses":[{"chunks":[],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#;
    let scenario_dir = std::env::temp_dir().join("tddy-codex-acp-test");
    let _ = std::fs::create_dir_all(&scenario_dir);
    let scenario_path = scenario_dir.join("empty.json");
    std::fs::write(&scenario_path, scenario).unwrap();
    std::env::set_var("TDDY_ACP_SCENARIO", &scenario_path);
    let backend = a_spec_backend(path);
    let req = make_request("Hello", Some(SessionMode::Fresh("sess-1".to_string())));

    // When
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), backend.invoke(req)).await;
    assert!(result.is_ok(), "invoke timed out");
    let resp = result.unwrap().expect("invoke");
    assert!(
        resp.session_id.is_some(),
        "expected session_id from stub new_session"
    );
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn codex_acp_backend_resume_uses_load_session() {
    // Given
    let path = stub_agent_path();

    // Then
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    let scenario = r#"{"responses":[{"chunks":["resumed"],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#;
    let scenario_dir = std::env::temp_dir().join("tddy-codex-acp-resume");
    let _ = std::fs::create_dir_all(&scenario_dir);
    let scenario_path = scenario_dir.join("resume.json");
    std::fs::write(&scenario_path, scenario).unwrap();
    std::env::set_var("TDDY_ACP_SCENARIO", &scenario_path);
    let backend = a_spec_backend(path);
    let req = make_request(
        "Continue",
        Some(SessionMode::Resume("thread-xyz".to_string())),
    );

    // When
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), backend.invoke(req)).await;
    assert!(result.is_ok(), "invoke timed out");
    let resp = result.unwrap().expect("invoke");
    assert_eq!(resp.session_id.as_deref(), Some("thread-xyz"));
    assert!(resp.output.contains("resumed"), "output={:?}", resp.output);
}

#[test]
fn default_codex_acp_binary_constant_matches_cli() {
    // Given — the binary name is the spec's `binary.command` (`codex-acp.yaml`), not a constant

    // Then
    assert_eq!(
        tddy_coding_agent_specs::CodingAgentRegistry::builtin_cached()
            .tools()
            .get("codex-acp")
            .map(|tool| tool.binary.command.as_str()),
        Some("codex-acp")
    );
}

#[test]
fn oauth_authorize_filename_matches_livekit_contract() {
    // Given

    // Then
    assert_eq!(
        CODEX_OAUTH_AUTHORIZE_URL_FILENAME,
        "codex_oauth_authorize.url"
    );
}
