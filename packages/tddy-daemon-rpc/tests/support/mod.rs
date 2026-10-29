//! Two daemons in one process, reaching each other through an in-memory [`PeerForwarder`] — the
//! world the cross-host project-home suites run in (`#start-parity`).
//!
//! `host-a` and `host-b` each hold the same two projects, in their own sessions directory, and see
//! every host on the roster; `host-c` is on the roster with no daemon behind it, so it never
//! answers. A forwarded call — unary, or a start's event stream — is decoded and handed to the
//! addressed daemon's own service, stamped as arriving over LiveKit — the transport a real peer's
//! call arrives on.

#![allow(dead_code)] // each suite uses the part of the world it needs

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use futures_util::StreamExt;
use prost::Message;
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_projects::project_storage::{write_projects, ProjectData};
use tddy_rpc::{Request, RequestMetadata, RequestTransport, Status};
use tddy_service::proto::activity::{ActivityService as _, IsSessionTerminalServedRequest};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::session::{
    ListSessionsRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_service::proto::tasks::{CancelTaskRequest, CancelTaskResponse};
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotificationBus,
};
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::test_util::{test_config, TEST_TOKEN, TEST_USER};
use tddy_session_lifecycle::CliSessionManager;
use tddy_task::TaskId;
use tddy_tool_engine::exec_tool_entry::EXEC_TOOL_SERVICE;
use tokio::sync::mpsc;

pub const THIS_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3b";
pub const OTHER_PROJECT: &str = "019d105b-ac0f-78d3-9a89-409731145a3c";
pub const HOST_A: &str = "host-a";
pub const HOST_B: &str = "host-b";
pub const SILENT_HOST: &str = "host-c";
pub const SESSION_SERVICE: &str = "session.SessionService";
pub const TASKS_SERVICE: &str = "tasks.TaskService";
pub const ACTIVITY_SERVICE: &str = tddy_service::session_activity::ACTIVITY_SERVICE;

// ── the hosts ────────────────────────────────────────────────────────────────────────────────

/// Every eligible host, as each daemon's roster lists them: the two daemons and one that never
/// answers.
pub struct TheCommonRoom;

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
pub struct InProcessPeers {
    daemons: RwLock<HashMap<String, Arc<TestDaemon>>>,
    /// Hosts that take every call and never answer: a peer whose link has gone quiet.
    never_answering: RwLock<HashSet<String>>,
    /// Every unary call made, as `(peer, service, method)`, in the order it was made — answered or not.
    unary_calls: Mutex<Vec<(String, String, String)>>,
}

impl InProcessPeers {
    /// `instance_id` takes every call from now on and never answers it.
    pub fn never_answers(&self, instance_id: &str) {
        self.never_answering
            .write()
            .unwrap()
            .insert(instance_id.to_string());
    }

    /// How many times `method` of `service` was asked of `peer`.
    pub fn times_asked(&self, peer: &str, service: &str, method: &str) -> usize {
        self.unary_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, s, m)| p == peer && s == service && m == method)
            .count()
    }

    pub fn register(&self, instance_id: &str, daemon: Arc<TestDaemon>) {
        self.daemons
            .write()
            .unwrap()
            .insert(instance_id.to_string(), daemon);
    }

    pub fn daemon(&self, peer_id: &str) -> Result<Arc<TestDaemon>, Status> {
        self.daemons
            .read()
            .unwrap()
            .get(peer_id)
            .cloned()
            .ok_or_else(|| Status::unavailable(format!("daemon {peer_id} does not answer")))
    }
}

pub fn arriving_from_a_peer<T>(inner: T) -> Request<T> {
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
        self.unary_calls.lock().unwrap().push((
            peer_id.to_string(),
            service.to_string(),
            method.to_string(),
        ));
        if self.never_answering.read().unwrap().contains(peer_id) {
            std::future::pending::<()>().await;
        }
        let daemon = self.daemon(peer_id)?;
        match (service, method) {
            // A peer asked whether it serves a session's main terminal (`#home-inbox` 10/11).
            (ACTIVITY_SERVICE, "IsSessionTerminalServed") => {
                let req = IsSessionTerminalServedRequest::decode(body.as_slice()).unwrap();
                daemon
                    .connection()
                    .activity_service()
                    .is_session_terminal_served(arriving_from_a_peer(req))
                    .await
                    .map(|r| r.into_inner().encode_to_vec())
            }
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
            // A start's cancel, forwarded by the host the orchestrator is on: the peer's own task
            // registry, which its launch sessions run the start's task in.
            (TASKS_SERVICE, "CancelTask") => {
                let req = CancelTaskRequest::decode(body.as_slice()).unwrap();
                cancel_task_in(&daemon, req)
                    .await
                    .map(|r| r.encode_to_vec())
            }
            other => Err(Status::unimplemented(format!(
                "the in-process peers do not serve {other:?}"
            ))),
        }
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let daemon = self.daemon(peer_id)?;
        match (service, method) {
            // A start addressed at another host: the peer runs it and streams its events back.
            (SESSION_SERVICE, "StreamStartSession") => {
                let req = StartSessionRequest::decode(body.as_slice()).unwrap();
                let mut events = daemon
                    .stream_start_session(arriving_from_a_peer(req))
                    .await?
                    .into_inner();
                let (tx, rx) = mpsc::channel(16);
                tokio::spawn(async move {
                    while let Some(event) = events.next().await {
                        let frame = event.map(|e| e.encode_to_vec());
                        if tx.send(frame).await.is_err() {
                            return;
                        }
                    }
                });
                Ok(rx)
            }
            other => Err(Status::unimplemented(format!(
                "the in-process peers serve no stream {other:?}"
            ))),
        }
    }
}

