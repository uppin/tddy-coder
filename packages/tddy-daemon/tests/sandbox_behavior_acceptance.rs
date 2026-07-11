//! Acceptance: sandboxed claude-cli behavior with `tddy-demo-tui` and SessionChannel LLM egress.
//!
//! macOS-only integration tests. Uses the same fake claude binary as Cypress e2e
//! (`tddy-demo-tui`) plus an egress probe script for SessionChannel relay assertions.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serial_test::serial;
use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_sandbox::SANDBOX_SPAWN_MANIFEST;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest, StreamTerminalOutputRequest,
};
use tddy_testing_commons::{
    process_is_alive, write_egress_probe_claude_script, EGRESS_PROBE_DIRECT_DENIED,
    EGRESS_PROBE_SESSION_CHANNEL_OK,
};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "sandbox-behavior-project";
const TERMINAL_POLL: Duration = Duration::from_millis(200);
const TERMINAL_DEADLINE: Duration = Duration::from_secs(15);

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn tddy_tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

fn demo_tui_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-demo-tui")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-demo-tui")
        })
}

fn write_config_with_claude_cli_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let tddy_tools = tddy_tools_binary();
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: /bin/true
    label: true
claude_cli:
  binary_path: {stub_binary}
  tddy_tools_path: {tddy_tools}
"#,
        tddy_tools = tddy_tools.display()
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(dir);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("git command failed");
    };
    let author_env = &[
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "t@t.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "t@t.com"),
    ];
    run(&["init", "-b", "main"], &[]);
    run(&["config", "user.email", "t@t.com"], &[]);
    run(&["config", "user.name", "Test"], &[]);
    run(&["commit", "--allow-empty", "-m", "init"], author_env);
    run(&["remote", "add", "origin", dir.to_str().unwrap()], &[]);
    run(&["push", "-u", "origin", "main"], &[]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: sandbox-behavior\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn sandbox_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        tool_path: String::new(),
        project_id: TEST_PROJECT_ID.to_string(),
        agent: String::new(),
        daemon_instance_id: String::new(),
        recipe: String::new(),
        model: TEST_MODEL.to_string(),
        session_type: "claude-cli".to_string(),
        branch_worktree_intent: String::new(),
        new_branch_name: String::new(),
        selected_integration_base_ref: String::new(),
        selected_branch_to_work_on: String::new(),
        initial_prompt: String::new(),
        permission_mode: String::new(),
        stack_parent: String::new(),
        sandbox: true,
        managed_codebase: false,
        specialized_agents: vec![],
        ..Default::default()
    }
}

async fn collect_terminal_text_until(
    service: &ConnectionServiceImpl,
    session_id: &str,
    deadline: Duration,
    needle: &str,
) -> String {
    let resp = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
            terminal_id: "main".to_string(),
            initial_cols: 80,
            initial_rows: 24,
        }))
        .await
        .expect("StreamTerminalOutput must succeed");
    let mut stream = resp.into_inner();
    let mut collected = String::new();
    let end = tokio::time::Instant::now() + deadline;
    while tokio::time::Instant::now() < end {
        if let Ok(Some(Ok(msg))) = tokio::time::timeout(TERMINAL_POLL, stream.next()).await {
            collected.push_str(&String::from_utf8_lossy(&msg.data));
            if collected.contains(needle) {
                return collected;
            }
        }
    }
    collected
}

async fn spawn_llm_echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind echo server");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let response =
                    b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\nLLM_ECHO\n";
                let _ = stream.write_all(response).await;
            });
        }
    });
    port
}

fn read_spawn_manifest(session_dir: &Path) -> serde_json::Value {
    let path = session_dir.join("egress").join(SANDBOX_SPAWN_MANIFEST);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("spawn manifest must exist at {}: {e}", path.display()));
    serde_json::from_str(&text).expect("spawn manifest json")
}

fn configure_egress_probe_env(echo_port: u16) {
    std::env::set_var("TDDY_EGRESS_PROBE_HOST", "127.0.0.1");
    std::env::set_var("TDDY_EGRESS_PROBE_PORT", echo_port.to_string());
    std::env::set_var(
        "TDDY_EGRESS_PROBE_URL",
        format!("http://127.0.0.1:{echo_port}/llm"),
    );
}

