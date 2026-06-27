//! ConnectionService worktree RPCs (`ListWorktreesForProject`, `RemoveWorktree`).

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::project_storage::{self, ProjectData};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_daemon::user_sessions_path::projects_path_for_user;
use tddy_rpc::Code;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListWorktreesForProjectRequest,
    RemoveWorktreeRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn test_config_for_os_user(os_user: &str) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    DaemonConfig::load(&path).unwrap()
}

fn test_service(sessions_base: PathBuf, os_user: &str) -> ConnectionServiceImpl {
    let config = test_config_for_os_user(os_user);
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == TEST_TOKEN {
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

fn require_git() {
    let ok = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "git must be available for worktree RPC tests");
}

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {:?} in {:?}: {e}", args, cwd));
    assert!(st.success(), "git {:?} failed in {:?}", args, cwd);
}

/// Acceptance: invalid session token is rejected before project resolution.
#[tokio::test]
async fn list_worktrees_rejects_invalid_session() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .list_worktrees_for_project(Request::new(ListWorktreesForProjectRequest {
            session_token: "bad".to_string(),
            project_id: "p1".to_string(),
            refresh: false,
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::Unauthenticated);
}

/// Acceptance: unknown `project_id` yields NOT_FOUND (requires valid passwd for `os_user`).
#[tokio::test]
async fn list_worktrees_unknown_project_not_found() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    projects_path_for_user(&os_user, None).expect("projects path for current user");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .list_worktrees_for_project(Request::new(ListWorktreesForProjectRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: "00000000-0000-0000-0000-000000000099".to_string(),
            refresh: false,
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::NotFound);
}

/// Acceptance: empty `worktree_path` is INVALID_ARGUMENT.
#[tokio::test]
async fn remove_worktree_empty_path_invalid_argument() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .remove_worktree(Request::new(RemoveWorktreeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: "any".to_string(),
            worktree_path: "".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::InvalidArgument);
}

/// Acceptance: refresh lists worktrees for a registered project (git + projects registry).
#[tokio::test]
async fn list_worktrees_refresh_returns_git_worktree_rows() {
    // Given — a git repo with a secondary worktree, registered in the project registry
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");

    // The service derives its projects registry from its tddy_data_dir
    // (`{tddy_data_dir}/projects/`), so the test must register the project under the
    // same data dir the service was constructed with — not the profile/global default.
    let data_dir = tempfile::tempdir().unwrap();
    let service = test_service(data_dir.path().to_path_buf(), &os_user);
    let projects_dir =
        projects_path_for_user(&os_user, Some(data_dir.path())).expect("projects dir");

    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("main");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init"]);
    run_git(&repo, &["config", "user.email", "t@e.st"]);
    run_git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("README.md"), "x\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "init"]);
    let wt = tmp.path().join("wt-secondary");
    run_git(
        &repo,
        &[
            "worktree",
            "add",
            wt.to_str().unwrap(),
            "-b",
            "rpc-accept-branch",
        ],
    );

    let main_repo_path = repo.canonicalize().unwrap();
    let project_id = uuid::Uuid::new_v4().to_string();
    let project = ProjectData {
        project_id: project_id.clone(),
        name: "rpc-worktrees-test".to_string(),
        git_url: "https://example.com/r.git".to_string(),
        main_repo_path: main_repo_path.display().to_string(),
        main_branch_ref: None,
        host_repo_paths: std::collections::HashMap::new(),
    };

    project_storage::add_project(&projects_dir, project).unwrap();

    // When
    let res = service
        .list_worktrees_for_project(Request::new(ListWorktreesForProjectRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project_id.clone(),
            refresh: true,
        }))
        .await
        .expect("ListWorktreesForProject");
    // Then
    let paths: Vec<String> = res
        .into_inner()
        .worktrees
        .into_iter()
        .map(|w| w.path)
        .collect();
    assert!(
        paths.iter().any(|p| p.contains("wt-secondary")),
        "expected secondary worktree path in response, got {:?}",
        paths
    );
}