/// `CancelTask` as the peer's `TaskService` answers it for a task it runs: the registry the daemon's
/// launch sessions own, signalled; `NOT_FOUND` when no such task is registered there.
async fn cancel_task_in(
    daemon: &TestDaemon,
    req: CancelTaskRequest,
) -> Result<CancelTaskResponse, Status> {
    let registry = daemon.connection().task_registry();
    if registry.cancel_task(&TaskId(req.task_id)).await {
        Ok(CancelTaskResponse {
            ok: true,
            message: String::new(),
        })
    } else {
        Err(Status::not_found("task not found"))
    }
}

pub fn a_daemon_named(
    instance_id: &str,
    sessions_base: &Path,
    peers: Arc<InProcessPeers>,
) -> TestDaemon {
    daemon_over(
        test_config(),
        instance_id,
        sessions_base,
        peers,
        Arc::new(CliSessionManager::new()),
    )
}

/// [`a_daemon_named`], except that the test user maps to the OS user this process runs as. A probe
/// that runs as an agent's OS user needs that user to resolve on this machine, which the fixture's
/// `testdev` does not here.
pub fn a_daemon_as_current_user(
    instance_id: &str,
    sessions_base: &Path,
    peers: Arc<InProcessPeers>,
) -> TestDaemon {
    daemon_over(
        config_mapping_test_user_to(&current_os_user()),
        instance_id,
        sessions_base,
        peers,
        Arc::new(CliSessionManager::new()),
    )
}

/// The OS user this test process runs as, which resolves in the passwd database.
pub fn current_os_user() -> String {
    // SAFETY: `getuid` cannot fail, and `getpwuid` returns either null (checked below) or a pointer
    // to a static passwd entry that stays valid until the next `getpw*` call on this thread.
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    // SAFETY: `pw` is non-null, and its `pw_name` is a NUL-terminated C string owned by that entry,
    // copied out here before any other `getpw*` call.
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// The test config, with [`TEST_USER`] mapped to `os_user` rather than the fixture's `testdev`.
fn config_mapping_test_user_to(os_user: &str) -> tddy_daemon_kernel::config::DaemonConfig {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!("users:\n  - github_user: \"{TEST_USER}\"\n    os_user: \"{os_user}\"\n");
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    tddy_daemon_kernel::config::DaemonConfig::load(&path).expect("config must parse")
}

fn daemon_over(
    config: tddy_daemon_kernel::config::DaemonConfig,
    instance_id: &str,
    sessions_base: &Path,
    peers: Arc<InProcessPeers>,
    terminals: Arc<CliSessionManager>,
) -> TestDaemon {
    daemon_over_customised(
        config,
        instance_id,
        sessions_base,
        peers,
        terminals,
        |host| host,
    )
}

/// [`daemon_over`], with `customise` given the host before its handlers are installed on it: the
/// `with_*` a suite wants on it, which cannot be added afterwards.
fn daemon_over_customised(
    mut config: tddy_daemon_kernel::config::DaemonConfig,
    instance_id: &str,
    sessions_base: &Path,
    peers: Arc<InProcessPeers>,
    terminals: Arc<CliSessionManager>,
    customise: impl FnOnce(DaemonSessionHost) -> DaemonSessionHost,
) -> TestDaemon {
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
        terminals,
    );
    let daemon = TestDaemon::from_host(customise(host));
    daemon.as_arc().install_sandbox_rpc_bridge();
    daemon
}

// ── the projects and sessions ────────────────────────────────────────────────────────────────

