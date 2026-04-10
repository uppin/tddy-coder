//! Acceptance tests for [`CodexAcpBackend`] (`codex-acp` over ACP, stubbed via `tddy-acp-stub`).
//!
//! Run with: `cargo build -p tddy-acp-stub` then `cargo test -p tddy-integration-tests codex_acp_ -- --test-threads=1`

mod common;

use std::path::PathBuf;

use serial_test::serial;
use tddy_core::backend::codex_acp::DEFAULT_CODEX_ACP_BINARY;
use tddy_core::CODEX_OAUTH_AUTHORIZE_URL_FILENAME;
use tddy_core::{CodexAcpBackend, CodingBackend, InvokeRequest, SessionMode};

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
    let backend = CodexAcpBackend::new();
    assert_eq!(backend.name(), "codex-acp");
}

#[tokio::test]
#[serial]
async fn codex_acp_backend_with_stub_path_constructs() {
    let path = stub_agent_path();
    let _backend = CodexAcpBackend::with_agent_path(path);
    assert_eq!(
        CodexAcpBackend::with_agent_path(PathBuf::from("/tmp/no-such-stub")).name(),
        "codex-acp"
    );
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn codex_acp_backend_fresh_session_returns_session_id() {
    let path = stub_agent_path();
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
    let backend = CodexAcpBackend::with_agent_path(path);
    let req = make_request("Hello", Some(SessionMode::Fresh("sess-1".to_string())));
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
    let path = stub_agent_path();
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
    let backend = CodexAcpBackend::with_agent_path(path);
    let req = make_request(
        "Continue",
        Some(SessionMode::Resume("thread-xyz".to_string())),
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), backend.invoke(req)).await;
    assert!(result.is_ok(), "invoke timed out");
    let resp = result.unwrap().expect("invoke");
    assert_eq!(resp.session_id.as_deref(), Some("thread-xyz"));
    assert!(resp.output.contains("resumed"), "output={:?}", resp.output);
}

#[test]
fn default_codex_acp_binary_constant_matches_cli() {
    assert_eq!(DEFAULT_CODEX_ACP_BINARY, "codex-acp");
}

#[test]
fn oauth_authorize_filename_matches_livekit_contract() {
    assert_eq!(
        CODEX_OAUTH_AUTHORIZE_URL_FILENAME,
        "codex_oauth_authorize.url"
    );
}
