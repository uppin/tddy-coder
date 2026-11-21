//! Acceptance: an orchestrator checks, itself, whether its home worktree is at its project's trunk
//! tip — after a fetch — and fast-forwards it there, through the daemon's `ExecuteTool`; a worker
//! session is refused both (`#orchestra` 4/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.

use std::path::{Path, PathBuf};

use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
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
    run_git(path, &["commit", "-q", "--allow-empty", "-m", "init"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
    repo
}

/// A project with an orchestrator session and one worker.
struct Project {
    service: TestDaemon,
    orchestrator: String,
    worker: String,
    sessions: tempfile::TempDir,
    _repo: tempfile::TempDir,
}

impl Project {
    async fn new() -> Self {
        let repo = a_git_repo_with_origin();
        let sessions = tempfile::tempdir().unwrap();
        tddy_projects::project_storage::write_projects(
            &sessions.path().join("projects"),
            &[tddy_projects::project_storage::ProjectData {
                project_id: PROJECT_ID.to_string(),
                name: "fresh".to_string(),
                git_url: String::new(),
                main_repo_path: repo.path().display().to_string(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: Default::default(),
                accounts: Vec::new(),
                local_gh: false,
                home_session_id: None,
                orchestrator_defaults: None,
                lifecycle: Default::default(),
            }],
        )
        .expect("register project");
        let service = test_service(sessions.path().to_path_buf());
        let orchestrator = start_workspace(&service).await;
        let worker = start_workspace(&service).await;
        let project = Self {
            service,
            orchestrator,
            worker,
            sessions,
            _repo: repo,
        };
        project.give_role(&project.orchestrator, "orchestrator");
        project
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        tddy_core::session_lifecycle::unified_session_dir_path(self.sessions.path(), session_id)
    }

    fn give_role(&self, session_id: &str, role: &str) {
        let dir = self.session_dir(session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).expect("session metadata");
        meta.session_role = Some(role.to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).expect("write metadata");
    }

    async fn call(
        &self,
        session_id: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> ExecuteToolResponse {
        self.service
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
            .unwrap_or_else(|status| ExecuteToolResponse {
                is_error: true,
                error_message: status.message().to_string(),
                ..Default::default()
            })
    }
}

async fn start_workspace(service: &TestDaemon) -> String {
    service
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session must start")
        .into_inner()
        .session_id
}

fn result_of(response: &ExecuteToolResponse) -> serde_json::Value {
    assert!(!response.is_error, "{}", response.error_message);
    serde_json::from_str(&response.result_json).expect("the result is JSON")
}

impl Project {
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            tddy_core::read_session_metadata(&self.session_dir(session_id))
                .expect("session metadata")
                .repo_path
                .expect("a worktree"),
        )
    }

    /// A commit lands on the project's trunk (`origin/main`); returns its sha.
    fn a_commit_lands_on_the_trunk(&self, subject: &str) -> String {
        let repo = self._repo.path();
        run_git(repo, &["commit", "-q", "--allow-empty", "-m", subject]);
        run_git(repo, &["push", "-q", "origin", "main"]);
        let out = std::process::Command::new("git")
            .args(["rev-parse", "main"])
            .current_dir(repo)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}

#[tokio::test]
async fn check_base_freshness_reports_the_orchestrators_home_behind_after_the_trunk_moves() {
    // Given an orchestrator, and a commit that landed on the trunk after its worktree was cut
    let project = Project::new().await;
    let tip = project.a_commit_lands_on_the_trunk("landed");

    // When it checks its base
    let checked = project
        .call(
            &project.orchestrator,
            "CheckBaseFreshness",
            serde_json::json!({}),
        )
        .await;

    // Then it is told it is one behind the new tip
    let freshness = result_of(&checked);
    assert_eq!(
        (
            freshness["behind"].as_u64(),
            freshness["remote_tip"].as_str()
        ),
        (Some(1), Some(tip.as_str()))
    );
}

#[tokio::test]
async fn refresh_home_brings_the_orchestrators_home_to_the_tip() {
    // Given an orchestrator behind its trunk
    let project = Project::new().await;
    let tip = project.a_commit_lands_on_the_trunk("landed");

    // When it refreshes its home
    let refreshed = project
        .call(&project.orchestrator, "RefreshHome", serde_json::json!({}))
        .await;

    // Then its worktree is at the tip
    assert_eq!(result_of(&refreshed)["behind"].as_u64(), Some(0));
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project.worktree_of(&project.orchestrator))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), tip);
}

#[tokio::test]
async fn a_worker_session_is_refused_the_freshness_tools() {
    // Given a worker session (no orchestrator role) behind its trunk
    let project = Project::new().await;
    project.a_commit_lands_on_the_trunk("landed");
    let before = project.worktree_of(&project.worker);

    // When it asks to refresh
    let refused = project
        .call(&project.worker, "RefreshHome", serde_json::json!({}))
        .await;

    // Then it is refused
    assert!(refused.is_error, "{}", refused.result_json);
    assert!(before.exists());
}