pub fn git(cwd: &Path, args: &[&str]) {
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

pub fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn a_repo_with_origin() -> tempfile::TempDir {
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

pub fn a_project(project_id: &str, repo: &Path) -> ProjectData {
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

/// The session a start the home session began answers to. `started` is the `StartSession` answer's
/// JSON, which carries the start's handle; the start is polled with `GetSessionStart` until it is no
/// longer running, and it must have succeeded (`#start-parity` 4/9).
pub async fn session_started_by(daemon: &TestDaemon, home: &str, started: &str) -> String {
    let handle: serde_json::Value =
        serde_json::from_str(started).expect("the start's handle is JSON");
    let start_id = handle["start_id"].as_str().expect("a start_id").to_string();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let answer = daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: home.to_string(),
                tool_name: "GetSessionStart".to_string(),
                args_json: serde_json::json!({ "start_id": start_id }).to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .expect("GetSessionStart is served to the home session")
            .into_inner();
        assert!(!answer.is_error, "{}", answer.error_message);
        let record: serde_json::Value =
            serde_json::from_str(&answer.result_json).expect("the record is JSON");
        match record["state"].as_str() {
            Some("running") => {}
            Some("succeeded") => {
                return record["session_id"]
                    .as_str()
                    .expect("a succeeded start names its session")
                    .to_string()
            }
            _ => panic!("start {start_id} did not succeed: {record}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "start {start_id} is still running: {record}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub async fn a_workspace_in(daemon: &TestDaemon, project_id: &str) -> String {
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
pub struct Host {
    pub daemon: Arc<TestDaemon>,
    /// The manager the daemon's terminals live in: a test starts a session's child here.
    pub terminals: Arc<CliSessionManager>,
    pub sessions: tempfile::TempDir,
    _repos: (tempfile::TempDir, tempfile::TempDir),
}

impl Host {
    pub fn worktree_of(&self, session_id: &str) -> PathBuf {
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

    pub fn make_project_home(&self, session_id: &str) {
        let dir = tddy_core::session_lifecycle::unified_session_dir_path(
            self.sessions.path(),
            session_id,
        );
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_role = Some("project-home".to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }
}

pub fn a_host(instance_id: &str, peers: &Arc<InProcessPeers>) -> Host {
    host_built_by(instance_id, peers, test_config())
}

/// [`a_host`], whose daemon maps the test user to the OS user this process runs as.
pub fn a_host_as_current_user(instance_id: &str, peers: &Arc<InProcessPeers>) -> Host {
    host_built_by(
        instance_id,
        peers,
        config_mapping_test_user_to(&current_os_user()),
    )
}

fn host_built_by(
    instance_id: &str,
    peers: &Arc<InProcessPeers>,
    config: tddy_daemon_kernel::config::DaemonConfig,
) -> Host {
    host_built_with(instance_id, peers, config, |host, _sessions| host)
}

/// [`a_host`], whose daemon keeps a project notification log on its notification bus
/// (`#home-inbox` 7/8): publish onto the returned bus to record a notification, as the daemon's
/// publish sites do. The log persists under the host's sessions directory.
pub fn a_host_logging_notifications(
    instance_id: &str,
    peers: &Arc<InProcessPeers>,
) -> (Host, Arc<SessionNotificationBus>) {
    let bus = Arc::new(std::sync::Mutex::new(None));
    let published_on = Arc::clone(&bus);
    let host = host_built_with(instance_id, peers, test_config(), move |host, sessions| {
        let base = sessions.to_path_buf();
        let log = Arc::new(
            ProjectNotificationLog::open(
                &sessions.join("notification-log"),
                Arc::new(move |_os_user: &str| Some(base.clone())),
            )
            .expect("the notification log opens"),
        );
        let notifications =
            Arc::new(SessionNotificationBus::new().with_subscriber(Arc::clone(&log)));
        *published_on.lock().unwrap() = Some(Arc::clone(&notifications));
        host.with_session_notification_bus(notifications)
            .with_project_notification_log(log)
    });
    let bus = bus.lock().unwrap().take().expect("the bus was installed");
    (host, bus)
}

/// [`a_host`], whose daemon's notification bus records everything published on it
/// (`#home-inbox` 11/11): what an agent's `NotifyOperator` reached.
pub fn a_host_recording_notifications(
    instance_id: &str,
    peers: &Arc<InProcessPeers>,
) -> (Host, Arc<RecordingSessionNotificationSubscriber>) {
    let recorded = Arc::new(RecordingSessionNotificationSubscriber::new());
    let subscriber = Arc::clone(&recorded);
    let host = host_built_with(instance_id, peers, test_config(), move |host, _sessions| {
        host.with_session_notification_bus(Arc::new(
            SessionNotificationBus::new().with_subscriber(subscriber),
        ))
    });
    (host, recorded)
}

fn host_built_with(
    instance_id: &str,
    peers: &Arc<InProcessPeers>,
    config: tddy_daemon_kernel::config::DaemonConfig,
    customise: impl FnOnce(DaemonSessionHost, &Path) -> DaemonSessionHost,
) -> Host {
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
    let terminals = Arc::new(CliSessionManager::new());
    let sessions_path = sessions.path().to_path_buf();
    let daemon = Arc::new(daemon_over_customised(
        config,
        instance_id,
        sessions.path(),
        Arc::clone(peers),
        Arc::clone(&terminals),
        |host| customise(host, &sessions_path),
    ));
    peers.register(instance_id, Arc::clone(&daemon));
    Host {
        daemon,
        terminals,
        sessions,
        _repos: (this_repo, other_repo),
    }
}
