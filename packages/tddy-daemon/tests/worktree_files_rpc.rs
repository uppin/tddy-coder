//! ConnectionService worktree Code-pane RPCs (`ListWorktreeDirectory`, `ReadWorktreeFile`).
//!
//! These assert per-directory listing rooted at a session's git worktree (`.gitignore`-aware,
//! `.git`-excluded), size-capped UTF-8 reads, worktree-list validation, and traversal/auth
//! rejection. Handlers are not implemented yet.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::project_storage::{self, ProjectData};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_daemon::user_sessions_path::projects_path_for_user;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListWorktreeDirectoryRequest,
    ReadWorktreeFileRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const MAIN_RS: &str = "fn main() { println!(\"worktree-code-pane\"); }\n";

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
    assert!(ok, "git must be available for worktree Code-pane RPC tests");
}

fn run_git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
    assert!(st.success(), "git {args:?} failed in {cwd:?}");
}

/// A registered project whose main repo has a secondary worktree (`wt-feature`) populated with
/// `README.md` (tracked), `src/main.rs` (untracked), and an ignored `secret.txt`
/// (via the worktree's `info/exclude`, so no tracked `.gitignore` is added).
struct Fixture {
    service: ConnectionServiceImpl,
    project_id: String,
    worktree_path: String,
    _data_dir: tempfile::TempDir,
    _tmp: tempfile::TempDir,
}

fn a_project_with_worktree(os_user: &str) -> Fixture {
    let data_dir = tempfile::tempdir().unwrap();
    let service = test_service(data_dir.path().to_path_buf(), os_user);
    let projects_dir =
        projects_path_for_user(os_user, Some(data_dir.path())).expect("projects dir");

    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("main");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q"]);
    run_git(&repo, &["config", "user.email", "t@e.st"]);
    run_git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("README.md"), "# Hello Worktree\n\n- alpha\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-q", "-m", "init"]);

    let wt = tmp.path().join("wt-feature");
    run_git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            wt.to_str().unwrap(),
            "-b",
            "feature-x",
        ],
    );

    // Ignore `secret.txt` via this worktree's private excludes (a linked worktree's `.git` is a
    // file, so its excludes live under the main repo's `.git/worktrees/<name>/info/`).
    let wt_info = repo.join(".git/worktrees/wt-feature/info");
    std::fs::create_dir_all(&wt_info).unwrap();
    std::fs::write(wt_info.join("exclude"), "secret.txt\n").unwrap();

    std::fs::create_dir_all(wt.join("src")).unwrap();
    std::fs::write(wt.join("src/main.rs"), MAIN_RS).unwrap();
    std::fs::write(wt.join("secret.txt"), "SECRET=must-not-appear\n").unwrap();

    let main_repo_path = repo.canonicalize().unwrap();
    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "code-pane-rpc-test".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: main_repo_path.display().to_string(),
            main_branch_ref: None,
            host_repo_paths: std::collections::HashMap::new(),
        },
    )
    .unwrap();

    Fixture {
        service,
        project_id,
        worktree_path: wt.canonicalize().unwrap().display().to_string(),
        _data_dir: data_dir,
        _tmp: tmp,
    }
}

/// Acceptance: an invalid session token is rejected before any filesystem access.
#[tokio::test]
async fn list_worktree_directory_rejects_invalid_session() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .list_worktree_directory(Request::new(ListWorktreeDirectoryRequest {
            session_token: "bad".to_string(),
            project_id: "p1".to_string(),
            worktree_path: "/tmp".to_string(),
            rel_path: "".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::Unauthenticated);
}

/// Acceptance: an unknown `project_id` yields NOT_FOUND.
#[tokio::test]
async fn list_worktree_directory_unknown_project_not_found() {
    // Given
    let os_user = std::env::var("USER").expect("USER must be set");
    projects_path_for_user(&os_user, None).expect("projects path for current user");
    let service = test_service(tempfile::tempdir().unwrap().path().to_path_buf(), &os_user);

    // When
    let err = service
        .list_worktree_directory(Request::new(ListWorktreeDirectoryRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: "00000000-0000-0000-0000-000000000099".to_string(),
            worktree_path: "/tmp".to_string(),
            rel_path: "".to_string(),
        }))
        .await
        .unwrap_err();

    // Then
    assert_eq!(err.code, Code::NotFound);
}

