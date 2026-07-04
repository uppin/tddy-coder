//! Acceptance: sandboxed `claude-cli` sessions (`StartSession.sandbox = true`).
//!
//! macOS-only integration tests use Seatbelt + `tddy-tools sandbox-runner`.
//! Non-darwin platforms get `failed_precondition` without fallback.
//!
//! Several imports and helpers here are consumed only by the `#[cfg(target_os = "macos")]`
//! Seatbelt tests, so they read as unused when building for other targets. Relax those lints
//! file-wide rather than scatter per-item `cfg_attr`s across a platform-multiplexed test file.
#![allow(dead_code, unused_imports)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
#[cfg(not(target_os = "macos"))]
use tddy_rpc::Code;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectSessionRequest, ConnectionService as ConnectionServiceTrait, StartSessionRequest,
    StreamTerminalOutputRequest,
};
use tddy_testing_commons::process_is_alive;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "sandbox-test-project";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn write_config_with_claude_cli_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
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
"#
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

fn create_test_repo_with_origin(dir: &std::path::Path) {
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

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: sandbox-test\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\ncat\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

/// Like [`write_echo_argv_script`], but also echoes `TDDY_SUBAGENT` so a test can confirm the
/// specialized-agent env overlay actually reached the spawned process inside the jail.
fn write_echo_argv_and_subagent_env_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude_subagent.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\necho \"ARGV: $@\"\necho \"TDDY_SUBAGENT=$TDDY_SUBAGENT\"\ncat\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

fn sandbox_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        tool_path: String::new(),
        project_id: TEST_PROJECT_ID.to_string(),
        agent: String::new(),
        daemon_instance_id: String::new(),
        recipe: String::new(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
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
    }
}

/// **start_session_sandbox_unsupported_on_non_darwin**: requesting a sandboxed claude-cli session
/// on non-macOS hosts returns `failed_precondition` — no silent fallback to unsandboxed spawn.
#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn start_session_sandbox_unsupported_on_non_darwin() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("claude");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let err = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect_err("sandbox StartSession must fail on non-darwin");

    // Then
    assert_eq!(
        err.code,
        Code::FailedPrecondition,
        "unsupported sandbox must map to failed_precondition"
    );
    assert!(
        err.message.contains("sandbox unsupported"),
        "error must explain sandbox is unavailable: {}",
        err.message
    );
}

/// **sandboxed_claude_cli_start_persists_metadata_and_empty_livekit**: `StartSession(sandbox=true)`
/// writes `sandbox: true` metadata and returns empty LiveKit fields.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn sandboxed_claude_cli_start_persists_metadata_and_empty_livekit() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("sandbox StartSession must succeed on darwin");
    let inner = resp.into_inner();

    // Then — response
    assert!(
        inner.livekit_room.is_empty() && inner.livekit_url.is_empty(),
        "sandboxed claude-cli must not allocate LiveKit"
    );

    let session_dir = sessions_tmp.path().join("sessions").join(&inner.session_id);
    let meta = read_session_metadata(&session_dir).expect(".session.yaml must exist");
    assert_eq!(meta.sandbox, Some(true));
    assert_eq!(meta.session_type.as_deref(), Some("claude-cli"));
    assert!(meta.pid.is_some());
    assert!(process_is_alive(meta.pid.unwrap()));
}

/// **sandboxed_claude_cli_starts_on_linux_with_the_cgroups_backend**: on Linux,
/// `StartSession(sandbox=true)` runs the runner under the rootless cgroups backend, returns empty
/// LiveKit fields, and persists `sandbox: true` metadata. Requires a host with unprivileged user
/// namespaces (the Linux analogue of the macOS Seatbelt tests above).
///
/// NOTE for green: once Linux is supported, `start_session_sandbox_unsupported_on_non_darwin` must
/// be re-gated to exclude linux (e.g. `cfg(not(any(target_os = "macos", target_os = "linux")))`).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn sandboxed_claude_cli_starts_on_linux_with_the_cgroups_backend() {
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        eprintln!(
            "SKIP: host forbids unprivileged user namespaces (cannot create the sandbox here)"
        );
        return;
    }

    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("sandbox StartSession must succeed on linux with the cgroups backend");
    let inner = resp.into_inner();

    // Then
    assert!(
        inner.livekit_room.is_empty() && inner.livekit_url.is_empty(),
        "sandboxed claude-cli must not allocate LiveKit"
    );
    let session_dir = sessions_tmp.path().join("sessions").join(&inner.session_id);
    let meta = read_session_metadata(&session_dir).expect(".session.yaml must exist");
    assert_eq!(meta.sandbox, Some(true));
    assert_eq!(meta.session_type.as_deref(), Some("claude-cli"));
    assert!(meta.pid.is_some());
    assert!(process_is_alive(meta.pid.unwrap()));
}

