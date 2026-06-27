//! Acceptance tests: permission_mode for Claude Code CLI sessions.
//! (PRD: docs/ft/daemon/claude-cli-permission-mode.md)

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::claude_cli_session::{ClaudeCliSessionManager, PtyHandle};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest,
};

// ---------------------------------------------------------------------------
// Test helpers (shared with claude_cli_session_acceptance.rs patterns)
// ---------------------------------------------------------------------------

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";

fn write_config_with_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
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

fn minimal_service_with_manager(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<ClaudeCliSessionManager>,
) -> ConnectionServiceImpl {
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
        manager,
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
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

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

// ---------------------------------------------------------------------------
// Unit tests — build_claude_argv with permission_mode (5th argument)
//
// These tests fully specify the argv layout. The existing tests in
// `claude_cli_session_acceptance.rs` that call build_claude_argv with 4 args
// will also need to be updated during the green phase (they'll fail once the
// 5th parameter is added).
// ---------------------------------------------------------------------------

/// **build_claude_argv_default_permission_mode_is_auto**: when permission_mode is None,
/// the argv must contain `--permission-mode` followed by `auto`.
#[test]
fn build_claude_argv_default_permission_mode_is_auto() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session",
        None,
        None, // permission_mode not specified → must default to "auto"
    );

    let pm_idx = argv
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("argv must contain --permission-mode flag when permission_mode is None");
    assert_eq!(
        argv[pm_idx + 1],
        "auto",
        "default permission_mode must be 'auto'; argv: {:?}",
        argv
    );
}

/// **build_claude_argv_explicit_bypass_permissions**: `Some("bypassPermissions")` is passed
/// as-is to the `--permission-mode` flag.
#[test]
fn build_claude_argv_explicit_bypass_permissions() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session",
        None,
        Some("bypassPermissions"),
    );

    let pm_idx = argv
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("argv must contain --permission-mode flag");
    assert_eq!(
        argv[pm_idx + 1],
        "bypassPermissions",
        "bypassPermissions must be passed through unchanged; argv: {:?}",
        argv
    );
}

/// **build_claude_argv_accept_edits_mode**: `Some("acceptEdits")` is passed as-is.
#[test]
fn build_claude_argv_accept_edits_mode() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session",
        None,
        Some("acceptEdits"),
    );

    let pm_idx = argv
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("argv must contain --permission-mode flag");
    assert_eq!(
        argv[pm_idx + 1],
        "acceptEdits",
        "acceptEdits must be passed through; argv: {:?}",
        argv
    );
}

/// **build_claude_argv_empty_string_defaults_to_auto**: `Some("")` is treated the same as
/// `None` — the mode defaults to `"auto"`.
#[test]
fn build_claude_argv_empty_string_defaults_to_auto() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session",
        None,
        Some(""),
    );

    let pm_idx = argv
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("argv must contain --permission-mode flag for empty permission_mode");
    assert_eq!(
        argv[pm_idx + 1],
        "auto",
        "empty permission_mode must default to 'auto'; argv: {:?}",
        argv
    );
}

/// **build_claude_argv_whitespace_defaults_to_auto**: `Some("   ")` (all whitespace) is
/// trimmed to empty and defaults to `"auto"`.
#[test]
fn build_claude_argv_whitespace_defaults_to_auto() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session",
        None,
        Some("   "),
    );

    let pm_idx = argv
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("argv must contain --permission-mode flag for whitespace permission_mode");
    assert_eq!(
        argv[pm_idx + 1],
        "auto",
        "whitespace-only permission_mode must default to 'auto'; argv: {:?}",
        argv
    );
}

/// **build_claude_argv_permission_mode_before_positional_prompt**: the `--permission-mode`
/// flag and its value must appear in the argv **before** any positional `initial_prompt`
/// argument so the `claude` binary parses flags before positional args.
#[test]
fn build_claude_argv_permission_mode_before_positional_prompt() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session",
        Some("build a feature"),
        Some("plan"),
    );

    let pm_idx = argv
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("argv must contain --permission-mode flag");
    let prompt_idx = argv
        .iter()
        .position(|a| a == "build a feature")
        .expect("argv must contain the positional prompt");
    assert!(
        pm_idx < prompt_idx,
        "--permission-mode (idx {}) must come before positional prompt (idx {}); argv: {:?}",
        pm_idx,
        prompt_idx,
        argv
    );
}

/// **build_claude_argv_permission_mode_appears_once**: regardless of input, `--permission-mode`
/// must appear exactly once in the built argv — no duplicate flags.
#[test]
fn build_claude_argv_permission_mode_appears_once() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session",
        None,
        Some("auto"),
    );

    let count = argv.iter().filter(|a| *a == "--permission-mode").count();
    assert_eq!(
        count, 1,
        "--permission-mode must appear exactly once; argv: {:?}",
        argv
    );
}

