//! Acceptance: a project-home session lists its project's sessions on every host and reaches a
//! session on another host with the same tools it uses on its own (`#start-parity` 1/9).
//!
//! Two daemons, `host-a` (holding the project-home session) and `host-b`, run in this process and
//! reach each other through an in-memory [`PeerForwarder`] that hands each forwarded call to the
//! other daemon's own service, stamped as arriving over LiveKit — the transport a real peer's call
//! arrives on. A third eligible host, `host-c`, has no daemon behind it: it never answers.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-remote-targets.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use prost::Message;
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_projects::project_storage::{write_projects, ProjectData};
use tddy_rpc::{Code, Request, RequestMetadata, RequestTransport, Status};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{
    ListSessionsRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};
use tddy_session_lifecycle::CliSessionManager;
use tddy_tool_engine::exec_tool_entry::EXEC_TOOL_SERVICE;
use tokio::sync::mpsc;

const THIS_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3b";
const OTHER_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3c";
const HOST_A: &str = "host-a";
const HOST_B: &str = "host-b";
const SILENT_HOST: &str = "host-c";
const SESSION_SERVICE: &str = "session.SessionService";

// ── the hosts ────────────────────────────────────────────────────────────────────────────────

/// Every eligible host, as each daemon's roster lists them: the two daemons and one that never
/// answers.
struct TheCommonRoom;

#[async_trait::async_trait]
impl EligibleDaemonSource for TheCommonRoom {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        [HOST_A, HOST_B, SILENT_HOST]
            .into_iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.to_string()),
                label: id.to_string(),
            })
            .collect()
    }
}

/// The transport between the in-process daemons: a forwarded call is decoded and handed to the
/// addressed daemon's own service, exactly as its LiveKit RPC participant would. A host with no
/// daemon registered here is unreachable.
#[derive(Default)]
struct InProcessPeers {
    daemons: RwLock<HashMap<String, Arc<TestDaemon>>>,
}

impl InProcessPeers {
    fn register(&self, instance_id: &str, daemon: Arc<TestDaemon>) {
        self.daemons
            .write()
            .unwrap()
            .insert(instance_id.to_string(), daemon);
    }

    fn daemon(&self, peer_id: &str) -> Result<Arc<TestDaemon>, Status> {
        self.daemons
            .read()
            .unwrap()
            .get(peer_id)
            .cloned()
            .ok_or_else(|| Status::unavailable(format!("daemon {peer_id} does not answer")))
    }
}

fn arriving_from_a_peer<T>(inner: T) -> Request<T> {
    Request::with_metadata(inner, RequestMetadata::over(RequestTransport::LiveKit))
}

#[async_trait::async_trait]
impl PeerForwarder for InProcessPeers {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let daemon = self.daemon(peer_id)?;
        match (service, method) {
            (EXEC_TOOL_SERVICE, "ExecuteTool") => {
                let req = ExecuteToolRequest::decode(body.as_slice()).unwrap();
                daemon
                    .execute_tool(arriving_from_a_peer(req))
                    .await
                    .map(|r| r.into_inner().encode_to_vec())
            }
            (SESSION_SERVICE, "ListSessions") => {
                let req = ListSessionsRequest::decode(body.as_slice()).unwrap();
                daemon
                    .list_sessions(arriving_from_a_peer(req))
                    .await
                    .map(|r| r.into_inner().encode_to_vec())
            }
            other => Err(Status::unimplemented(format!(
                "the in-process peers do not serve {other:?}"
            ))),
        }
    }

    async fn call_server_stream(
        &self,
        _peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented(format!(
            "the in-process peers serve no stream ({service}/{method})"
        )))
    }
}

fn a_daemon_named(
    instance_id: &str,
    sessions_base: &Path,
    peers: Arc<InProcessPeers>,
) -> TestDaemon {
    let mut config = test_config();
    config.daemon_instance_id = Some(instance_id.to_string());
    let base = sessions_base.to_path_buf();
    let routing = PeerRouting::new(
        config.clone(),
        Arc::new(TheCommonRoom),
        Some(CommonRoom::new(peers, &config)),
    );
    let host = DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(base.clone())),
        sessions_base.to_path_buf(),
        Arc::new(|token| (token == TEST_TOKEN).then(|| TEST_USER.to_string())),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        routing,
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
    );
    let daemon = TestDaemon::from_host(host);
    daemon.as_arc().install_sandbox_rpc_bridge();
    daemon
}

