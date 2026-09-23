//! Acceptance tests: workspace session type (PRD: docs/ft/daemon/remote-codebase-mode.md).
//!
//! AC1-3, AC11: workspace session creation (no PTY), `.session.yaml` metadata, empty LiveKit
//! credentials, connect/resume short-circuit, and ExecuteTool working on the worktree.
//!
//! The exec-tool half of `tddy-session-lifecycle`'s `workspace_session_acceptance.rs`, split out
//! because the exec tools are served by `tddy-daemon-rpc`'s `ExecToolRpcHandler`. The fixture is
//! that suite's, trimmed to what these tests reach.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_PROJECT_ID: &str = "test-project";

fn write_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
"#;
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service(config: DaemonConfig, sessions_base: PathBuf) -> TestDaemon {
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
    TestDaemon::from_host(DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
    ))
}

/// Create a bare git repo that can serve as an origin for worktree creation.
fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com");
        cmd.output().expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// AC1+AC5+AC6: after creating a workspace session, `ExecuteTool("Write")` creates a file
/// in the worktree, and `ExecuteTool("Read")` on the same path returns the written content.
#[tokio::test]
async fn workspace_session_execute_tool_write_then_read_round_trips() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let start_resp = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            ..Default::default()
        }))
        .await
        .expect("StartSession workspace must succeed");
    let session_id = start_resp.get_ref().session_id.clone();

    // When — Write a file via ExecuteTool.
    let write_resp = service
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: r#"{"path":"hello.txt","contents":"hello remote world"}"#.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ExecuteTool Write must not return an RPC error");

    // Then
    assert!(
        !write_resp.get_ref().is_error,
        "Write must succeed (is_error=false), got error: {:?}",
        write_resp.get_ref().error_message
    );

    // When — Read it back via ExecuteTool.
    let read_resp = service
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.clone(),
            tool_name: "Read".to_string(),
            args_json: r#"{"path":"hello.txt"}"#.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ExecuteTool Read must not return an RPC error");

    // Then
    assert!(
        !read_resp.get_ref().is_error,
        "Read must succeed, got error: {:?}",
        read_resp.get_ref().error_message
    );

    let result: serde_json::Value = serde_json::from_str(&read_resp.get_ref().result_json)
        .expect("result_json must be valid JSON");
    let content = result
        .get("content")
        .and_then(|v| v.as_str())
        .expect("result_json must have a 'content' string field");

    assert_eq!(
        content, "hello remote world",
        "Read must return the content that was written, got: {:?}",
        content
    );
}
