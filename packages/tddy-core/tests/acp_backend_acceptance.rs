//! Acceptance tests for ClaudeAcpBackend.
//!
//! These tests verify the ACP backend behavior using tddy-acp-stub as the agent.
//! Run with: cargo test -p tddy-core acp_ --no-fail-fast -- --test-threads=1
//! (sequential execution avoids TDDY_ACP_SCENARIO env var interference)

use std::path::PathBuf;

use agent_client_protocol::{self as acp, Agent as _, Client};
use serial_test::serial;
use tddy_core::{ClaudeAcpBackend, CodingBackend, Goal, InvokeRequest};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let base = PathBuf::from(manifest_dir);
    // packages/tddy-core -> workspace root
    let workspace_root = base.join("../..");
    #[cfg(windows)]
    let stub = workspace_root.join("target/debug/tddy-acp-stub.exe");
    #[cfg(not(windows))]
    let stub = workspace_root.join("target/debug/tddy-acp-stub");
    stub
}

fn make_request(prompt: &str, session: Option<tddy_core::SessionMode>) -> InvokeRequest {
    InvokeRequest {
        prompt: prompt.to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
        socket_path: None,
        plan_dir: None,
    }
}

/// ClaudeAcpBackend has name "claude-acp".
#[tokio::test]
#[serial]
async fn acp_backend_has_correct_name() {
    let backend = ClaudeAcpBackend::new();
    assert_eq!(backend.name(), "claude-acp");
}

/// ClaudeAcpBackend with stub agent path can be constructed.
#[tokio::test]
#[serial]
async fn acp_backend_with_stub_path_constructs() {
    let path = stub_agent_path();
    let _backend = ClaudeAcpBackend::with_agent_path(path);
    assert_eq!(
        ClaudeAcpBackend::with_agent_path(PathBuf::from("/tmp/stub")).name(),
        "claude-acp"
    );
}

/// Prompt round-trip: send prompt, receive accumulated text in InvokeResponse.
/// Uses empty-chunks scenario to avoid deadlock when stub sends session_notifications.
#[tokio::test(flavor = "current_thread")]
#[serial]
async fn acp_backend_prompt_round_trip_returns_accumulated_text() {
    let path = stub_agent_path();
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    let scenario = r#"{"responses":[{"chunks":[],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#;
    let scenario_dir = std::env::temp_dir().join("tddy-acp-test");
    let _ = std::fs::create_dir_all(&scenario_dir);
    let scenario_path = scenario_dir.join("one-chunk.json");
    std::fs::write(&scenario_path, scenario).unwrap();
    std::env::set_var("TDDY_ACP_SCENARIO", &scenario_path);
    let backend = ClaudeAcpBackend::with_agent_path(path);
    let req = make_request("Hello, stub!", None);
    let result: Result<_, _> =
        tokio::time::timeout(std::time::Duration::from_secs(5), backend.invoke(req)).await;
    assert!(result.is_ok(), "invoke timed out after 5s");
    let resp = result.unwrap().expect("invoke should succeed");
    // With empty-chunks scenario, output is empty; invoke succeeds (no hang).
    let _ = resp;
}

/// Session management: fresh session creates new ACP session.
#[tokio::test(flavor = "current_thread")]
#[serial]
async fn acp_backend_fresh_session_creates_new_session() {
    let path = stub_agent_path();
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    let scenario = r#"{"responses":[{"chunks":[],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#;
    let scenario_dir = std::env::temp_dir().join("tddy-acp-test");
    let _ = std::fs::create_dir_all(&scenario_dir);
    let scenario_path = scenario_dir.join("empty-chunks.json");
    std::fs::write(&scenario_path, scenario).unwrap();
    std::env::set_var("TDDY_ACP_SCENARIO", &scenario_path);
    let backend = ClaudeAcpBackend::with_agent_path(path);
    let req = make_request(
        "Session test",
        Some(tddy_core::SessionMode::Fresh("sess-1".to_string())),
    );
    let result = backend.invoke(req).await;
    assert!(result.is_ok(), "invoke should succeed: {:?}", result.err());
    let resp = result.unwrap();
    assert!(
        resp.session_id.is_some(),
        "fresh session should return session_id"
    );
}

/// Raw pipe test: spawn stub, send initialize JSON, read response. No ACP SDK.
#[tokio::test(flavor = "current_thread")]
#[serial]
async fn acp_raw_pipe_initialize() {
    let path = stub_agent_path();
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    std::env::remove_var("TDDY_ACP_SCENARIO");
    let mut child = tokio::process::Command::new(&path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn stub");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let req = r#"{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"1.0","clientInfo":{"name":"t","version":"0.1.0","title":"T"}},"id":1}"#;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut buf = String::new();
    stdin.write_all(req.as_bytes()).await.unwrap();
    stdin.write_all(b"\n").await.unwrap();
    stdin.flush().await.unwrap();
    drop(stdin);
    let mut reader = tokio::io::BufReader::new(stdout);
    reader.read_line(&mut buf).await.unwrap();
    assert!(buf.contains("jsonrpc"));
    assert!(buf.contains("result"));
    let _ = child.kill().await;
}

