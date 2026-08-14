//! Acceptance tests: deleting a `workspace` session removes the git worktree it created.
//!
//! A `workspace` session exists to hold a checkout with no PTY and no agent — it is the B-side of a
//! split session (docs/ft/daemon/remote-managed-worktree.md) and the unit the remote-codebase tool
//! engine executes against. Its whole purpose is the worktree, so leaving that worktree behind on
//! delete leaks the only thing the session was for.
//!
//! `session_deletion.rs` gates worktree removal on `session_type == "claude-cli"`, so a workspace
//! session currently loses its session directory and keeps its worktree — despite
//! docs/ft/daemon/remote-codebase-mode.md criterion 3 asserting the opposite.
//!
//! Removal must be git-aware: dropping the directory alone leaves a registered-but-missing entry in
//! `git worktree list`, which then blocks re-creating a worktree at the same path.

use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, StartSessionRequest,
};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a37";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A git repository with one commit and an `origin` remote pointing at itself, so the worktree
/// setup's `git fetch origin` succeeds without a real server (mirrors
/// `claude_cli_session_acceptance::create_test_repo_with_origin`).
fn a_git_repo_with_one_commit() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let path = repo.path();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "acceptance@example.invalid"]);
    run_git(path, &["config", "user.name", "Acceptance"]);
    std::fs::write(path.join("README.md"), "acceptance\n").expect("seed file");
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "-qm", "seed"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

/// Every path `git worktree list --porcelain` currently reports for `repo`.
fn registered_worktree_paths(repo: &Path) -> Vec<String> {
    let out = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()
        .expect("git worktree list");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree ").map(str::to_string))
        .collect()
}

fn register_project(sessions_base: &Path, repo_path: &Path) {
    let projects_dir = sessions_base.join("projects");
    tddy_daemon::project_storage::write_projects(
        &projects_dir,
        &[tddy_daemon::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "workspace-deletion".to_string(),
            git_url: "https://example.invalid/workspace-deletion.git".to_string(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_a_workspace_session_removes_its_git_worktree() {
    // Given a workspace session holding a real worktree cut from the project's repo
    let repo = a_git_repo_with_one_commit();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path());
    let service = test_service(sessions_tmp.path().to_path_buf());

    let started = service
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session must start")
        .into_inner();

    let worktree = worktree_path_of(sessions_tmp.path(), &started.session_id);
    assert!(
        worktree.exists(),
        "the workspace session must have created its worktree at {worktree:?}"
    );

    // When
    service
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: started.session_id.clone(),
        }))
        .await
        .expect("deleting a workspace session must succeed");

    // Then — the directory is gone
    assert!(
        !worktree.exists(),
        "deleting a workspace session must remove its worktree; {worktree:?} still exists"
    );
}

#[tokio::test]
async fn deleting_a_workspace_session_deregisters_the_worktree_from_git() {
    // Given
    let repo = a_git_repo_with_one_commit();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path());
    let service = test_service(sessions_tmp.path().to_path_buf());

    let started = service
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session must start")
        .into_inner();

    let worktree = worktree_path_of(sessions_tmp.path(), &started.session_id);
    let worktree_str = worktree.canonicalize().expect("worktree canonicalize");

    // When
    service
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: started.session_id.clone(),
        }))
        .await
        .expect("deleting a workspace session must succeed");

    // Then — git no longer lists it. Removing only the directory would leave a stale registration
    // that blocks re-creating a worktree at the same path.
    let registered = registered_worktree_paths(repo.path());
    assert!(
        !registered.iter().any(|p| Path::new(p) == worktree_str),
        "git must no longer list the removed worktree; still registered: {registered:?}"
    );
}

/// The worktree a session recorded in its `.session.yaml`.
fn worktree_path_of(sessions_base: &Path, session_id: &str) -> PathBuf {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let metadata =
        tddy_core::read_session_metadata(&session_dir).expect("session metadata must be readable");
    PathBuf::from(
        metadata
            .repo_path
            .expect("a workspace session must record the worktree it created"),
    )
}
