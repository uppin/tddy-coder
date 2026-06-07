//! Acceptance tests: Claude Code CLI session type (PRD: docs/ft/daemon/claude-cli-session.md).
//!
//! These tests define the desired behaviour of `session_type = "claude-cli"` sessions:
//! session metadata persistence, enrichment without changeset.yaml, LiveKit-free responses,
//! and resume in the existing worktree.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_metadata::{
    read_session_metadata, write_session_metadata, SessionMetadata,
};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::user_sessions_path::TDDY_PROJECTS_DIR_ENV;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest, ResumeSessionRequest,
    StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";

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
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(config, sessions_base_resolver, user_resolver, None, None, None)
}

/// Create a git repo with an origin remote pointing at itself so that
/// `git fetch origin` / `setup_worktree_for_session_with_optional_chain_base` succeed.
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
    // Add the repo itself as origin so git fetch origin works without a real server.
    run(
        &["remote", "add", "origin", dir.to_str().unwrap()],
        &[],
    );
    run(&["push", "-u", "origin", "main"], &[]);
}

/// Write a `projects.yaml` in `projects_dir` registering the given repo as TEST_PROJECT_ID.
fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// **claude_cli_session_metadata_fields_persisted**: after `StartSession` with
/// `session_type = "claude-cli"` succeeds, `.session.yaml` under the new session directory must
/// contain `session_type = "claude-cli"` and `model = TEST_MODEL`. The `repo_path` must point to
/// a real linked git worktree of the project repo (visible in `git worktree list`).
#[tokio::test]
#[serial_test::serial]
async fn claude_cli_session_metadata_fields_persisted() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    // Register the project and override the projects path via env var.
    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    std::env::set_var(TDDY_PROJECTS_DIR_ENV, projects_tmp.path());
    let _restore = scopeguard::guard((), |_| std::env::remove_var(TDDY_PROJECTS_DIR_ENV));

    let sessions_tmp = tempfile::tempdir().unwrap();
    // `/bin/cat` as a stub for `claude` — works in a PTY without the real binary.
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let resp = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(), // default: new_branch_from_base with generated name
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
        }))
        .await
        .expect("StartSession with session_type=claude-cli must succeed");

    let session_id = resp.into_inner().session_id;
    assert!(!session_id.is_empty(), "session_id must be non-empty");

    // sessions_base_resolver returns sessions_tmp.path() directly (no username segment);
    // the daemon appends SESSIONS_SUBDIR ("sessions") and session_id.
    let session_dir = sessions_tmp
        .path()
        .join("sessions")
        .join(&session_id);
    let meta = read_session_metadata(&session_dir)
        .expect(".session.yaml must be written for claude-cli session");

    assert_eq!(
        meta.session_type.as_deref(),
        Some("claude-cli"),
        "session_type must be persisted as 'claude-cli'"
    );
    assert_eq!(
        meta.model.as_deref(),
        Some(TEST_MODEL),
        "model must be persisted in .session.yaml"
    );
    assert!(
        meta.repo_path.is_some(),
        "repo_path must be set to the created worktree path"
    );
    let worktree_path = PathBuf::from(meta.repo_path.unwrap());
    assert!(
        worktree_path.exists(),
        "worktree directory must exist at repo_path: {}",
        worktree_path.display()
    );

    // Assert it is a real git worktree (appears in `git worktree list` for the project repo).
    let wt_list = std::process::Command::new("git")
        .args(["worktree", "list"])
        .current_dir(repo_dir.path())
        .output()
        .expect("git worktree list must run");
    let wt_stdout = String::from_utf8_lossy(&wt_list.stdout);
    assert!(
        wt_stdout
            .lines()
            .any(|l| l.starts_with(worktree_path.to_str().unwrap())),
        "worktree must appear in 'git worktree list' for the project repo;\n\
         worktree_path={}\ngit worktree list:\n{}",
        worktree_path.display(),
        wt_stdout
    );
}

/// **claude_cli_session_livekit_fields_empty**: `StartSessionResponse` for
/// `session_type = "claude-cli"` must return empty `livekit_room`, `livekit_url`, and
/// `livekit_server_identity` — no LiveKit room is created for these sessions.
#[tokio::test]
#[serial_test::serial]
async fn claude_cli_session_livekit_fields_empty() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let projects_tmp = tempfile::tempdir().unwrap();
    register_project(projects_tmp.path(), repo_dir.path());
    std::env::set_var(TDDY_PROJECTS_DIR_ENV, projects_tmp.path());
    let _restore = scopeguard::guard((), |_| std::env::remove_var(TDDY_PROJECTS_DIR_ENV));

    let sessions_tmp = tempfile::tempdir().unwrap();
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let inner = service
        .start_session(Request::new(StartSessionRequest {
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
        }))
        .await
        .expect("StartSession must succeed")
        .into_inner();

    assert!(
        inner.livekit_room.is_empty(),
        "livekit_room must be empty for claude-cli sessions; got: {}",
        inner.livekit_room
    );
    assert!(
        inner.livekit_url.is_empty(),
        "livekit_url must be empty for claude-cli sessions; got: {}",
        inner.livekit_url
    );
    assert!(
        inner.livekit_server_identity.is_empty(),
        "livekit_server_identity must be empty for claude-cli sessions; got: {}",
        inner.livekit_server_identity
    );
}

