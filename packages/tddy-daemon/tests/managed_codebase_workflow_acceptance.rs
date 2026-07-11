//! Acceptance tests: managed-codebase workflow for claude-cli sessions
//! (PRD: docs/ft/coder/managed-codebase-workflow.md).
//!
//! A claude-cli session started with `managed_codebase = true` and a workflow `recipe` must be
//! launched *workflow-aware*: the daemon seeds `changeset.yaml` with the recipe's start goal,
//! launches `claude` with `--append-system-prompt-file` (the recipe's orchestration prompt), and
//! injects a per-session `TDDY_SOCKET` so `tddy-tools transition` can advance the workflow. An
//! unknown recipe is rejected. These tests exercise the non-sandboxed path (a stub binary stands in
//! for `claude`), which needs no platform sandbox to run in CI.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::changeset::read_changeset;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::claude_cli_session::{ClaudeCliSessionManager, PtyHandle};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ResumeSessionRequest, StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";
const STUB_OUTPUT_TIMEOUT_MS: u64 = 10_000;
/// The start goal of the `tdd` recipe (`TddRecipe::start_goal()`), used to assert seeding.
const TDD_START_GOAL: &str = "interview";

/// The OS user the test process runs as — a real, resolvable user (same-user, so the interactive
/// claude-cli spawn needs no privilege drop). Fixtures use this rather than a fabricated name so
/// impersonation resolves during the spawn.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn write_config_with_claude_cli_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
claude_cli:
  binary_path: {stub_binary}
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service_with_manager(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver = Arc::new(move |token| {
        if token == VALID_TOKEN {
            Some(resolved_user.clone())
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
        manager,
    )
}

/// Create a git repo with an origin remote pointing at itself so worktree setup succeeds.
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
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// Executable stub that echoes its argv (for asserting the claude argument list).
fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude_argv.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\n").unwrap();
    make_executable(&script_path);
    script_path
}

/// Executable stub that dumps `TDDY_SOCKET` (for asserting per-session env injection).
fn write_env_echo_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude_env.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\necho \"ENVDUMP TDDY_SOCKET=[$TDDY_SOCKET]\"\n",
    )
    .unwrap();
    make_executable(&script_path);
    script_path
}

/// Executable stub that dumps both its argv and `TDDY_SOCKET` (for asserting a resumed managed
/// session is re-wired with the orchestration prompt and per-session socket).
fn write_argv_and_env_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude_argv_env.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\necho \"ARGV: $@\"\necho \"ENVDUMP TDDY_SOCKET=[$TDDY_SOCKET]\"\n",
    )
    .unwrap();
    make_executable(&script_path);
    script_path
}

fn make_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Poll `handle.capture` until its UTF-8 contents contain `needle` or the timeout elapses.
async fn wait_for_capture_contains(handle: &Arc<PtyHandle>, needle: &str, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        {
            let cap = handle.capture.lock().unwrap();
            if String::from_utf8_lossy(&cap).contains(needle) {
                return true;
            }
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// A managed claude-cli StartSession request template (non-sandboxed).
fn managed_request(recipe: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        recipe: recipe.to_string(),
        managed_codebase: true,
        sandbox: false,
        ..Default::default()
    }
}

/// AC6: a managed claude-cli session seeds `changeset.yaml` `state.current` with the recipe's start
/// goal (`interview` for `tdd`) before launch.
#[tokio::test]
async fn managed_claude_cli_session_seeds_changeset_with_recipe_start_goal() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );

    // When
    let session_id = service
        .start_session(Request::new(managed_request("tdd")))
        .await
        .expect("managed claude-cli StartSession must succeed")
        .into_inner()
        .session_id;

    // Then
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let changeset =
        read_changeset(&session_dir).expect("changeset.yaml must exist for the session");
    assert_eq!(
        changeset.state.current.as_str(),
        TDD_START_GOAL,
        "managed session must seed changeset state with the recipe start goal"
    );
}