// ---------------------------------------------------------------------------
// Integration tests — PTY output verification
// ---------------------------------------------------------------------------

/// **claude_cli_session_pty_argv_includes_default_permission_mode**: when `manager.start()` is
/// called with `permission_mode = None`, the spawned process sees `--permission-mode auto` in
/// its `$@`.
#[tokio::test]
async fn claude_cli_session_pty_argv_includes_default_permission_mode() {
    // Given
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let worktree_dir = tempfile::tempdir().unwrap();
    let manager = ClaudeCliSessionManager::new();

    // When
    let handle = manager
        .start(
            "perm-mode-default-pty",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            stub_path.to_str().unwrap(),
            None, // initial_prompt
            None, // permission_mode → must default to "auto"
        )
        .await
        .expect("start with echo-argv stub and no permission_mode must succeed");

    // Then
    let found = wait_for_capture_contains(&handle, "ARGV:", 2000).await;
    assert!(
        found,
        "stub script must write ARGV: to PTY output within 2s"
    );

    let cap = handle.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    assert!(
        output.contains("--permission-mode"),
        "PTY ARGV must include --permission-mode flag; got: {:?}",
        output
    );
    assert!(
        output.contains("auto"),
        "default permission mode must be 'auto' in PTY ARGV; got: {:?}",
        output
    );
}

/// **claude_cli_session_pty_argv_includes_explicit_permission_mode**: when `manager.start()` is
/// called with `permission_mode = Some("bypassPermissions")`, the spawned process sees
/// `--permission-mode bypassPermissions` in its `$@`.
#[tokio::test]
async fn claude_cli_session_pty_argv_includes_explicit_permission_mode() {
    // Given
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let worktree_dir = tempfile::tempdir().unwrap();
    let manager = ClaudeCliSessionManager::new();

    // When
    let handle = manager
        .start(
            "perm-mode-bypass-pty",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            stub_path.to_str().unwrap(),
            None,
            Some("bypassPermissions"),
        )
        .await
        .expect("start with bypassPermissions must succeed");

    // Then
    let found = wait_for_capture_contains(&handle, "ARGV:", 2000).await;
    assert!(found, "stub script must write ARGV: within 2s");

    let cap = handle.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    assert!(
        output.contains("bypassPermissions"),
        "PTY ARGV must include bypassPermissions; got: {:?}",
        output
    );
}

// ---------------------------------------------------------------------------
// RPC wiring test — StartSessionRequest.permission_mode threads to PTY
// ---------------------------------------------------------------------------

/// **start_session_rpc_threads_permission_mode_to_pty**: `StartSession` with
/// `permission_mode = "bypassPermissions"` must pass `--permission-mode bypassPermissions`
/// down to the PTY subprocess. Verifies the full RPC → argv chain.
#[tokio::test]
#[serial_test::serial]
async fn start_session_rpc_threads_permission_mode_to_pty() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    // Register project under {sessions_base}/projects/ — where connection_service looks.
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());

    let (_cfg_dir, config) = write_config_with_binary(stub_path.to_str().unwrap());

    let shared_manager = Arc::new(ClaudeCliSessionManager::new());
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&shared_manager),
    );

    // When
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
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: "bypassPermissions".to_string(), // NEW FIELD — does not exist yet
            stack_parent: String::new(),
            sandbox: false,
        }))
        .await
        .expect("StartSession with permission_mode must succeed");

    // Then
    let session_id = resp.into_inner().session_id;
    let handle = shared_manager
        .get(&session_id)
        .await
        .expect("session must be registered in the shared manager");

    let found = wait_for_capture_contains(&handle, "ARGV:", 2000).await;
    assert!(found, "stub script must write ARGV: within 2s");

    let cap = handle.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    assert!(
        output.contains("bypassPermissions"),
        "StartSession permission_mode must reach the PTY ARGV; got: {:?}",
        output
    );
}

// ---------------------------------------------------------------------------
// Unit tests — exact argv equality (full structure)
// ---------------------------------------------------------------------------

/// **build_claude_argv_exact_structure_default_mode_no_prompt**: complete argv equality for the
/// common case: default permission_mode, no initial_prompt, model present.
/// Defines the canonical arg order: binary --model m --session-id id --permission-mode auto
#[test]
fn build_claude_argv_exact_structure_default_mode_no_prompt() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "sess-abc",
        None,
        None,
    );

    assert_eq!(
        argv,
        vec![
            "/usr/local/bin/claude",
            "--model",
            "claude-opus-4-8",
            "--session-id",
            "sess-abc",
            "--permission-mode",
            "auto",
        ],
        "full argv must be: binary --model m --session-id id --permission-mode auto"
    );
}

