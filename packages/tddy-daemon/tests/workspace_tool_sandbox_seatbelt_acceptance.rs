//! Acceptance: a sandboxed workspace session's tools inside a **real** Seatbelt jail.
//!
//! PRD: `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.
//! Changeset: `docs/dev/changesets.md`, 2026-08-30 workspace tool sandbox.
//!
//! `workspace_tool_sandbox_acceptance.rs` proves the daemon *routes* a tool call to the jail. That
//! is the daemon's claim and an injected provisioner can answer it. Whether the jail then holds is
//! the kernel's claim, and only a real one can: these tests run the production provisioner, spawn a
//! genuine Seatbelt jail, and ask it for files it must not be able to reach.
//!
//! The Linux cgroups half of the same claim is `tddy-e2e/tests/vm_workspace_tool_sandbox_acceptance.rs`.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ExecuteToolRequest, ExecuteToolResponse,
    StartSessionRequest,
};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a43";

/// Long enough for a jailed `sh -c` to finish, short enough that a wedged jail fails the test
/// rather than hanging the suite.
const SHELL_BLOCK_MS: u64 = 30_000;

/// What a host file outside the worktree contains. A jail that can read it leaks this exact string.
const HOST_SECRET: &str = "a-host-file-the-jail-must-never-read";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn sandbox_runner_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-sandbox-runner")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-sandbox-runner")
        })
}

fn tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

