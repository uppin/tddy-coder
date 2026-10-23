//! Acceptance: a project-home session's exec tools are read-only, enforced by the daemon's
//! `ExecuteTool` itself — a write, an edit, a delete or a shell command is refused for that role
//! whatever the agent's allowlist says, while reading still works.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session (`#project-home` 4/7).

use std::path::Path;

use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
use tddy_rpc::{Code, Request};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a3a";

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
    tddy_projects::project_storage::write_projects(
        &sessions_base.join("projects"),
        &[tddy_projects::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "project-home".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
            accounts: Vec::new(),
            local_gh: false,
            home_agent: None,
        }],
    )
    .expect("register project");
}

/// A workspace session plus the service that owns it — the unit the exec tools run against.
struct Workspace {
    service: TestDaemon,
    session_id: String,
    worktree: std::path::PathBuf,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

async fn a_workspace_session() -> Workspace {
    let repo = a_git_repo_with_origin();
    let sessions = tempfile::tempdir().unwrap();
    register_project(sessions.path(), repo.path());
    let service = test_service(sessions.path().to_path_buf());

    let started = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session must start")
        .into_inner();

    let session_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        sessions.path(),
        &started.session_id,
    );
    let worktree = std::path::PathBuf::from(
        tddy_core::read_session_metadata(&session_dir)
            .expect("session metadata")
            .repo_path
            .expect("workspace worktree"),
    );

    Workspace {
        service,
        session_id: started.session_id,
        worktree,
        _repo: repo,
        _sessions: sessions,
    }
}

/// Give the workspace session the project-home role, as `EnsureProjectHomeSession` records it.
fn with_the_project_home_role(workspace: &Workspace) {
    let dir = tddy_core::session_lifecycle::unified_session_dir_path(
        workspace._sessions.path(),
        &workspace.session_id,
    );
    let mut meta = tddy_core::read_session_metadata(&dir).expect("session metadata");
    meta.session_role = Some("project-home".to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).expect("write metadata");
}

fn calling(workspace: &Workspace, tool: &str, args: serde_json::Value) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: workspace.session_id.clone(),
        tool_name: tool.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: String::new(),
        conversation_id: String::new(),
    }
}

#[tokio::test]
async fn a_project_home_session_cannot_write_or_run_a_shell_through_execute_tool() {
    // Given a project-home session over a worktree holding one file
    let workspace = a_workspace_session().await;
    std::fs::write(workspace.worktree.join("kept.txt"), "original\n").unwrap();
    with_the_project_home_role(&workspace);

    // When it tries every mutating tool
    let attempts = [
        (
            "Write",
            serde_json::json!({ "path": "new.txt", "contents": "x" }),
        ),
        (
            "StrReplace",
            serde_json::json!({ "path": "kept.txt", "old_string": "original", "new_string": "changed" }),
        ),
        ("Delete", serde_json::json!({ "path": "kept.txt" })),
        (
            "Shell",
            serde_json::json!({ "command": "touch shelled.txt" }),
        ),
    ];
    let mut codes = Vec::new();
    for (tool, args) in attempts {
        let outcome = workspace
            .service
            .execute_tool(Request::direct(calling(&workspace, tool, args)))
            .await;
        codes.push((tool, outcome.map(|_| ()).map_err(|s| s.code())));
    }

    // Then each is refused, and the worktree is untouched
    assert_eq!(
        codes,
        vec![
            ("Write", Err(Code::PermissionDenied)),
            ("StrReplace", Err(Code::PermissionDenied)),
            ("Delete", Err(Code::PermissionDenied)),
            ("Shell", Err(Code::PermissionDenied)),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(workspace.worktree.join("kept.txt")).unwrap(),
        "original\n"
    );
    assert!(!workspace.worktree.join("new.txt").exists());
    assert!(!workspace.worktree.join("shelled.txt").exists());
}

#[tokio::test]
async fn a_project_home_session_still_reads_through_execute_tool() {
    // Given
    let workspace = a_workspace_session().await;
    std::fs::write(workspace.worktree.join("kept.txt"), "original\n").unwrap();
    with_the_project_home_role(&workspace);

    // When
    let read = workspace
        .service
        .execute_tool(Request::direct(calling(
            &workspace,
            "Read",
            serde_json::json!({ "path": "kept.txt" }),
        )))
        .await
        .expect("Read is allowed")
        .into_inner();

    // Then
    assert!(!read.is_error, "{}", read.error_message);
    assert!(read.result_json.contains("original"));
}