/// **claude_cli_session_enrichment_reads_from_metadata**: when a session directory contains
/// `.session.yaml` with `session_type = "claude-cli"` and no `changeset.yaml`, `ListSessions`
/// must return `agent = "claude-cli"` and `model` from the metadata — not placeholder dashes.
#[tokio::test]
async fn claude_cli_session_enrichment_reads_from_metadata() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000001";
    let session_dir = sessions_tmp
        .path()
        .join("testuser")
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Write a .session.yaml with session_type and model.
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        updated_at: "2026-06-06T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktree-stub".to_string()),
        pid: Some(99999),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
    };
    write_session_metadata(&session_dir, &meta).unwrap();
    // No changeset.yaml — intentionally absent to test the claude-cli fallback path.

    let config_yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
"#;
    let cfg_dir = tempfile::tempdir().unwrap();
    let cfg_path = cfg_dir.path().join("d.yaml");
    std::fs::write(&cfg_path, config_yaml).unwrap();
    let config = DaemonConfig::load(&cfg_path).unwrap();

    let sessions_base = sessions_tmp.path().join("testuser");
    let service = minimal_service(config, sessions_base);

    let sessions = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions must succeed")
        .into_inner()
        .sessions;

    let entry = sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("session must appear in ListSessions response");

    assert_eq!(
        entry.agent, "claude-cli",
        "agent must be 'claude-cli' for claude-cli sessions (not '—')"
    );
    assert_eq!(
        entry.model, TEST_MODEL,
        "model must be read from .session.yaml for claude-cli sessions (not '—')"
    );
    assert!(
        entry.workflow_goal.is_empty() || entry.workflow_goal == "—",
        "workflow_goal should be empty/dash for claude-cli sessions (no changeset)"
    );
    assert!(
        entry.workflow_state.is_empty() || entry.workflow_state == "—",
        "workflow_state should be empty/dash for claude-cli sessions (no changeset)"
    );
}

/// **claude_cli_session_resume_relaunches_in_worktree**: after a claude-cli session becomes
/// inactive (process exits), `ResumeSession` must relaunch `claude --model <model>` in the
/// existing worktree and mark the session active again.
#[tokio::test]
async fn claude_cli_session_resume_relaunches_in_worktree() {
    let worktree_dir = tempfile::tempdir().unwrap();
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000002";
    let session_dir = sessions_tmp
        .path()
        .join("testuser")
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Simulate an inactive claude-cli session (process previously exited).
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        updated_at: "2026-06-06T10:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: Some(worktree_dir.path().to_str().unwrap().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
    };
    write_session_metadata(&session_dir, &meta).unwrap();

    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let sessions_base = sessions_tmp.path().join("testuser");
    let service = minimal_service(config, sessions_base);

    let resp = service
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("ResumeSession must succeed for an inactive claude-cli session");

    let inner = resp.into_inner();
    assert_eq!(
        inner.session_id, session_id,
        "ResumeSession must return the same session_id"
    );
    assert!(
        inner.livekit_room.is_empty(),
        "resumed claude-cli session must not return a livekit_room"
    );

    // After resume, metadata must be updated to active with a fresh PID.
    let updated_meta = read_session_metadata(&session_dir)
        .expect(".session.yaml must be readable after resume");
    assert_eq!(
        updated_meta.status, "active",
        "session must be marked active after resume"
    );
    assert!(
        updated_meta.pid.is_some(),
        "session must have a non-None pid after resume (new PTY process spawned)"
    );
}

/// **claude_cli_start_session_requires_model**: `StartSession` with `session_type = "claude-cli"`
/// and an empty `model` must return `INVALID_ARGUMENT`.
#[tokio::test]
async fn claude_cli_start_session_requires_model() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let config_yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
"#;
    let cfg_dir = tempfile::tempdir().unwrap();
    let cfg_path = cfg_dir.path().join("d.yaml");
    std::fs::write(&cfg_path, config_yaml).unwrap();
    let config = DaemonConfig::load(&cfg_path).unwrap();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let err = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: String::new(), // empty — must be rejected before project lookup
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
        }))
        .await
        .expect_err("StartSession with claude-cli and empty model must fail");

    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "empty model for claude-cli must yield INVALID_ARGUMENT"
    );
    let msg = err.message.to_ascii_lowercase();
    assert!(
        msg.contains("model"),
        "error message must mention 'model'; got: {}",
        err.message
    );
}

/// **claude_cli_start_session_requires_project**: `StartSession` with `session_type = "claude-cli"`
/// and an empty `project_id` must return `INVALID_ARGUMENT`.
#[tokio::test]
#[serial_test::serial]
async fn claude_cli_start_session_requires_project() {
    // Point TDDY_PROJECTS_DIR at an empty temp dir so find_project returns None cleanly.
    let projects_tmp = tempfile::tempdir().unwrap();
    std::env::set_var(TDDY_PROJECTS_DIR_ENV, projects_tmp.path());
    let _restore = scopeguard::guard((), |_| std::env::remove_var(TDDY_PROJECTS_DIR_ENV));

    let sessions_tmp = tempfile::tempdir().unwrap();
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // Empty project_id → InvalidArgument.
    let err = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: String::new(), // empty
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
        }))
        .await
        .expect_err("StartSession with empty project_id must fail");

    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "empty project_id for claude-cli must yield INVALID_ARGUMENT"
    );

    // Unknown project_id → NotFound.
    let err2 = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: "no-such-project".to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
        }))
        .await
        .expect_err("StartSession with unknown project_id must fail");

    assert_eq!(
        err2.code,
        Code::NotFound,
        "unknown project_id for claude-cli must yield NOT_FOUND"
    );
}
