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
    CleanWorktreeRequest, ConnectionService as ConnectionServiceTrait,
    ListWorktreesForProjectRequest, RemoveWorktreeRequest, RestoreSessionWorktreeRequest,
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

// --- CleanWorktree (git clean -fdx) ---

/// Invalid session token is rejected before any project/worktree work.
#[tokio::test]
async fn clean_worktree_rejects_invalid_session() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .clean_worktree(Request::new(CleanWorktreeRequest {
            session_token: "bad".to_string(),
            project_id: "p1".to_string(),
            worktree_path: "/some/wt".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::Unauthenticated);
}

/// Empty `worktree_path` is INVALID_ARGUMENT.
#[tokio::test]
async fn clean_worktree_empty_path_invalid_argument() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .clean_worktree(Request::new(CleanWorktreeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: "any".to_string(),
            worktree_path: "".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::InvalidArgument);
}

/// Clearing the project's primary worktree is refused with FAILED_PRECONDITION.
#[tokio::test]
async fn clean_worktree_primary_is_failed_precondition() {
    // Given — a registered project whose main repo is a git repo
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");
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
    let main_repo_path = repo.canonicalize().unwrap();

    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "clean-primary".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: main_repo_path.display().to_string(),
            main_branch_ref: None,
            host_repo_paths: std::collections::HashMap::new(),
        },
    )
    .unwrap();

    // When the primary worktree path is passed to CleanWorktree
    let err = service
        .clean_worktree(Request::new(CleanWorktreeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id,
            worktree_path: main_repo_path.display().to_string(),
        }))
        .await
        .unwrap_err();

    // Then it is refused
    assert_eq!(err.code, Code::FailedPrecondition);
}

/// Clearing a secondary worktree drops untracked + ignored files and invalidates the stats cache.
#[tokio::test]
async fn clean_worktree_clears_secondary_and_invalidates_cache() {
    // Given — a registered project with a secondary worktree holding untracked + ignored files,
    // and a populated stats cache
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");
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
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    run_git(&repo, &["add", "README.md", ".gitignore"]);
    run_git(&repo, &["commit", "-m", "init"]);
    let wt = tmp.path().join("wt-secondary");
    run_git(
        &repo,
        &[
            "worktree",
            "add",
            wt.to_str().unwrap(),
            "-b",
            "clean-branch",
        ],
    );
    let wt = wt.canonicalize().unwrap();
    let untracked = wt.join("scratch.txt");
    let ignored = wt.join("target").join("build.o");
    std::fs::create_dir_all(wt.join("target")).unwrap();
    std::fs::write(&untracked, "scratch\n").unwrap();
    std::fs::write(&ignored, "obj\n").unwrap();

    let main_repo_path = repo.canonicalize().unwrap();
    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "clean-secondary".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: main_repo_path.display().to_string(),
            main_branch_ref: None,
            host_repo_paths: std::collections::HashMap::new(),
        },
    )
    .unwrap();

    // Populate the stats cache via a refreshing list.
    service
        .list_worktrees_for_project(Request::new(ListWorktreesForProjectRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project_id.clone(),
            refresh: true,
        }))
        .await
        .expect("prime cache");

    // When the secondary worktree is cleared
    let resp = service
        .clean_worktree(Request::new(CleanWorktreeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project_id.clone(),
            worktree_path: wt.display().to_string(),
        }))
        .await
        .expect("CleanWorktree")
        .into_inner();

    // Then the clear succeeds, untracked + ignored files are gone, tracked survive, and the cache
    // was invalidated (a non-refreshing list now returns no rows).
    assert!(resp.ok, "clean response should be ok: {}", resp.message);
    assert!(wt.join("README.md").exists(), "tracked file must survive");
    assert!(!untracked.exists(), "untracked file must be removed");
    assert!(!ignored.exists(), "ignored target/build.o must be removed");

    let cached = service
        .list_worktrees_for_project(Request::new(ListWorktreesForProjectRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id,
            refresh: false,
        }))
        .await
        .expect("cache-only list")
        .into_inner();
    assert!(
        cached.worktrees.is_empty(),
        "cache should have been invalidated on clean, got {:?}",
        cached.worktrees
    );
}

// --- RestoreSessionWorktree ---

/// Invalid session token is rejected before any restore work.
#[tokio::test]
async fn restore_session_worktree_rejects_invalid_session() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .restore_session_worktree(Request::new(RestoreSessionWorktreeRequest {
            session_token: "bad".to_string(),
            project_id: "p1".to_string(),
            session_id: "s1".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::Unauthenticated);
}

/// Restore recreates the session's worktree from its persisted changeset (branch + integration
/// base), landing a registered worktree in `git worktree list`.
#[tokio::test]
async fn restore_session_worktree_recreates_worktree_from_changeset() {
    // Given — a project repo with an `origin` remote, and a session whose changeset records its
    // branch, worktree name, and integration base, but whose worktree does not yet exist.
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");
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
    std::fs::write(repo.join("f"), "x\n").unwrap();
    run_git(&repo, &["add", "f"]);
    run_git(&repo, &["commit", "-m", "init"]);
    run_git(&repo, &["branch", "-M", "master"]);
    run_git(&repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    run_git(&repo, &["push", "-u", "origin", "master"]);
    let main_repo_path = repo.canonicalize().unwrap();

    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "restore-proj".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: main_repo_path.display().to_string(),
            main_branch_ref: None,
            host_repo_paths: std::collections::HashMap::new(),
        },
    )
    .unwrap();

    let session_id = "restore-sess-1";
    let session_dir = data_dir.path().join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let changeset = tddy_core::Changeset {
        name: Some("restore me".to_string()),
        branch_suggestion: Some("feature/restore".to_string()),
        worktree_suggestion: Some("feature-restore".to_string()),
        effective_worktree_integration_base_ref: Some("origin/master".to_string()),
        ..Default::default()
    };
    tddy_core::write_changeset(&session_dir, &changeset).unwrap();

    // When the session's worktree is restored
    let resp = service
        .restore_session_worktree(Request::new(RestoreSessionWorktreeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id,
            session_id: session_id.to_string(),
        }))
        .await
        .expect("RestoreSessionWorktree")
        .into_inner();

    // Then a worktree path is returned, exists on disk, and is registered with git.
    assert!(resp.ok, "restore should succeed: {}", resp.message);
    assert!(!resp.worktree_path.is_empty(), "restore must return a path");
    assert!(
        std::path::Path::new(&resp.worktree_path).exists(),
        "restored worktree dir must exist at {}",
        resp.worktree_path
    );

    let listed = Command::new("git")
        .current_dir(&main_repo_path)
        .args(["worktree", "list"])
        .output()
        .expect("git worktree list");
    let stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(
        stdout.contains("feature-restore"),
        "restored worktree must appear in git worktree list, got:\n{stdout}"
    );
}