/// **sandboxed_claude_cli_connect_session_returns_empty_livekit**: `ConnectSession` for a sandboxed
/// session returns empty LiveKit credentials.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn sandboxed_claude_cli_connect_session_returns_empty_livekit() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    // When
    let connect = service
        .connect_session(Request::new(ConnectSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("ConnectSession must succeed")
        .into_inner();

    // Then
    assert!(connect.livekit_room.is_empty());
    assert!(connect.livekit_url.is_empty());
    assert!(connect.livekit_server_identity.is_empty());
}

/// **sandboxed_claude_cli_terminal_io_round_trips**: daemon bridges PTY output from the sandbox
/// runner and accepts keystrokes via `SendSandboxInput`.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn sandboxed_claude_cli_terminal_io_round_trips() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    // When — stream terminal output
    let stream_resp = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
            terminal_id: String::new(),
            initial_cols: 80,
            initial_rows: 24,
        }))
        .await
        .expect("stream_terminal_output must succeed for sandbox session");
    let mut stream = stream_resp.into_inner();

    let terminal_capture = tokio::time::timeout(Duration::from_secs(30), async {
        let mut saw_argv = false;
        let mut saw_mcp_allowlist = false;
        while let Some(Ok(msg)) = stream.next().await {
            let text = String::from_utf8_lossy(&msg.data);
            if text.contains("ARGV:") {
                saw_argv = true;
            }
            if text.contains("--allowedTools")
                && text.contains("mcp__tddy-tools__Read")
                && text.contains("--mcp-config")
                && text.contains("--permission-prompt-tool")
            {
                saw_mcp_allowlist = true;
            }
            if saw_argv && saw_mcp_allowlist {
                break;
            }
        }
        (saw_argv, saw_mcp_allowlist)
    })
    .await
    .unwrap_or((false, false));

    // Then
    assert!(
        terminal_capture.0,
        "terminal stream must include stub claude PTY output"
    );
    assert!(
        terminal_capture.1,
        "stub claude argv must include MCP allowlist flags (mcp__tddy-tools__Read, --mcp-config)"
    );
}

/// **sandboxed_claude_cli_tool_exec_via_ipc_reads_host_worktree**: tool IPC inside the sandbox
/// forwards `Read` to the daemon, which executes against the host git worktree.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn sandboxed_claude_cli_tool_exec_via_ipc_reads_host_worktree() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    std::fs::write(repo_dir.path().join("README.md"), "host-worktree-contents").unwrap();
    let git_env = [
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "t@t.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "t@t.com"),
    ];
    for args in [
        &["add", "README.md"][..],
        &["commit", "-m", "add readme"][..],
    ] {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(repo_dir.path());
        for (k, v) in git_env {
            cmd.env(k, v);
        }
        cmd.output().expect("git commit README for worktree");
    }
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let session_id = service
        .start_session(Request::new(sandbox_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    let tool_ipc = tddy_sandbox::SandboxSpec::short_ipc_socket_path(&session_id);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while !tool_ipc.exists() {
        if tokio::time::Instant::now() >= deadline {
            panic!("tool IPC socket never appeared at {}", tool_ipc.display());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // When — invoke Read via the same IPC path MCP uses in the sandbox
    std::env::set_var("TDDY_SANDBOX_TOOL_IPC", &tool_ipc);
    let args = serde_json::json!({
        "path": "README.md"
    });
    let raw = tddy_tools::session_tool_client::dispatch_session_tool("Read", args).await;

    // Then — dispatch_session_tool returns tool result JSON directly on success
    let result: serde_json::Value = serde_json::from_str(&raw).expect("valid tool result json");
    assert_eq!(
        result.get("content").and_then(|v| v.as_str()),
        Some("host-worktree-contents"),
        "Read must return host worktree file contents, got: {result}"
    );
}

/// **sandboxed_claude_cli_start_wires_specialized_agents_env_and_metadata**: a
/// `StartSession(sandbox=true, specialized_agents=["fastcontext"])` request threads the resolved
/// agent through to (a) the persisted `.session.yaml` and (b) the actual process env the spawned
/// runner exposes to the jailed `claude` process — proving the proto field → resolved YAML def →
/// `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` env wiring end to end (see
/// docs/ft/coder/managed-codebase-subagents.md § Tool replacement).
#[cfg(target_os = "macos")]
#[tokio::test]
async fn sandboxed_claude_cli_start_wires_specialized_agents_env_and_metadata() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_and_subagent_env_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        specialized_agents: vec!["fastcontext".to_string()],
        ..sandbox_start_request()
    };

    // When
    let resp = service
        .start_session(Request::new(request))
        .await
        .expect("sandbox StartSession with specialized_agents must succeed on darwin");
    let inner = resp.into_inner();

    // Then — persisted metadata
    let session_dir = sessions_tmp.path().join("sessions").join(&inner.session_id);
    let meta = read_session_metadata(&session_dir).expect(".session.yaml must exist");
    assert_eq!(meta.specialized_agents, vec!["fastcontext".to_string()]);

    // Then — the jailed process actually received TDDY_SUBAGENT in its env
    let stream_resp = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: inner.session_id.clone(),
            terminal_id: String::new(),
            initial_cols: 80,
            initial_rows: 24,
        }))
        .await
        .expect("stream_terminal_output must succeed for sandbox session");
    let mut stream = stream_resp.into_inner();

    let saw_subagent_env = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(Ok(msg)) = stream.next().await {
            let text = String::from_utf8_lossy(&msg.data);
            if text.contains("TDDY_SUBAGENT=fastcontext") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);

    assert!(
        saw_subagent_env,
        "jailed process env must include TDDY_SUBAGENT=fastcontext"
    );
}