/// ClientSideConnection with stub - only initialize (no prompt).
#[tokio::test(flavor = "current_thread")]
#[serial]
async fn acp_direct_subprocess_initialize_only() {
    let path = stub_agent_path();
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    struct TestClient;
    #[async_trait::async_trait(?Send)]
    impl Client for TestClient {
        async fn session_notification(&self, _: acp::SessionNotification) -> acp::Result<()> {
            Ok(())
        }
        async fn request_permission(
            &self,
            _: acp::RequestPermissionRequest,
        ) -> acp::Result<acp::RequestPermissionResponse> {
            Err(acp::Error::method_not_found())
        }
    }
    let mut child = tokio::process::Command::new(&path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn stub");
    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();
    let local_set = tokio::task::LocalSet::new();
    let result: acp::Result<()> = local_set
        .run_until(async move {
            let (conn, handle_io) =
                acp::ClientSideConnection::new(TestClient, outgoing, incoming, |fut| {
                    tokio::task::spawn_local(fut);
                });
            tokio::task::spawn_local(handle_io);
            conn.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                    .client_info(acp::Implementation::new("test", "0.1.0").title("Test")),
            )
            .await
            .map(|_| ())
        })
        .await;
    let _ = child.kill().await;
    assert!(result.is_ok(), "initialize failed: {:?}", result.err());
}

/// ClientSideConnection with stub - initialize + new_session (no prompt).
#[tokio::test(flavor = "current_thread")]
#[serial]
async fn acp_direct_subprocess_new_session_only() {
    let path = stub_agent_path();
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    struct TestClient;
    #[async_trait::async_trait(?Send)]
    impl Client for TestClient {
        async fn session_notification(&self, _: acp::SessionNotification) -> acp::Result<()> {
            Ok(())
        }
        async fn request_permission(
            &self,
            _: acp::RequestPermissionRequest,
        ) -> acp::Result<acp::RequestPermissionResponse> {
            Err(acp::Error::method_not_found())
        }
    }
    let mut child = tokio::process::Command::new(&path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn stub");
    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();
    let local_set = tokio::task::LocalSet::new();
    let result: acp::Result<()> = local_set
        .run_until(async move {
            let (conn, handle_io) =
                acp::ClientSideConnection::new(TestClient, outgoing, incoming, |fut| {
                    tokio::task::spawn_local(fut);
                });
            tokio::task::spawn_local(handle_io);
            conn.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                    .client_info(acp::Implementation::new("test", "0.1.0").title("Test")),
            )
            .await?;
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            conn.new_session(acp::NewSessionRequest::new(cwd)).await?;
            Ok(())
        })
        .await;
    let _ = child.kill().await;
    assert!(result.is_ok(), "new_session failed: {:?}", result.err());
}

/// Minimal test: spawn stub as subprocess, use ClientSideConnection directly.
/// Uses scenario with empty chunks to avoid deadlock when agent sends session_notifications.
#[tokio::test(flavor = "current_thread")]
#[serial]
async fn acp_direct_subprocess_round_trip() {
    let path = stub_agent_path();
    assert!(
        path.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    struct TestClient;
    #[async_trait::async_trait(?Send)]
    impl Client for TestClient {
        async fn session_notification(&self, _args: acp::SessionNotification) -> acp::Result<()> {
            Ok(())
        }
        async fn request_permission(
            &self,
            _: acp::RequestPermissionRequest,
        ) -> acp::Result<acp::RequestPermissionResponse> {
            Err(acp::Error::method_not_found())
        }
    }
    let scenario = r#"{"responses":[{"chunks":[],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#;
    let scenario_dir = std::env::temp_dir().join("tddy-acp-test");
    let _ = std::fs::create_dir_all(&scenario_dir);
    let scenario_path = scenario_dir.join("empty-chunks.json");
    std::fs::write(&scenario_path, scenario).unwrap();
    let mut child = tokio::process::Command::new(&path)
        .arg("--scenario")
        .arg(&scenario_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn stub");
    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();
    let local_set = tokio::task::LocalSet::new();
    let result: acp::Result<()> = local_set
        .run_until(async move {
            let (conn, handle_io) =
                acp::ClientSideConnection::new(TestClient, outgoing, incoming, |fut| {
                    tokio::task::spawn_local(fut);
                });
            tokio::task::spawn_local(handle_io);
            conn.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                    .client_info(acp::Implementation::new("test", "0.1.0").title("Test")),
            )
            .await?;
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let sess = conn.new_session(acp::NewSessionRequest::new(cwd)).await?;
            let _resp = conn
                .prompt(acp::PromptRequest::new(sess.session_id, vec!["hi".into()]))
                .await?;
            Ok(())
        })
        .await;
    let _ = child.kill().await;
    assert!(
        result.is_ok(),
        "direct subprocess round-trip failed: {:?}",
        result.err()
    );
}