/// **build_claude_argv_exact_structure_bypass_with_prompt**: with bypassPermissions and a prompt,
/// the prompt comes last after all flags.
#[test]
fn build_claude_argv_exact_structure_bypass_with_prompt() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "claude",
        "claude-opus-4-8",
        "sess-xyz",
        Some("build the feature"),
        Some("bypassPermissions"),
    );

    assert_eq!(
        argv,
        vec![
            "claude",
            "--model",
            "claude-opus-4-8",
            "--session-id",
            "sess-xyz",
            "--permission-mode",
            "bypassPermissions",
            "build the feature",
        ],
        "full argv with prompt: binary --model m --session-id id --permission-mode mode prompt"
    );
}

/// **build_claude_argv_exact_structure_no_model**: when model is empty, --model is omitted but
/// --permission-mode is still present.
#[test]
fn build_claude_argv_exact_structure_no_model() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "claude",
        "", // empty model → --model flag omitted
        "sess-nomodel",
        None,
        None,
    );

    assert_eq!(
        argv,
        vec![
            "claude",
            "--session-id",
            "sess-nomodel",
            "--permission-mode",
            "auto",
        ],
        "with empty model: binary --session-id id --permission-mode auto (no --model)"
    );
}

/// **build_claude_argv_plan_mode**: `"plan"` (read-only, no execution) is passed through.
#[test]
fn build_claude_argv_plan_mode() {
    // When / Then
    let argv = ClaudeCliSessionManager::build_claude_argv(
        "claude",
        "claude-opus-4-8",
        "sess-plan",
        None,
        Some("plan"),
    );

    let pm_idx = argv
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("argv must contain --permission-mode");
    assert_eq!(
        argv[pm_idx + 1],
        "plan",
        "plan mode must be passed as-is; argv: {:?}",
        argv
    );
}

/// **build_claude_argv_default_mode_string**: `"default"` (the mode named "default", which
/// prompts before every tool use) is distinct from `None` (which becomes `"auto"`).
#[test]
fn build_claude_argv_default_mode_string() {
    // When / Then
    let argv_explicit_default = ClaudeCliSessionManager::build_claude_argv(
        "claude",
        "claude-opus-4-8",
        "sess-default",
        None,
        Some("default"),
    );
    let argv_none = ClaudeCliSessionManager::build_claude_argv(
        "claude",
        "claude-opus-4-8",
        "sess-default",
        None,
        None,
    );

    let mode_explicit = argv_explicit_default
        .iter()
        .position(|a| a == "--permission-mode")
        .map(|i| argv_explicit_default[i + 1].as_str())
        .unwrap_or("");
    let mode_none = argv_none
        .iter()
        .position(|a| a == "--permission-mode")
        .map(|i| argv_none[i + 1].as_str())
        .unwrap_or("");

    assert_eq!(
        mode_explicit, "default",
        "explicit 'default' mode must be passed through"
    );
    assert_eq!(mode_none, "auto", "None must produce 'auto', not 'default'");
    assert_ne!(
        mode_explicit, mode_none,
        "explicit 'default' mode must differ from the None default ('auto')"
    );
}

// ---------------------------------------------------------------------------
// Integration — resume does not carry a permission_mode flag from prior start
// ---------------------------------------------------------------------------

/// **resume_pty_argv_uses_default_permission_mode**: `manager.resume()` never accepts a
/// permission_mode argument — it always spawns with the default `--permission-mode auto`.
/// This test verifies resume does not accidentally replay the initial permission_mode.
#[tokio::test]
async fn resume_pty_argv_uses_default_permission_mode() {
    // Given
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());
    let worktree_dir = tempfile::tempdir().unwrap();
    let manager = ClaudeCliSessionManager::new();

    // Start with bypassPermissions.
    let _h1 = manager
        .start(
            "perm-resume-test",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            stub_path.to_str().unwrap(),
            None,
            Some("bypassPermissions"),
        )
        .await
        .expect("initial start with bypassPermissions must succeed");

    // When — resume() signature has no permission_mode parameter; always uses the default.
    let handle2 = manager
        .resume(
            "perm-resume-test",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            stub_path.to_str().unwrap(),
        )
        .await
        .expect("resume must succeed without permission_mode");

    // Then
    let found = wait_for_capture_contains(&handle2, "ARGV:", 2000).await;
    assert!(found, "stub script must write ARGV: within 2s on resume");

    let cap = handle2.capture.lock().unwrap();
    let output = String::from_utf8_lossy(&cap);
    let argv_line = output
        .lines()
        .find(|l| l.trim_start().starts_with("ARGV:"))
        .unwrap_or("");

    assert!(
        argv_line.contains("--permission-mode"),
        "resumed session must still include --permission-mode; ARGV: {:?}",
        argv_line
    );
    assert!(
        argv_line.contains("auto"),
        "resumed session must use 'auto' mode (not bypassPermissions); ARGV: {:?}",
        argv_line
    );
    assert!(
        !argv_line.contains("bypassPermissions"),
        "resume must NOT carry over bypassPermissions from the initial start; ARGV: {:?}",
        argv_line
    );
}