/// AC7: a managed claude-cli session with an unknown recipe is rejected with INVALID_ARGUMENT.
#[tokio::test]
async fn managed_claude_cli_session_with_unknown_recipe_is_rejected() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::new(ClaudeCliSessionManager::new()),
    );

    // When
    let err = service
        .start_session(Request::new(managed_request("no-such-recipe")))
        .await
        .expect_err("managed claude-cli StartSession with an unknown recipe must fail");

    // Then
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "unknown recipe must yield INVALID_ARGUMENT, got: {err:?}"
    );
}

/// AC8: a managed claude-cli session launches `claude` with `--append-system-prompt-file`.
#[tokio::test]
async fn managed_claude_cli_session_launches_claude_with_orchestration_prompt_file() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = write_echo_argv_script(stub_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When
    let session_id = service
        .start_session(Request::new(managed_request("tdd")))
        .await
        .expect("managed claude-cli StartSession must succeed")
        .into_inner()
        .session_id;

    // Then
    let handle = manager
        .get(&session_id)
        .await
        .expect("session must be registered in the manager");
    assert!(
        wait_for_capture_contains(&handle, "ARGV:", STUB_OUTPUT_TIMEOUT_MS).await,
        "stub claude must echo its ARGV"
    );
    let cap = handle.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    assert!(
        output.contains("--append-system-prompt-file"),
        "managed session must launch claude with --append-system-prompt-file; got: {output:?}"
    );
}

/// AC9: a managed claude-cli session launches `claude` with a per-session `TDDY_SOCKET` in its env.
#[tokio::test]
async fn managed_claude_cli_session_launches_claude_with_tddy_socket_in_env() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let stub = write_env_echo_script(stub_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When
    let session_id = service
        .start_session(Request::new(managed_request("tdd")))
        .await
        .expect("managed claude-cli StartSession must succeed")
        .into_inner()
        .session_id;

    // Then
    let handle = manager
        .get(&session_id)
        .await
        .expect("session must be registered in the manager");
    assert!(
        wait_for_capture_contains(&handle, "ENVDUMP TDDY_SOCKET=[", STUB_OUTPUT_TIMEOUT_MS).await,
        "stub claude must echo its TDDY_SOCKET env"
    );
    let cap = handle.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    assert!(
        !output.contains("ENVDUMP TDDY_SOCKET=[]"),
        "managed session must inject a non-empty per-session TDDY_SOCKET; got: {output:?}"
    );
}

/// AC12: resuming a managed claude-cli session re-wires the orchestration prompt and per-session
/// TDDY_SOCKET (a resumed managed session stays workflow-aware, not relaunched as a plain session).
#[tokio::test]
async fn resuming_a_managed_claude_cli_session_re_wires_orchestration_and_socket() {
    // Given — an inactive managed claude-cli session (metadata records the recipe) in a worktree
    let worktree = tempfile::tempdir().unwrap();
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-0000000000aa";
    let session_dir = sessions_tmp.path().join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-07-03T10:00:00Z".to_string(),
        updated_at: "2026-07-03T10:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: Some(worktree.path().to_string_lossy().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: Some("tdd".to_string()),
        specialized_agents: Vec::new(),
    };
    write_session_metadata(&session_dir, &meta).unwrap();

    let stub_dir = tempfile::tempdir().unwrap();
    let stub = write_argv_and_env_script(stub_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub.to_str().unwrap());
    let manager = Arc::new(ClaudeCliSessionManager::new());
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&manager),
    );

    // When
    service
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("resuming a managed claude-cli session must succeed");

    // Then — the relaunched process is workflow-aware again
    let handle = manager
        .get(session_id)
        .await
        .expect("resumed session must be registered in the manager");
    assert!(
        wait_for_capture_contains(&handle, "ENVDUMP", STUB_OUTPUT_TIMEOUT_MS).await,
        "resumed stub claude must echo its argv + env"
    );
    let cap = handle.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    assert!(
        output.contains("--append-system-prompt-file"),
        "resumed managed session must re-inject the orchestration prompt; got: {output:?}"
    );
    assert!(
        !output.contains("ENVDUMP TDDY_SOCKET=[]"),
        "resumed managed session must re-inject a non-empty per-session TDDY_SOCKET; got: {output:?}"
    );
}