// ── the projects and sessions ────────────────────────────────────────────────────────────────

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

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn a_repo_with_origin() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    // The daemon's own git commands (a session worktree's commit) do not carry the test's env
    // identity; a CI runner has no global one.
    git(repo.path(), &["config", "user.name", "Test"]);
    git(repo.path(), &["config", "user.email", "t@t.com"]);
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

async fn a_workspace_in(daemon: &TestDaemon, project_id: &str) -> String {
    daemon
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

/// One daemon's half of the world: its sessions directory and the repos its projects point at.
struct Host {
    daemon: Arc<TestDaemon>,
    sessions: tempfile::TempDir,
    _repos: (tempfile::TempDir, tempfile::TempDir),
}

impl Host {
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

    fn make_project_home(&self, session_id: &str) {
        let dir = tddy_core::session_lifecycle::unified_session_dir_path(
            self.sessions.path(),
            session_id,
        );
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_role = Some("project-home".to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }
}

fn a_host(instance_id: &str, peers: &Arc<InProcessPeers>) -> Host {
    let sessions = tempfile::tempdir().unwrap();
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
    let daemon = Arc::new(a_daemon_named(
        instance_id,
        sessions.path(),
        Arc::clone(peers),
    ));
    peers.register(instance_id, Arc::clone(&daemon));
    Host {
        daemon,
        sessions,
        _repos: (this_repo, other_repo),
    }
}

/// The project-home session on `host-a`, one more session of its project on each host, and a
/// session of another project on `host-b`.
struct TwoHosts {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    on_b: String,
    foreign_on_b: String,
}

async fn two_hosts() -> TwoHosts {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    let foreign_on_b = a_workspace_in(&b.daemon, OTHER_PROJECT).await;
    TwoHosts {
        a,
        b,
        home,
        on_a,
        on_b,
        foreign_on_b,
    }
}

impl TwoHosts {
    async fn call_on_a(
        &self,
        session_id: &str,
        request_daemon: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ExecuteToolResponse, Status> {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: request_daemon.to_string(),
                conversation_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    /// `tool` as the home session's agent calls it: on its own host, naming the target host in the
    /// tool's arguments.
    async fn home_calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.call_on_a(&self.home, "", tool, args)
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
    }
}

fn listed(response: &ExecuteToolResponse) -> serde_json::Value {
    assert!(!response.is_error, "{}", response.error_message);
    serde_json::from_str(&response.result_json).expect("JSON result")
}

fn session_hosts(listing: &serde_json::Value) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = listing["sessions"]
        .as_array()
        .expect("a sessions array")
        .iter()
        .map(|s| {
            (
                s["session_id"].as_str().unwrap().to_string(),
                s["daemon_instance_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

// ── listing ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_project_sessions_reports_the_projects_sessions_on_every_host_with_their_host() {
    // Given one session of the project on each host besides the home session
    let world = two_hosts().await;

    // When the home session lists its project's sessions
    let listing = listed(
        &world
            .home_calls("ListProjectSessions", serde_json::json!({}))
            .await,
    );

    // Then both are listed, each with the host it runs on, and nothing of the other project is
    let mut expected = vec![
        (world.on_a.clone(), HOST_A.to_string()),
        (world.on_b.clone(), HOST_B.to_string()),
    ];
    expected.sort();
    assert_eq!(session_hosts(&listing), expected);
}

#[tokio::test]
async fn a_host_that_does_not_answer_is_named_not_dropped() {
    // Given an eligible host with no daemon answering behind it
    let world = two_hosts().await;

    // When the home session lists its project's sessions
    let listing = listed(
        &world
            .home_calls("ListProjectSessions", serde_json::json!({}))
            .await,
    );

    // Then that host is named as unreachable, and the hosts that answered are still listed
    let unreachable: Vec<&str> = listing["unreachable_hosts"]
        .as_array()
        .expect("an unreachable_hosts array")
        .iter()
        .map(|h| h["daemon_instance_id"].as_str().unwrap())
        .collect();
    assert_eq!(unreachable, vec![SILENT_HOST]);
    assert_eq!(session_hosts(&listing).len(), 2);
}

// ── reaching a session on another host ──────────────────────────────────────────────────────

#[tokio::test]
async fn read_session_worktree_reads_a_file_from_a_session_on_another_host() {
    // Given a file in the worktree of the project's session on host-b
    let world = two_hosts().await;
    std::fs::write(
        world.b.worktree_of(&world.on_b).join("notes.md"),
        "on host b\n",
    )
    .unwrap();

    // When the home session reads it, naming host-b
    let read = world
        .home_calls(
            "ReadSessionWorktree",
            serde_json::json!({
                "session_id": world.on_b,
                "path": "notes.md",
                "daemon_instance_id": HOST_B,
            }),
        )
        .await;

    // Then it reads host-b's file
    assert_eq!(listed(&read)["content"], "on host b\n");
}

#[tokio::test]
async fn diff_session_worktree_diffs_a_session_on_another_host() {
    // Given an uncommitted change in the worktree of the project's session on host-b
    let world = two_hosts().await;
    std::fs::write(
        world.b.worktree_of(&world.on_b).join("added.rs"),
        "fn added_on_b() {}\n",
    )
    .unwrap();

    // When the home session diffs it, naming host-b
    let diff = world
        .home_calls(
            "DiffSessionWorktree",
            serde_json::json!({ "session_id": world.on_b, "daemon_instance_id": HOST_B }),
        )
        .await;

    // Then the diff is host-b's change
    assert!(listed(&diff)["diff"]
        .as_str()
        .unwrap()
        .contains("fn added_on_b()"));
}

#[tokio::test]
async fn commit_session_worktree_commits_on_the_target_host() {
    // Given an uncommitted file in the worktree of the project's session on host-b
    let world = two_hosts().await;
    let worktree = world.b.worktree_of(&world.on_b);
    std::fs::write(worktree.join("committed.rs"), "fn committed() {}\n").unwrap();
    let head_before = git_stdout(&worktree, &["rev-parse", "HEAD"]);

    // When the home session commits it, naming host-b
    let commit = world
        .home_calls(
            "CommitSessionWorktree",
            serde_json::json!({
                "session_id": world.on_b,
                "message": "commit from the orchestrator",
                "daemon_instance_id": HOST_B,
            }),
        )
        .await;

    // Then host-b's worktree has a new commit holding the file
    assert!(!commit.is_error, "{}", commit.error_message);
    assert_ne!(git_stdout(&worktree, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(
        git_stdout(&worktree, &["log", "-1", "--format=%s"]),
        "commit from the orchestrator"
    );
    assert!(
        git_stdout(&worktree, &["show", "--name-only", "--format=", "HEAD"])
            .contains("committed.rs")
    );
}

// ── what stays refused ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_target_outside_the_callers_project_is_refused_on_the_other_host() {
    // Given a session of another project on host-b
    let world = two_hosts().await;

    // When the home session tries to read its worktree, naming host-b
    let read = world
        .home_calls(
            "ReadSessionWorktree",
            serde_json::json!({
                "session_id": world.foreign_on_b,
                "path": "",
                "daemon_instance_id": HOST_B,
            }),
        )
        .await;

    // Then host-b refuses it as another project's
    assert!(read.is_error);
    assert!(
        read.error_message.contains("another project"),
        "{}",
        read.error_message
    );
}

#[tokio::test]
async fn an_ordinary_session_cannot_reach_another_host_by_addressing_the_request_there() {
    // Given an ordinary session on host-a — no project-home role
    let world = two_hosts().await;

    // When its agent addresses a project-home tool's request straight at host-b
    let refused = world
        .call_on_a(
            &world.on_a,
            HOST_B,
            "ReadSessionWorktree",
            serde_json::json!({ "session_id": world.on_b, "path": "" }),
        )
        .await
        .expect_err("a session that is not a project-home session is refused on every host");

    // Then host-b refuses it, having found on host-a that the caller is not a project-home session
    assert_eq!(
        refused.code(),
        Code::PermissionDenied,
        "{}",
        refused.message()
    );
}

#[tokio::test]
async fn the_home_sessions_own_host_still_answers_for_its_own_sessions() {
    // Given a file in the worktree of the project's session on host-a
    let world = two_hosts().await;
    std::fs::write(
        world.a.worktree_of(&world.on_a).join("local.md"),
        "on host a\n",
    )
    .unwrap();

    // When the home session reads it naming no host
    let read = world
        .home_calls(
            "ReadSessionWorktree",
            serde_json::json!({ "session_id": world.on_a, "path": "local.md" }),
        )
        .await;

    // Then it is read on host-a, as before
    assert_eq!(listed(&read)["content"], "on host a\n");
}
