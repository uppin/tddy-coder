//! Acceptance: sandboxed `cursor-cli` sessions (`StartSession.sandbox = true`).
#![allow(dead_code, unused_imports)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectSessionRequest, ConnectionService as ConnectionServiceTrait, StartSessionRequest,
    StreamTerminalOutputRequest,
};
use tddy_testing_commons::process_is_alive;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "composer-2.5";
const TEST_PROJECT_ID: &str = "sandbox-cursor-test-project";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn write_config_with_cursor_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: /bin/true
    label: true
cursor_cli:
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
        "projects:\n  - project_id: {}\n    name: sandbox-cursor-test\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_agent.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\ncat\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

fn write_echo_argv_and_subagent_env_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_agent_subagent.sh");
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

fn sandbox_cursor_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        tool_path: String::new(),
        project_id: TEST_PROJECT_ID.to_string(),
        agent: String::new(),
        daemon_instance_id: String::new(),
        recipe: String::new(),
        session_type: "cursor-cli".to_string(),
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
        ..Default::default()
    }
}

/// **sandboxed_cursor_cli_start_persists_metadata_and_empty_livekit**: `StartSession(sandbox=true)`
/// for cursor-cli writes `sandbox: true` metadata and returns empty LiveKit fields.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn sandboxed_cursor_cli_start_persists_metadata_and_empty_livekit() {
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let resp = service
        .start_session(Request::new(sandbox_cursor_start_request()))
        .await
        .expect("sandbox cursor StartSession must succeed");
    let inner = resp.into_inner();

    assert!(inner.livekit_room.is_empty());
    assert!(inner.livekit_url.is_empty());
    assert!(inner.livekit_server_identity.is_empty());

    let session_dir = sessions_tmp.path().join("sessions").join(&inner.session_id);
    let meta = read_session_metadata(&session_dir).expect("metadata must exist");
    assert_eq!(meta.session_type.as_deref(), Some("cursor-cli"));
    assert_eq!(meta.sandbox, Some(true));
    assert_eq!(meta.model.as_deref(), Some(TEST_MODEL));
    assert!(meta.hook_token.as_deref().is_some_and(|t| !t.is_empty()));

    if let Some(pid) = meta.pid {
        assert!(
            process_is_alive(pid),
            "sandboxed cursor-cli child pid={pid} must be alive"
        );
    }
}

/// **sandboxed_cursor_cli_connect_session_returns_empty_livekit**: `ConnectSession` for a sandboxed
/// cursor-cli session returns empty LiveKit credentials.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn sandboxed_cursor_cli_connect_session_returns_empty_livekit() {
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let session_id = service
        .start_session(Request::new(sandbox_cursor_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    let connect = service
        .connect_session(Request::new(ConnectSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id,
        }))
        .await
        .expect("ConnectSession")
        .into_inner();

    assert!(connect.livekit_room.is_empty());
    assert!(connect.livekit_url.is_empty());
    assert!(connect.livekit_server_identity.is_empty());
}

/// **sandboxed_cursor_cli_terminal_io_round_trips**: daemon bridges PTY output and the stub agent
/// argv is forwarded without implicit Cursor headless MCP flags.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn sandboxed_cursor_cli_terminal_io_round_trips() {
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let session_id = service
        .start_session(Request::new(sandbox_cursor_start_request()))
        .await
        .expect("StartSession")
        .into_inner()
        .session_id;

    let stream_resp = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
            terminal_id: String::new(),
            initial_cols: 80,
            initial_rows: 24,
        }))
        .await
        .expect("stream_terminal_output");
    let mut stream = stream_resp.into_inner();

    let terminal_capture = tokio::time::timeout(Duration::from_secs(30), async {
        let mut saw_argv = false;
        let mut saw_implicit_mcp_flags = false;
        while let Some(Ok(msg)) = stream.next().await {
            let text = String::from_utf8_lossy(&msg.data);
            if text.contains("ARGV:") {
                saw_argv = true;
            }
            if text.contains("--approve-mcps")
                || text.contains("--force")
                || text.contains("--trust")
            {
                saw_implicit_mcp_flags = true;
            }
            if saw_argv {
                break;
            }
        }
        (saw_argv, saw_implicit_mcp_flags)
    })
    .await
    .unwrap_or((false, false));

    assert!(
        terminal_capture.0,
        "terminal stream must include stub agent PTY output"
    );
    assert!(
        !terminal_capture.1,
        "stub agent argv must not include implicit Cursor headless MCP flags"
    );
}

/// **sandboxed_cursor_cli_start_wires_specialized_agents_env_and_metadata**: specialized agent
/// selection persists in metadata and reaches the jailed process env.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn sandboxed_cursor_cli_start_wires_specialized_agents_env_and_metadata() {
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_and_subagent_env_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        specialized_agents: vec!["fastcontext".to_string()],
        ..sandbox_cursor_start_request()
    };

    let resp = service
        .start_session(Request::new(request))
        .await
        .expect("StartSession with specialized_agents");
    let inner = resp.into_inner();

    let session_dir = sessions_tmp.path().join("sessions").join(&inner.session_id);
    let meta = read_session_metadata(&session_dir).expect("metadata");
    assert_eq!(meta.specialized_agents, vec!["fastcontext".to_string()]);

    let stream_resp = service
        .stream_terminal_output(Request::new(StreamTerminalOutputRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: inner.session_id.clone(),
            terminal_id: String::new(),
            initial_cols: 80,
            initial_rows: 24,
        }))
        .await
        .expect("stream_terminal_output");
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
        "jailed cursor agent env must include TDDY_SUBAGENT=fastcontext"
    );
}
