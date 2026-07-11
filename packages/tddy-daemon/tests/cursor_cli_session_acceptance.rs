//! Acceptance tests: Cursor Agent CLI session type (PRD: docs/ft/daemon/cursor-cli-session.md).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_metadata::{read_session_metadata, SessionMetadata};
use tddy_daemon::claude_cli_session::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest, StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-4.6-sonnet-medium-thinking";
const TEST_PROJECT_ID: &str = "test-project";

fn write_config_with_cursor_cli_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
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
        Arc::new(CliSessionManager::new()),
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

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn start_cursor_cli_request() -> StartSessionRequest {
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
        sandbox: false,
        managed_codebase: false,
        specialized_agents: vec![],
        ..Default::default()
    }
}

#[test]
fn build_cursor_argv_includes_model_and_optional_prompt() {
    let argv = CliSessionManager::build_cursor_argv(
        "/usr/bin/agent",
        "gpt-5.3-codex",
        Some("fix the bug"),
    );
    assert_eq!(
        argv,
        vec![
            "/usr/bin/agent".to_string(),
            "--model".to_string(),
            "gpt-5.3-codex".to_string(),
            "fix the bug".to_string(),
        ]
    );
}

#[tokio::test]
async fn cursor_cli_start_with_empty_branch_name_uses_default_branch() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let mut req = start_cursor_cli_request();
    req.branch_worktree_intent = "new_branch_from_base".to_string();
    req.new_branch_name = String::new();

    let resp = service
        .start_session(Request::new(req))
        .await
        .expect("StartSession must succeed with web-form branch defaults");

    let session_id = resp.into_inner().session_id;
    let short_id = &session_id[..8.min(session_id.len())];
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let cs = tddy_core::read_changeset(&session_dir).expect("changeset must exist");
    let branch = cs
        .workflow
        .and_then(|w| w.new_branch_name)
        .expect("default branch name must be set");
    assert_eq!(branch, format!("cursor-cli/{short_id}"));
}

#[tokio::test]
async fn cursor_cli_session_metadata_fields_persisted() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let resp = service
        .start_session(Request::new(start_cursor_cli_request()))
        .await
        .expect("StartSession cursor-cli must succeed");

    let session_id = resp.into_inner().session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let meta = read_session_metadata(&session_dir).expect(".session.yaml must exist");

    assert_eq!(meta.session_type.as_deref(), Some("cursor-cli"));
    assert_eq!(meta.model.as_deref(), Some(TEST_MODEL));
    assert!(meta.hook_token.is_some());
    assert!(meta
        .repo_path
        .as_ref()
        .is_some_and(|p| PathBuf::from(p).exists()));
}

#[tokio::test]
async fn cursor_cli_session_writes_hooks_json() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let resp = service
        .start_session(Request::new(start_cursor_cli_request()))
        .await
        .expect("StartSession must succeed");

    let session_id = resp.into_inner().session_id;
    let meta =
        read_session_metadata(&sessions_tmp.path().join("sessions").join(&session_id)).unwrap();
    let worktree = PathBuf::from(meta.repo_path.unwrap());
    let hooks_path = worktree.join(".cursor/hooks.json");
    assert!(hooks_path.exists(), "hooks.json must be written");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(hooks_path).unwrap()).unwrap();
    assert_eq!(json.get("version").and_then(|v| v.as_i64()), Some(1));
    assert!(json["hooks"]["sessionStart"].is_array());
}

#[tokio::test]
async fn cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available() {
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return;
    }
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let mut req = start_cursor_cli_request();
    req.sandbox = true;
    let resp = service
        .start_session(Request::new(req))
        .await
        .expect("sandbox cursor-cli must start when sandbox backend is available");

    assert!(resp.into_inner().livekit_room.is_empty());
}

#[tokio::test]
async fn cursor_cli_session_enrichment_reads_from_metadata() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000099";
    let session_dir = sessions_tmp
        .path()
        .join("testuser")
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktree-stub".to_string()),
        pid: Some(99999),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("cursor-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        specialized_agents: Vec::new(),
    };
    tddy_core::write_session_metadata(&session_dir, &meta).unwrap();

    let (_cfg_dir, config) = write_config_with_cursor_cli_binary("/bin/cat");
    let sessions_base = sessions_tmp.path().join("testuser");
    let service = minimal_service(config, sessions_base);

    let list = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions must succeed")
        .into_inner();

    let entry = list
        .sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("session must appear in list");
    assert_eq!(entry.agent, "cursor-cli");
    assert_eq!(entry.model, TEST_MODEL);
}
