//! Acceptance: an orchestrator-role session's `Read`, `Grep` and `Glob` answer for every tddy
//! worktree on its host, the coding agents' declared home directories and its own grants, by
//! absolute path; anything else is refused naming `RequestPathAccess`. A worker session stays
//! confined to its own worktree (`#orchestra` 1/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.

use std::path::{Path, PathBuf};

use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_activity::path_grants::{PathGrant, PathGrantMode, PathGrantStore};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const HOME_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3a";
const OTHER_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3b";

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

fn a_project(
    project_id: &str,
    name: &str,
    repo: &Path,
) -> tddy_projects::project_storage::ProjectData {
    tddy_projects::project_storage::ProjectData {
        project_id: project_id.to_string(),
        name: name.to_string(),
        git_url: String::new(),
        main_repo_path: repo.display().to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: Default::default(),
        accounts: Vec::new(),
        local_gh: false,
        home_session_id: None,
        orchestrator_defaults: None,
        lifecycle: Default::default(),
    }
}

/// A host with two tddy projects, an orchestrator session of the first, and one of its workers.
struct Host {
    service: TestDaemon,
    orchestrator: String,
    worker: String,
    worker_worktree: PathBuf,
    other_project_repo: PathBuf,
    sessions: tempfile::TempDir,
    outside: tempfile::TempDir,
    _repos: (tempfile::TempDir, tempfile::TempDir),
}

impl Host {
    async fn new() -> Self {
        let home_repo = a_git_repo_with_origin();
        let other_repo = a_git_repo_with_origin();
        std::fs::write(other_repo.path().join("api.rs"), "pub fn other_api() {}\n").unwrap();
        let sessions = tempfile::tempdir().unwrap();
        tddy_projects::project_storage::write_projects(
            &sessions.path().join("projects"),
            &[
                a_project(HOME_PROJECT, "home", home_repo.path()),
                a_project(OTHER_PROJECT, "other", other_repo.path()),
            ],
        )
        .expect("register projects");
        let service = test_service(sessions.path().to_path_buf());

        let orchestrator = start_workspace(&service).await;
        let worker = start_workspace(&service).await;
        set_role(sessions.path(), &orchestrator, "orchestrator");
        let worker_worktree = worktree_of(sessions.path(), &worker);
        std::fs::write(
            worker_worktree.join("progress.md"),
            "step 3 of 5: tests written\n",
        )
        .unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "not for you\n").unwrap();

        Self {
            service,
            orchestrator,
            worker,
            worker_worktree,
            other_project_repo: other_repo.path().canonicalize().unwrap(),
            sessions,
            outside,
            _repos: (home_repo, other_repo),
        }
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
            .unwrap_or_else(|s| panic!("{tool} must be answered, not refused at the RPC: {s:?}"))
            .into_inner()
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        tddy_core::session_lifecycle::unified_session_dir_path(self.sessions.path(), session_id)
    }
}

async fn start_workspace(service: &TestDaemon) -> String {
    service
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: HOME_PROJECT.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session must start")
        .into_inner()
        .session_id
}

fn worktree_of(sessions_base: &Path, session_id: &str) -> PathBuf {
    let dir = tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    PathBuf::from(
        tddy_core::read_session_metadata(&dir)
            .expect("session metadata")
            .repo_path
            .expect("workspace worktree"),
    )
    .canonicalize()
    .unwrap()
}

fn set_role(sessions_base: &Path, session_id: &str, role: &str) {
    let dir = tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let mut meta = tddy_core::read_session_metadata(&dir).expect("session metadata");
    meta.session_role = Some(role.to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).expect("write metadata");
}

fn read_args(path: &Path) -> serde_json::Value {
    serde_json::json!({ "path": path })
}

#[tokio::test]
async fn an_orchestrator_reads_a_file_in_another_projects_worktree_by_absolute_path() {
    // Given an orchestrator of one project and a second project on the same host
    let host = Host::new().await;

    // When it reads the other project's file by absolute path
    let read = host
        .call(
            &host.orchestrator,
            "Read",
            read_args(&host.other_project_repo.join("api.rs")),
        )
        .await;

    // Then the file's content is returned
    assert!(!read.is_error, "{}", read.error_message);
    assert!(
        read.result_json.contains("other_api"),
        "{}",
        read.result_json
    );
}

#[tokio::test]
async fn an_orchestrator_greps_across_a_sibling_sessions_worktree() {
    // Given an orchestrator and a worker whose worktree holds a progress note
    let host = Host::new().await;

    // When it greps the worker's worktree by absolute path
    let grep = host
        .call(
            &host.orchestrator,
            "Grep",
            serde_json::json!({ "pattern": "tests written", "path": host.worker_worktree }),
        )
        .await;

    // Then the match is found
    assert!(!grep.is_error, "{}", grep.error_message);
    assert!(
        grep.result_json.contains("progress.md"),
        "{}",
        grep.result_json
    );
}

#[tokio::test]
async fn an_orchestrator_reads_a_file_under_an_agents_declared_home_dir() {
    // Given Claude's persistent home holds a chat transcript
    let host = Host::new().await;
    let transcript_dir = host
        .sessions
        .path()
        .join("homes/claude/.claude/projects/repo");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    let transcript = transcript_dir.join("conversation.jsonl");
    std::fs::write(&transcript, "{\"type\":\"assistant\",\"text\":\"done\"}\n").unwrap();

    // When the orchestrator reads it by absolute path
    let read = host
        .call(&host.orchestrator, "Read", read_args(&transcript))
        .await;

    // Then the transcript is returned
    assert!(!read.is_error, "{}", read.error_message);
    assert!(
        read.result_json.contains("assistant"),
        "{}",
        read.result_json
    );
}

#[tokio::test]
async fn a_path_outside_reach_is_refused_naming_request_path_access() {
    // Given a file outside every tddy worktree and agent home
    let host = Host::new().await;

    // When the orchestrator reads it
    let read = host
        .call(
            &host.orchestrator,
            "Read",
            read_args(&host.outside.path().join("secret.txt")),
        )
        .await;

    // Then it is refused, and told how to ask for it
    assert!(read.is_error, "{}", read.result_json);
    assert!(
        read.error_message.contains("RequestPathAccess"),
        "{}",
        read.error_message
    );
}

#[tokio::test]
async fn a_granted_path_is_readable() {
    // Given the operator granted the orchestrator a path outside its default reach
    let host = Host::new().await;
    PathGrantStore::open(&host.session_dir(&host.orchestrator))
        .add(PathGrant {
            path: host.outside.path().canonicalize().unwrap(),
            mode: PathGrantMode::Read,
            granted_at_unix_ms: 1_795_000_000_000,
            reason: "read the shared fixture".to_string(),
        })
        .unwrap();

    // When it reads a file there
    let read = host
        .call(
            &host.orchestrator,
            "Read",
            read_args(&host.outside.path().join("secret.txt")),
        )
        .await;

    // Then the file is returned
    assert!(!read.is_error, "{}", read.error_message);
    assert!(
        read.result_json.contains("not for you"),
        "{}",
        read.result_json
    );
}

#[tokio::test]
async fn a_worker_sessions_reads_stay_confined_to_its_worktree() {
    // Given a worker session (no orchestrator role)
    let host = Host::new().await;

    // When it reads another project's file by absolute path
    let read = host
        .call(
            &host.worker,
            "Read",
            read_args(&host.other_project_repo.join("api.rs")),
        )
        .await;

    // Then it is refused as escaping its worktree
    assert!(read.is_error, "{}", read.result_json);
    assert!(
        read.error_message.contains("escapes worktree"),
        "{}",
        read.error_message
    );
}