fn a_git_repo_with_origin() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let path = repo.path();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@t.com"]);
    run_git(path, &["config", "user.name", "Test"]);
    run_git(path, &["commit", "-q", "--allow-empty", "-m", "init"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn register_project(sessions_base: &Path, repo_path: &Path) {
    tddy_daemon::project_storage::write_projects(
        &sessions_base.join("projects"),
        &[tddy_daemon::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "workspace-tool-sandbox-seatbelt".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

/// A sandboxed workspace session on a real daemon, plus a host file deliberately left outside its
/// worktree for the jail to fail to reach.
struct JailedWorkspace {
    service: tddy_daemon::connection_service::ConnectionServiceImpl,
    session_id: String,
    worktree: PathBuf,
    host_secret_file: PathBuf,
    host_outside_dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

async fn a_sandboxed_workspace_session() -> JailedWorkspace {
    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    assert!(
        runner.exists(),
        "build tddy-sandbox-runner first: {}",
        runner.display()
    );
    assert!(
        tools.exists(),
        "build tddy-tools first: {}",
        tools.display()
    );

    let repo = a_git_repo_with_origin();
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    register_project(sessions.path(), repo.path());
    let service = test_service(sessions.path().to_path_buf());

    // A host file that is emphatically not in the worktree, and not under the session directory
    // either — the two trees the jail legitimately holds.
    let host_outside_dir = tempfile::tempdir().expect("host tempdir");
    let host_secret_file = host_outside_dir.path().join("host-secret.txt");
    std::fs::write(&host_secret_file, HOST_SECRET).expect("write host secret");

    let started = service
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: PROJECT_ID.to_string(),
            sandbox: true,
            ..Default::default()
        }))
        .await
        .expect("a sandboxed workspace session must start on a host with Seatbelt")
        .into_inner();

    let session_dir = unified_session_dir_path(sessions.path(), &started.session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir).expect("session metadata");

    // The fixture's own premise, asserted before any test builds on it: a session that came up
    // unsandboxed would still serve every tool below, from the bare host, and each assertion after
    // this point would be about the tool engine rather than about a jail.
    assert_eq!(
        metadata.sandbox,
        Some(true),
        "the session must actually be sandboxed for anything below to be about the jail"
    );

    let worktree = PathBuf::from(metadata.repo_path.expect("workspace worktree"));

    JailedWorkspace {
        service,
        session_id: started.session_id,
        worktree,
        host_secret_file,
        host_outside_dir,
        _repo: repo,
        _sessions: sessions,
    }
}

impl JailedWorkspace {
    async fn execute_tool(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.service
            .execute_tool(Request::new(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
            }))
            .await
            .expect("ExecuteTool must not fail at the RPC level")
            .into_inner()
    }

    /// Run `command` in the jail and return its `{stdout, stderr, exit_code}`.
    async fn shell(&self, command: &str) -> ShellResult {
        let response = self
            .execute_tool(
                "Shell",
                serde_json::json!({ "command": command, "block_until_ms": SHELL_BLOCK_MS }),
            )
            .await;
        assert!(
            !response.is_error,
            "the Shell call itself must reach the jail; error was '{}'",
            response.error_message
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&response.result_json).unwrap_or_else(|e| {
                panic!("Shell result must be JSON ({e}): {}", response.result_json)
            });
        ShellResult {
            stdout: parsed["stdout"].as_str().unwrap_or_default().to_string(),
            stderr: parsed["stderr"].as_str().unwrap_or_default().to_string(),
            exit_code: parsed["exit_code"].as_i64().unwrap_or(-1),
        }
    }
}

struct ShellResult {
    stdout: String,
    stderr: String,
    exit_code: i64,
}

// ---------------------------------------------------------------------------
// The jail serves the worktree
// ---------------------------------------------------------------------------

/// The jail is not merely a wall: the tools that go through it still do their job, against the
/// real checkout, and what they wrote is on the host afterwards.
#[tokio::test]
async fn a_write_tool_lands_in_the_worktree_through_a_real_seatbelt_jail() {
    // Given
    let workspace = a_sandboxed_workspace_session().await;

    // When
    let response = workspace
        .execute_tool(
            "Write",
            serde_json::json!({ "path": "through-the-jail.txt", "contents": "written inside" }),
        )
        .await;

    // Then
    assert!(
        !response.is_error,
        "the jailed Write must succeed; error was '{}'",
        response.error_message
    );
    assert_eq!(
        std::fs::read_to_string(workspace.worktree.join("through-the-jail.txt"))
            .expect("the jailed write must be visible on the host worktree"),
        "written inside"
    );
}

/// A jailed `Shell` runs in the worktree, so a relative command sees the session's own checkout
/// rather than whatever directory the daemon happened to be in.
#[tokio::test]
async fn a_shell_tool_runs_in_the_worktree_inside_a_real_seatbelt_jail() {
    // Given
    let workspace = a_sandboxed_workspace_session().await;
    std::fs::write(workspace.worktree.join("marker.txt"), "in the worktree")
        .expect("seed a file in the worktree");

    // When
    let result = workspace.shell("cat marker.txt").await;

    // Then
    assert_eq!(result.exit_code, 0, "stderr was: {}", result.stderr);
    assert_eq!(result.stdout.trim(), "in the worktree");
}

// ---------------------------------------------------------------------------
// The jail holds — what the kernel enforces and the tool engine does not
// ---------------------------------------------------------------------------

/// The claim the feature exists for. `Shell` is the tool the engine's path checks never bounded:
/// unconfined, `cat /abs/path` reads anything the daemon user can. Inside the jail it must not.
#[tokio::test]
async fn a_shell_tool_in_the_jail_cannot_read_a_host_file_outside_the_worktree() {
    // Given a host file outside the worktree, which the daemon user can read perfectly well
    let workspace = a_sandboxed_workspace_session().await;
    assert_eq!(
        std::fs::read_to_string(&workspace.host_secret_file).expect("the host itself can read it"),
        HOST_SECRET
    );

    // When the jailed shell is asked for it
    let result = workspace
        .shell(&format!(
            "cat {}",
            workspace.host_secret_file.to_string_lossy()
        ))
        .await;

    // Then it is refused, and not one byte of it came back
    assert_ne!(
        result.exit_code, 0,
        "reading outside the worktree must fail inside the jail; stdout was: {}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(HOST_SECRET),
        "the jail leaked a host file outside the worktree: {}",
        result.stdout
    );
}

/// The mutating half of the same boundary: a jailed shell may write its own checkout and nothing
/// else of the host.
#[tokio::test]
async fn a_shell_tool_in_the_jail_cannot_write_a_host_file_outside_the_worktree() {
    // Given
    let workspace = a_sandboxed_workspace_session().await;
    let target = workspace.host_outside_dir.path().join("escaped.txt");

    // When
    let result = workspace
        .shell(&format!("echo escaped > {}", target.to_string_lossy()))
        .await;

    // Then
    assert_ne!(
        result.exit_code, 0,
        "writing outside the worktree must fail inside the jail"
    );
    assert!(
        !target.exists(),
        "the jail wrote {} on the host, outside the worktree it was given",
        target.display()
    );
}

/// A path traversal out of the worktree is the same escape spelled differently, and the jail —
/// not the tool engine's string checks — is what must stop it.
#[tokio::test]
async fn a_shell_tool_in_the_jail_cannot_climb_out_of_the_worktree_with_a_relative_path() {
    // Given
    let workspace = a_sandboxed_workspace_session().await;

    // When
    let result = workspace
        .shell(&format!(
            "cat ../../../../../../../..{}",
            workspace.host_secret_file.to_string_lossy()
        ))
        .await;

    // Then
    assert_ne!(result.exit_code, 0, "stdout was: {}", result.stdout);
    assert!(
        !result.stdout.contains(HOST_SECRET),
        "a relative climb reached a host file outside the worktree: {}",
        result.stdout
    );
}
