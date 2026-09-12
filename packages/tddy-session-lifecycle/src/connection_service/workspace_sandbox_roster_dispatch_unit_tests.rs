use super::*;
use crate::test_util::{test_service, TestDaemon, TEST_TOKEN};
use std::sync::Mutex;
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    WorkspaceSandbox, WorkspaceSandboxProvisioner, WorkspaceSandboxSpec,
};
use tddy_sandbox::SandboxError;

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a44";
const JAIL_MARKER: &str = "ran-inside-the-workspace-jail";
const AGENT_ID: &str = "reviewer";

/// A jail that records what it ran and touches no filesystem, so an unchanged host worktree is
/// proof the call never reached the host tool engine.
#[derive(Default)]
struct RecordingSandbox {
    tools: Mutex<Vec<String>>,
}

impl RecordingSandbox {
    fn tools(&self) -> Vec<String> {
        self.tools.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl WorkspaceSandbox for RecordingSandbox {
    async fn execute_tool(&self, req: &ExecuteToolRequest) -> ExecuteToolResponse {
        self.tools.lock().unwrap().push(req.tool_name.clone());
        ExecuteToolResponse {
            result_json: serde_json::json!({ "marker": JAIL_MARKER }).to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }

    fn stop(&self) {}
}

#[derive(Default)]
struct RecordingProvisioner {
    sandbox: Arc<RecordingSandbox>,
}

#[async_trait::async_trait]
impl WorkspaceSandboxProvisioner for RecordingProvisioner {
    async fn provision(
        &self,
        _spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError> {
        Ok(Arc::clone(&self.sandbox) as Arc<dyn WorkspaceSandbox>)
    }
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

/// A sandboxed workspace session, plus the jail standing in for its confinement.
struct SeededWorkspace {
    service: TestDaemon,
    sandbox: Arc<RecordingSandbox>,
    session_id: String,
    session_dir: PathBuf,
    worktree: PathBuf,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

async fn a_sandboxed_workspace_session(sandbox: bool) -> SeededWorkspace {
    let repo = a_git_repo_with_origin();
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    crate::project_storage::write_projects(
        &sessions.path().join("projects"),
        &[crate::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "workspace-sandbox-roster".to_string(),
            git_url: String::new(),
            main_repo_path: repo.path().display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");

    let provisioner = Arc::new(RecordingProvisioner::default());
    let sandbox_double = Arc::clone(&provisioner.sandbox);
    let service = test_service(sessions.path().to_path_buf())
        .with_workspace_sandbox_provisioner(provisioner as Arc<dyn WorkspaceSandboxProvisioner>);

    let started = service
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: PROJECT_ID.to_string(),
            sandbox,
            ..Default::default()
        }))
        .await
        .expect("a workspace session must start")
        .into_inner();

    let session_dir = unified_session_dir_path(sessions.path(), &started.session_id);
    let worktree = PathBuf::from(
        tddy_core::read_session_metadata(&session_dir)
            .expect("session metadata")
            .repo_path
            .expect("workspace worktree"),
    );

    SeededWorkspace {
        service,
        sandbox: sandbox_double,
        session_id: started.session_id,
        session_dir,
        worktree,
        _repo: repo,
        _sessions: sessions,
    }
}

impl SeededWorkspace {
    /// How a roster agent this daemon serves locally reaches the session's files.
    fn agent_codebase_access(&self) -> tddy_discovery::subagent::CodebaseAccess {
        self.service.local_agent_codebase_access(
            &self.session_id,
            &self.session_dir,
            AGENT_ID,
            TEST_TOKEN,
        )
    }
}

/// A specialized agent seeded on the codebase host runs its loop here, so a jail that confined
/// only the remote `ExecuteTool` callers would leave the agent writing the bare host —
/// the larger hole of the two, because the agent's loop is the thing that decides what to write.
#[tokio::test]
async fn a_roster_agents_mutation_on_a_sandboxed_workspace_session_goes_through_the_jail() {
    // Given
    let workspace = a_sandboxed_workspace_session(true).await;

    // When
    let result = workspace
        .agent_codebase_access()
        .write("agent-wrote.txt", "from the agent")
        .await
        .expect("the agent's write must be answered");

    // Then
    assert_eq!(
        result.get("marker").and_then(|v| v.as_str()),
        Some(JAIL_MARKER)
    );
    assert_eq!(workspace.sandbox.tools(), vec!["Write".to_string()]);
    assert!(
        !workspace.worktree.join("agent-wrote.txt").exists(),
        "the host tool engine must not have run: the jail owns this write"
    );
}

/// `Shell` is the agent tool with no path argument to check, so it is the one that most needs
/// the kernel boundary rather than the tool engine's own.
#[tokio::test]
async fn a_roster_agents_shell_on_a_sandboxed_workspace_session_goes_through_the_jail() {
    // Given
    let workspace = a_sandboxed_workspace_session(true).await;

    // When
    let result = workspace
        .agent_codebase_access()
        .shell("echo hi", Some(5_000))
        .await
        .expect("the agent's shell must be answered");

    // Then
    assert_eq!(
        result.get("marker").and_then(|v| v.as_str()),
        Some(JAIL_MARKER)
    );
    assert_eq!(workspace.sandbox.tools(), vec!["Shell".to_string()]);
}

/// The control: on an unsandboxed workspace session the agent still reaches the host worktree
/// directly, so the assertions above are about the sandbox flag and not about roster agents.
#[tokio::test]
async fn a_roster_agents_mutation_on_an_unsandboxed_workspace_session_reaches_the_host_worktree() {
    // Given
    let workspace = a_sandboxed_workspace_session(false).await;

    // When
    workspace
        .agent_codebase_access()
        .write("agent-wrote.txt", "from the agent")
        .await
        .expect("the agent's write must be answered");

    // Then
    assert_eq!(workspace.sandbox.tools(), Vec::<String>::new());
    assert_eq!(
        std::fs::read_to_string(workspace.worktree.join("agent-wrote.txt"))
            .expect("an unsandboxed agent writes straight to the worktree"),
        "from the agent"
    );
}