/// Acceptance: a `worktree_path` that is not in the project's `git worktree list` is rejected.
#[tokio::test]
async fn list_worktree_directory_rejects_worktree_not_in_git_list() {
    // Given — a real registered project, but a bogus worktree path.
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");
    let fixture = a_project_with_worktree(&os_user);

    // When
    let err = fixture
        .service
        .list_worktree_directory(Request::new(ListWorktreeDirectoryRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: fixture.project_id.clone(),
            worktree_path: "/tmp/not-a-worktree".to_string(),
            rel_path: "".to_string(),
        }))
        .await
        .unwrap_err();

    // Then — a client/security error, never a successful listing outside the git worktree list.
    assert_ne!(err.code, Code::Ok);
    assert!(
        matches!(
            err.code,
            Code::InvalidArgument
                | Code::PermissionDenied
                | Code::FailedPrecondition
                | Code::NotFound
        ),
        "unlisted worktree path must be rejected, got {:?}",
        err.code
    );
}

/// Acceptance: listing the worktree root returns its files and directories, excluding `.git` and
/// ignored paths.
#[tokio::test]
async fn list_worktree_directory_returns_root_entries_excluding_ignored() {
    // Given
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");
    let fixture = a_project_with_worktree(&os_user);

    // When
    let entries = fixture
        .service
        .list_worktree_directory(Request::new(ListWorktreeDirectoryRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: fixture.project_id.clone(),
            worktree_path: fixture.worktree_path.clone(),
            rel_path: "".to_string(),
        }))
        .await
        .expect("ListWorktreeDirectory should succeed for a listed worktree")
        .into_inner()
        .entries;

    // Then — git metadata / dotfile visibility can vary, so assert on the entries that matter:
    // the source dir and README.md are present; `.git` and the ignored `secret.txt` are not.
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains(&"src".to_string()), "got {names:?}");
    assert!(names.contains(&"README.md".to_string()), "got {names:?}");
    assert!(!names.contains(&".git".to_string()), "got {names:?}");
    assert!(!names.contains(&"secret.txt".to_string()), "got {names:?}");
    let src = entries.iter().find(|e| e.name == "src").unwrap();
    assert!(src.is_dir, "src must be reported as a directory");
}

/// Acceptance: reading a file under the worktree returns its exact UTF-8 contents.
#[tokio::test]
async fn read_worktree_file_returns_utf8_content() {
    // Given
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");
    let fixture = a_project_with_worktree(&os_user);

    // When
    let resp = fixture
        .service
        .read_worktree_file(Request::new(ReadWorktreeFileRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: fixture.project_id.clone(),
            worktree_path: fixture.worktree_path.clone(),
            rel_path: "src/main.rs".to_string(),
        }))
        .await
        .expect("ReadWorktreeFile should succeed for a file under the worktree")
        .into_inner();

    // Then
    assert_eq!(resp.content_utf8, MAIN_RS);
    assert!(!resp.truncated);
    assert_eq!(resp.byte_size, MAIN_RS.len() as u64);
}

/// Acceptance: a traversal `rel_path` never reads outside the worktree root.
#[tokio::test]
async fn read_worktree_file_rejects_traversal() {
    // Given
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");
    let fixture = a_project_with_worktree(&os_user);

    // When / Then
    for malicious in ["../../etc/passwd", "..\\secret.txt", "src/../../secret.txt"] {
        let err = fixture
            .service
            .read_worktree_file(Request::new(ReadWorktreeFileRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: fixture.project_id.clone(),
                worktree_path: fixture.worktree_path.clone(),
                rel_path: malicious.to_string(),
            }))
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.code,
                Code::InvalidArgument | Code::PermissionDenied | Code::FailedPrecondition
            ),
            "malicious rel_path {malicious:?} must be rejected, got {:?}",
            err.code
        );
    }
}
