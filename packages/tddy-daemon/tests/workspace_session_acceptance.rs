//! Acceptance tests: workspace session type (PRD: docs/ft/daemon/remote-codebase-mode.md).
//!
//! AC1-3, AC11: workspace session creation (no PTY), `.session.yaml` metadata, empty LiveKit
//! credentials, connect/resume short-circuit, and ExecuteTool working on the worktree.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectSessionRequest, ConnectionService as ConnectionServiceTrait, ExecuteToolRequest,
    StartSessionRequest,
};

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
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
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

/// AC1: `StartSession` with `session_type:"workspace"` creates a git worktree,
/// persists `.session.yaml` with `session_type:"workspace"` and a real `repo_path`,
/// and does NOT spawn any PTY process.
#[tokio::test]
async fn workspace_session_creates_worktree_with_no_pty() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "workspace".to_string(),
            model: String::new(),
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: String::new(),
            sandbox: false,
            discovery_subagent: String::new(),
            fastcontext_url: String::new(),
            fastcontext_model: String::new(),
            fastcontext_max_turns: 0,
            subagent_replaces: String::new(),
        }))
        .await
        .expect("StartSession with session_type=workspace must succeed");

    // Then
    let session_id = &resp.get_ref().session_id;
    assert!(!session_id.is_empty(), "must return a session_id");

    // AC5 (livekit fields empty — no LiveKit for workspace):
    // workspace sessions do not need a LiveKit bridge; they are tool-only.
    assert!(
        resp.get_ref().livekit_room.is_empty(),
        "workspace session must return empty livekit_room, got {:?}",
        resp.get_ref().livekit_room
    );

    // AC1 — verify .session.yaml
    let session_dir = unified_session_dir_path(sessions_tmp.path(), session_id);
    let metadata = read_session_metadata(&session_dir)
        .expect(".session.yaml must be written after StartSession");

    assert_eq!(
        metadata.session_type.as_deref(),
        Some("workspace"),
        ".session.yaml session_type must be 'workspace', got {:?}",
        metadata.session_type
    );

    let repo_path = metadata
        .repo_path
        .as_deref()
        .expect(".session.yaml must have a repo_path");
    assert!(
        std::path::Path::new(repo_path).exists(),
        "repo_path must point to a real directory, got {:?}",
        repo_path
    );

    // No PID — workspace sessions spawn no agent process.
    assert!(
        metadata.pid.is_none(),
        "workspace session must not set a PID, got {:?}",
        metadata.pid
    );
}

/// AC2: `ConnectSession` against a workspace session returns empty LiveKit credentials
/// (there is no terminal to connect to).
#[tokio::test]
async fn connect_session_workspace_returns_empty_livekit() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let start_resp = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            ..Default::default()
        }))
        .await
        .expect("StartSession workspace must succeed");
    let session_id = start_resp.get_ref().session_id.clone();

    // When
    let connect_resp = service
        .connect_session(Request::new(ConnectSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id,
        }))
        .await
        .expect("ConnectSession workspace must succeed without error");

    // Then
    assert!(
        connect_resp.get_ref().livekit_room.is_empty(),
        "ConnectSession workspace must return empty livekit_room"
    );
    assert!(
        connect_resp.get_ref().livekit_server_identity.is_empty(),
        "ConnectSession workspace must return empty livekit_server_identity"
    );
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
        .start_session(Request::new(StartSessionRequest {
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
        .execute_tool(Request::new(ExecuteToolRequest {
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
        .execute_tool(Request::new(ExecuteToolRequest {
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
