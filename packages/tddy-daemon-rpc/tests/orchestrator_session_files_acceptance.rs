//! Acceptance: an orchestrator writes files into its own session directory and lists them, through
//! the daemon's `ExecuteTool`; each write answers with the `StartSession` attachment that hands the
//! file to a child; a name that is not one flat file name is refused, and a worker session cannot
//! write one (`#orchestra` 3/9).
//!
//! Feature doc: docs/ft/coder/session-attachments.md.

use std::path::{Path, PathBuf};

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN};

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
                name: "files".to_string(),
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

#[tokio::test]
async fn an_orchestrator_writes_a_session_file_into_its_own_session() {
    // Given an orchestrator
    let project = Project::new().await;

    // When it writes a brief
    let written = project
        .call(
            &project.orchestrator,
            "WriteSessionFile",
            serde_json::json!({ "name": "brief.md", "content": "# Add the outbox\n" }),
        )
        .await;

    // Then the file is in its session's files
    result_of(&written);
    assert_eq!(
        std::fs::read_to_string(
            project
                .session_dir(&project.orchestrator)
                .join("artifacts/files/brief.md")
        )
        .expect("the file was written"),
        "# Add the outbox\n"
    );
}

#[tokio::test]
async fn a_written_session_file_answers_with_the_attachment_that_hands_it_to_a_child() {
    // Given an orchestrator
    let project = Project::new().await;

    // When it writes a brief
    let written = project
        .call(
            &project.orchestrator,
            "WriteSessionFile",
            serde_json::json!({ "name": "brief.md", "content": "# Brief\n" }),
        )
        .await;

    // Then the answer carries the StartSession attachment for it: its session artifact under files/,
    // held by this daemon
    assert_eq!(
        result_of(&written)["attachment"],
        serde_json::json!({
            "basename": "brief.md",
            "daemon_instance_id": local_instance_id_for_config(&test_config()),
            "scope": "HOST_DOCUMENT_SCOPE_SESSION_ARTIFACT",
            "session_id": project.orchestrator,
            "relative_path": "files/brief.md"
        })
    );
}

#[tokio::test]
async fn an_orchestrator_lists_its_session_files() {
    // Given an orchestrator that wrote two files
    let project = Project::new().await;
    for name in ["plan.md", "brief.md"] {
        project
            .call(
                &project.orchestrator,
                "WriteSessionFile",
                serde_json::json!({ "name": name, "content": "x" }),
            )
            .await;
    }

    // When it lists them
    let listed = project
        .call(
            &project.orchestrator,
            "ListSessionFiles",
            serde_json::json!({}),
        )
        .await;

    // Then both are listed by name
    let names: Vec<String> = result_of(&listed)["files"]
        .as_array()
        .expect("a files array")
        .iter()
        .map(|f| f["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(names, vec!["brief.md".to_string(), "plan.md".to_string()]);
}

#[tokio::test]
async fn a_session_file_name_with_a_path_separator_is_refused() {
    // Given an orchestrator
    let project = Project::new().await;

    // When it names a file in another directory
    let refused = project
        .call(
            &project.orchestrator,
            "WriteSessionFile",
            serde_json::json!({ "name": "../escape.md", "content": "x" }),
        )
        .await;

    // Then it is refused for its name, and nothing is written outside its files
    assert!(refused.is_error, "{}", refused.result_json);
    assert!(
        refused.error_message.contains("flat file name"),
        "{}",
        refused.error_message
    );
    assert!(!project
        .session_dir(&project.orchestrator)
        .join("artifacts/escape.md")
        .exists());
}

#[tokio::test]
async fn a_worker_session_cannot_write_a_session_file() {
    // Given a worker session (no orchestrator role)
    let project = Project::new().await;

    // When it tries to write one
    let refused = project
        .call(
            &project.worker,
            "WriteSessionFile",
            serde_json::json!({ "name": "brief.md", "content": "x" }),
        )
        .await;

    // Then it is refused and nothing is written
    assert!(refused.is_error, "{}", refused.result_json);
    assert!(!project
        .session_dir(&project.worker)
        .join("artifacts/files/brief.md")
        .exists());
}
