//! Acceptance: a project-home session lists, starts, reads and diffs its project's other sessions
//! through four `ExecuteTool` tools — scoped to its own project, refused for any other session.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools (`#project-home` 5/7).

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_rpc::test_util::{test_service, TestDaemon};
use tddy_projects::project_storage::{write_projects, ProjectData};
use tddy_rpc::{Code, Request};
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
    ListExecToolsResponse,
};
use tddy_service::proto::session::{
    ListSessionsRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_session_lifecycle::CliSessionManager;

const THIS_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3b";
const OTHER_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3c";
const TEST_MODEL: &str = "claude-opus-4-8";

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn git(cwd: &Path, args: &[&str]) {
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

fn a_repo_with_origin() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    git(
        repo.path(),
        &["commit", "-q", "--allow-empty", "-m", "init"],
    );
    git(
        repo.path(),
        &["remote", "add", "origin", repo.path().to_str().unwrap()],
    );
    git(repo.path(), &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn a_project(project_id: &str, repo: &Path) -> ProjectData {
    ProjectData {
        project_id: project_id.to_string(),
        name: project_id.to_string(),
        git_url: String::new(),
        main_repo_path: repo.display().to_string(),
        main_branch_ref: Some("origin/main".to_string()),
        remote_name: None,
        host_repo_paths: Default::default(),
        accounts: Vec::new(),
        local_gh: false,
        home_session_id: None,
    }
}

/// A daemon with two projects: a project-home session and two ordinary sessions in this project,
/// and one session in the other project.
struct World {
    service: TestDaemon,
    sessions: tempfile::TempDir,
    home: String,
    sibling: String,
    second_sibling: String,
    foreign: String,
    _repos: (tempfile::TempDir, tempfile::TempDir),
}

impl World {
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        let dir = tddy_core::session_lifecycle::unified_session_dir_path(
            self.sessions.path(),
            session_id,
        );
        PathBuf::from(
            tddy_core::read_session_metadata(&dir)
                .expect("session metadata")
                .repo_path
                .expect("worktree"),
        )
    }

    async fn call_as(
        &self,
        session_id: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ExecuteToolResponse, tddy_rpc::Status> {
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
    }

    async fn call_as_home(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.call_as(&self.home, tool, args)
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
    }
}

async fn a_workspace_in(service: &TestDaemon, project_id: &str) -> String {
    service
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project_id.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session starts")
        .into_inner()
        .session_id
}

async fn a_project_home_world() -> World {
    let sessions = tempfile::tempdir().unwrap();
    a_project_home_world_on(test_service(sessions.path().to_path_buf()), sessions).await
}

/// The same world, on a daemon that can spawn `claude-cli` sessions: its config points
/// `claude_cli.binary_path` at `/bin/cat` (works in a PTY without the real binary — same stub as
/// `claude_cli_session_acceptance.rs`), and its user resolver answers the current OS user, whom
/// the spawn impersonates (a fabricated name does not resolve during the spawn).
async fn a_claude_capable_project_home_world() -> World {
    let sessions = tempfile::tempdir().unwrap();
    let config = a_config_with_a_stub_claude();
    let user = current_os_user();
    let user_resolver: UserResolver = Arc::new(move |token| {
        if token == TEST_TOKEN {
            Some(user.clone())
        } else {
            None
        }
    });
    let sessions_base = sessions.path().to_path_buf();
    let base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(sessions_base.clone()));
    let host = DaemonSessionHost::new(
        config.clone(),
        base_resolver,
        sessions.path().to_path_buf(),
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
    );
    let service = TestDaemon::from_host(host);
    service.as_arc().install_sandbox_rpc_bridge();
    a_project_home_world_on(service, sessions).await
}

async fn a_project_home_world_on(service: TestDaemon, sessions: tempfile::TempDir) -> World {
    let this_repo = a_repo_with_origin();
    let other_repo = a_repo_with_origin();
    write_projects(
        &sessions.path().join("projects"),
        &[
            a_project(THIS_PROJECT, this_repo.path()),
            a_project(OTHER_PROJECT, other_repo.path()),
        ],
    )
    .unwrap();
    let home = a_workspace_in(&service, THIS_PROJECT).await;
    let sibling = a_workspace_in(&service, THIS_PROJECT).await;
    let second_sibling = a_workspace_in(&service, THIS_PROJECT).await;
    let foreign = a_workspace_in(&service, OTHER_PROJECT).await;
    let home_dir = tddy_core::session_lifecycle::unified_session_dir_path(sessions.path(), &home);
    let mut meta = tddy_core::read_session_metadata(&home_dir).unwrap();
    meta.session_role = Some("project-home".to_string());
    tddy_core::session_metadata::write_session_metadata(&home_dir, &meta).unwrap();
    World {
        service,
        sessions,
        home,
        sibling,
        second_sibling,
        foreign,
        _repos: (this_repo, other_repo),
    }
}

/// The OS user this test process runs as — a real, resolvable one. The claude-cli spawn
/// impersonates the resolved user, and a fabricated name does not resolve there (same pattern as
/// `sandboxed_codebase_lifecycle_exec_tool_acceptance.rs`).
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A daemon config whose `claude` is `/bin/cat` — a stub that works in a PTY without the real
/// binary (a positional argument would make `cat` open it as a file, but the start under test
/// never gets that far on the red side, and the metadata assertion needs no output on green).
fn a_config_with_a_stub_claude() -> DaemonConfig {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
claude_cli:
  binary_path: /bin/cat
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    DaemonConfig::load(&config_path).expect("config must parse")
}

fn listed_ids(response: &ExecuteToolResponse) -> Vec<String> {
    let listed: serde_json::Value =
        serde_json::from_str(&response.result_json).expect("JSON result");
    let mut ids: Vec<String> = listed["sessions"]
        .as_array()
        .expect("a sessions array")
        .iter()
        .map(|s| s["session_id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn a_project_home_session_lists_its_projects_sessions_with_status() {
    // Given
    let world = a_project_home_world().await;

    // When
    let listed = world
        .call_as_home("ListProjectSessions", serde_json::json!({}))
        .await;

    // Then — this project's other sessions, not itself, not the other project's
    assert!(!listed.is_error, "{}", listed.error_message);
    let mut expected = vec![world.sibling.clone(), world.second_sibling.clone()];
    expected.sort();
    assert_eq!(listed_ids(&listed), expected);
    let first: serde_json::Value = serde_json::from_str(&listed.result_json).unwrap();
    let row = &first["sessions"][0];
    assert!(row["worktree"].as_str().is_some_and(|w| !w.is_empty()));
    assert!(row.get("is_active").is_some() && row.get("agent_status").is_some());
}

#[tokio::test]
async fn a_project_home_session_starts_a_session_tagged_with_itself_as_orchestrator() {
    // Given
    let world = a_project_home_world().await;

    // When
    let started = world
        .call_as_home(
            "StartSession",
            serde_json::json!({ "session_type": "workspace" }),
        )
        .await;

    // Then
    assert!(!started.is_error, "{}", started.error_message);
    let new_id =
        support::session_started_by(&world.service, &world.home, &started.result_json).await;
    let listed = world
        .service
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
        }))
        .await
        .unwrap()
        .into_inner()
        .sessions;
    let new_session = listed
        .iter()
        .find(|s| s.session_id == new_id)
        .expect("listed");
    assert_eq!(new_session.project_id, THIS_PROJECT);
    assert_eq!(new_session.orchestrator_session_id, world.home);
}

/// A project-home session starts a claude-cli session on the model it names in the tool call.
///
/// The buggy behavior: the start tool's arguments (then `StartProjectSession`) carried no `model`,
/// its schema advertised none, so a home session's `model` was dropped before the daemon's start, and every
/// `session_type = "claude-cli"` start through this tool was refused with "model is required for
/// claude-cli sessions" — however the caller spelled the arguments.
#[tokio::test]
async fn a_project_home_session_starts_a_claude_cli_session_on_the_model_it_names() {
    // Given
    let world = a_claude_capable_project_home_world().await;

    // When
    let started = world
        .call_as_home(
            "StartSession",
            serde_json::json!({
                "session_type": "claude-cli",
                "model": TEST_MODEL,
                "initial_prompt": "print the current branch and stop",
                "branch_worktree_intent": "new_branch_from_base",
                "new_branch_name": "home-started-claude-cli"
            }),
        )
        .await;

    // Then — the start is not refused for a missing model, and the session it started records
    // exactly the model the home session named
    assert!(!started.is_error, "{}", started.error_message);
    let new_id =
        support::session_started_by(&world.service, &world.home, &started.result_json).await;
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(world.sessions.path(), &new_id);
    let meta = tddy_core::read_session_metadata(&session_dir).expect("session metadata");
    assert_eq!(
        meta.model.as_deref(),
        Some(TEST_MODEL),
        "the named model is recorded"
    );
}

#[tokio::test]
async fn a_project_home_session_reads_a_file_and_lists_a_directory_in_another_worktree() {
    // Given
    let world = a_project_home_world().await;
    let worktree = world.worktree_of(&world.sibling);
    std::fs::create_dir_all(worktree.join("notes")).unwrap();
    std::fs::write(worktree.join("notes/plan.md"), "ship the parser\n").unwrap();

    // When
    let file = world
        .call_as_home(
            "ReadSessionWorktree",
            serde_json::json!({ "session_id": world.sibling, "path": "notes/plan.md" }),
        )
        .await;
    let dir = world
        .call_as_home(
            "ReadSessionWorktree",
            serde_json::json!({ "session_id": world.sibling, "path": "notes" }),
        )
        .await;

    // Then
    assert!(!file.is_error, "{}", file.error_message);
    assert!(file.result_json.contains("ship the parser"));
    assert!(!dir.is_error, "{}", dir.error_message);
    assert!(dir.result_json.contains("plan.md"));
}

#[tokio::test]
async fn a_project_home_session_gets_a_target_worktrees_diff_against_its_base() {
    // Given an uncommitted change in the sibling's worktree
    let world = a_project_home_world().await;
    std::fs::write(
        world.worktree_of(&world.sibling).join("added.rs"),
        "fn added() {}\n",
    )
    .unwrap();

    // When
    let diff = world
        .call_as_home(
            "DiffSessionWorktree",
            serde_json::json!({ "session_id": world.sibling }),
        )
        .await;

    // Then
    assert!(!diff.is_error, "{}", diff.error_message);
    assert!(diff.result_json.contains("added.rs"));
    assert!(diff.result_json.contains("fn added()"));
}

#[tokio::test]
async fn a_target_in_another_project_is_refused() {
    // Given
    let world = a_project_home_world().await;

    // When
    let read = world
        .call_as_home(
            "ReadSessionWorktree",
            serde_json::json!({ "session_id": world.foreign, "path": "." }),
        )
        .await;

    // Then
    assert!(read.is_error);
    assert!(
        read.error_message.contains("another project"),
        "{}",
        read.error_message
    );
}

#[tokio::test]
async fn a_path_escaping_the_target_worktree_is_refused() {
    // Given
    let world = a_project_home_world().await;

    // When
    let read = world
        .call_as_home(
            "ReadSessionWorktree",
            serde_json::json!({ "session_id": world.sibling, "path": "../../etc/passwd" }),
        )
        .await;

    // Then
    assert!(read.is_error);
    assert!(
        read.error_message.contains("outside the worktree"),
        "refused for escaping, not for anything else: {}",
        read.error_message
    );
    assert!(!read.result_json.contains("root:"));
}

#[tokio::test]
async fn a_session_without_the_project_home_role_cannot_call_the_tools() {
    // Given
    let world = a_project_home_world().await;

    // When
    let refusal = world
        .call_as(&world.sibling, "ListProjectSessions", serde_json::json!({}))
        .await
        .expect_err("an ordinary session is refused");

    // Then
    assert_eq!(refusal.code(), Code::PermissionDenied);
}

/// The tool names `ListExecTools` offers `session_id`, sorted.
async fn offered_tool_names(world: &World, session_id: &str) -> Vec<String> {
    let listed: ListExecToolsResponse = world
        .service
        .list_exec_tools(Request::direct(ListExecToolsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("the session's tools are listed")
        .into_inner();
    let mut names: Vec<String> = listed.tools.into_iter().map(|tool| tool.name).collect();
    names.sort();
    names
}

fn sorted_names(names: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_project_home_session_is_offered_its_read_only_tools_and_the_project_home_tools() {
    // Given
    let world = a_project_home_world().await;

    // When
    let offered = offered_tool_names(&world, &world.home).await;

    // Then exactly the read-only tools, the project-home tools and the seven git tools
    assert_eq!(
        offered,
        sorted_names(&[
            "Read",
            "Grep",
            "Glob",
            "ReadLints",
            "SemanticSearch",
            "ListProjectSessions",
            "StartSession",
            "GetSessionStart",
            "CancelSessionStart",
            "DescribeSessionStartOptions",
            "ReadSessionWorktree",
            "DiffSessionWorktree",
            "ListWorktrees",
            "GetWorktree",
            "ReadSessionActivity",
            "ReadSessionTranscript",
            "ReadSessionTerminal",
            "ClaimSessionTerminal",
            "SendSessionKeys",
            "AnswerSessionPermission",
            "AnswerSessionQuestion",
            "WaitProjectNotifications",
            "ReadProjectNotifications",
            "TerminateSession",
            "CommitSessionWorktree",
            "PushSessionBranch",
            "RebaseSessionBranch",
            "StashSessionWorktree",
            "ResetSessionWorktree",
            "CheckoutSessionWorktree",
            "CleanSessionWorktree",
            "RefreshWorktreeDiskUsage",
        ])
    );
}

#[tokio::test]
async fn an_ordinary_session_is_offered_the_catalog_without_the_project_home_tools() {
    // Given
    let world = a_project_home_world().await;
    let catalog: Vec<String> = tddy_tool_engine::tool_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    // When
    let offered = offered_tool_names(&world, &world.sibling).await;

    // Then the whole catalog, and none of the project-home tools
    let mut expected = catalog;
    expected.sort();
    assert_eq!(offered, expected);
    assert!(!offered.iter().any(|name| name == "ListProjectSessions"));
    assert!(!offered.iter().any(|name| name == "CommitSessionWorktree"));
}