/// **sandboxed_session_streams_demo_tui_dimensions_in_terminal**: `StartSession(sandbox=true)`
/// with `tddy-demo-tui` (Cypress e2e fake claude) streams `DEMO TUI W=` PTY output to the host.
#[tokio::test]
#[serial]
async fn sandboxed_session_streams_demo_tui_dimensions_in_terminal() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let demo_tui = demo_tui_binary();
    assert!(
        demo_tui.exists(),
        "build tddy-demo-tui before running this test"
    );
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(demo_tui.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    let terminal_text =
        collect_terminal_text_until(&service, &session_id, TERMINAL_DEADLINE, "DEMO TUI W=").await;

    // Then
    assert!(
        terminal_text.contains("DEMO TUI W="),
        "sandbox PTY must include demo-tui dimension banner, got:\n{terminal_text}"
    );
}

/// **sandboxed_session_spawn_manifest_records_session_channel_egress**: spawn manifest declares
/// denied jail network and SessionChannel as the only egress path.
#[tokio::test]
#[serial]
async fn sandboxed_session_spawn_manifest_records_session_channel_egress() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let demo_tui = demo_tui_binary();
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(demo_tui.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);

    // Then
    let manifest = read_spawn_manifest(&session_dir);
    assert_eq!(
        manifest.get("egress_via").and_then(|v| v.as_str()),
        Some("session_channel"),
        "spawn manifest must record SessionChannel egress, manifest={manifest}"
    );
    assert_eq!(
        manifest.get("network_policy").and_then(|v| v.as_str()),
        Some("deny"),
        "spawn manifest must record denied jail network, manifest={manifest}"
    );
}

/// **sandboxed_session_relays_claude_llm_egress_via_session_channel**: in-jail claude reaches
/// a host echo server only when the daemon relays `EgressRequest` on SessionChannel.
#[tokio::test]
#[serial]
async fn sandboxed_session_relays_claude_llm_egress_via_session_channel() {
    // Given
    let echo_port = spawn_llm_echo_server().await;
    configure_egress_probe_env(echo_port);

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let probe_claude = write_egress_probe_claude_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(probe_claude.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    let terminal_text = collect_terminal_text_until(
        &service,
        &session_id,
        TERMINAL_DEADLINE,
        EGRESS_PROBE_SESSION_CHANNEL_OK,
    )
    .await;

    // Then
    assert!(
        terminal_text.contains(EGRESS_PROBE_SESSION_CHANNEL_OK),
        "claude LLM egress must relay via SessionChannel, got:\n{terminal_text}"
    );
}

/// **sandboxed_session_denies_direct_outbound_network_from_jail**: direct TCP from the jail to
/// the LLM endpoint is denied; SessionChannel relay remains the only successful egress path.
#[tokio::test]
#[serial]
async fn sandboxed_session_denies_direct_outbound_network_from_jail() {
    // Given
    let echo_port = spawn_llm_echo_server().await;
    configure_egress_probe_env(echo_port);

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let probe_claude = write_egress_probe_claude_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(probe_claude.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    // The probe emits `direct=denied` first, then (after the curl relay) `session_channel=ok`
    // last. Wait on the *last* marker so the collected text contains both — otherwise the
    // collector returns on `direct=denied` and truncates before the SessionChannel line.
    let terminal_text = collect_terminal_text_until(
        &service,
        &session_id,
        TERMINAL_DEADLINE,
        EGRESS_PROBE_SESSION_CHANNEL_OK,
    )
    .await;

    // Then
    assert!(
        terminal_text.contains(EGRESS_PROBE_DIRECT_DENIED),
        "direct outbound network must be denied in the jail, got:\n{terminal_text}"
    );
    assert!(
        terminal_text.contains(EGRESS_PROBE_SESSION_CHANNEL_OK),
        "SessionChannel egress must still succeed, got:\n{terminal_text}"
    );
}

/// **sandboxed_session_child_is_alive_after_demo_tui_start**: sandbox child remains running while
/// demo-tui draws inside the jail (smoke check for seatbelt exec of the e2e fake claude binary).
#[tokio::test]
#[serial]
async fn sandboxed_session_child_is_alive_after_demo_tui_start() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let demo_tui = demo_tui_binary();
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(demo_tui.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let pid = tddy_core::session_metadata::read_session_metadata(&session_dir)
        .expect("metadata")
        .pid
        .expect("pid");

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Then
    assert!(
        process_is_alive(pid),
        "sandbox child must stay alive with demo-tui as claude binary (pid={pid})"
    );
}
