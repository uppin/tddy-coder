//! Acceptance: a **split** session — the agent runs on daemon A, its git worktree lives on daemon B.
//!
//! PRD: `docs/ft/daemon/remote-managed-worktree.md`
//! Changeset: `docs/dev/1-WIP/2026-08-31-split-sandbox-orchestration.md`
//!
//! `StartSessionRequest.codebase_daemon_instance_id` names the daemon whose filesystem holds the
//! worktree. When it differs from the daemon running the agent, A creates a `workspace` session on B
//! over the existing peer-forward path and spawns the agent locally with **no repository on disk**;
//! the agent reaches the worktree only through `mcp__tddy-tools__*` over LiveKit.
//!
//! `sandbox = true` on a split placement inverts from the co-located meaning: the **codebase** half
//! on B is sandboxed (the workspace tool jail confines the repository-side tool calls the agent
//! proxies to it) and the **agent** half on A stays unsandboxed. The codebase host's jail is
//! injected here as a recording provisioner, so confinement is asserted without a kernel sandbox
//! on the host running the test — what the jail then *confines* is proven against a real one in
//! `tests/workspace_tool_sandbox_seatbelt_acceptance.rs`.
//!
//! Each daemon serves RPC on its production identity `daemon-{instance_id}` — the one a real peer
//! answers on — so a call landing on B's service proves the split actually crossed hosts rather than
//! quietly resolving locally.
//!
//! These need the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`) and are `#[serial]`
//! so they own it alone. Placement *validation* needs none of this and lives in
//! `tests/remote_managed_worktree_acceptance.rs`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use livekit::prelude::RoomOptions;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    spawn_common_room_discovery_task, CommonRoomPeerRegistry, LiveKitDiscoveryHandles,
    LiveKitEligibleDaemonSource,
};
use tddy_daemon::workspace_tool_sandbox::{
    WorkspaceSandbox, WorkspaceSandboxProvisioner, WorkspaceSandboxSpec,
};
use tddy_github::{GitHubUser, SessionTokenSigner, TokenKind};
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_sandbox::SandboxError;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, ExecuteToolRequest,
    ExecuteToolResponse, ListEligibleDaemonsRequest, ListSessionsRequest, StartSessionRequest,
};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually_awaiting;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const ROOM: &str = "split-placement-room";
/// Daemon A — runs the agent.
///
/// Deliberately not `split-agent-…`: that prefix is reserved for a split session's *agent*
/// participant and is refused as a daemon advertisement, so a daemon named that way would never be
/// discovered (`livekit_peer_discovery::eligible_daemon_from_participant_fields`).
const AGENT_INSTANCE_ID: &str = "split-placement-agent-host";
/// Daemon B — holds the codebase.
const CODEBASE_INSTANCE_ID: &str = "split-codebase-host";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const TEST_PROJECT_ID: &str = "split-placement-proj";

/// The credential the browser presents on every call here, signed with [`LK_API_SECRET`] — the
/// secret both daemons hold, and the one a session token is verified against anywhere in a
/// deployment. Minted once and shared, because the requests and the stub user resolver have to
/// agree on the same string; a split placement mints the agent's own credential from these claims,
/// so an unsigned literal would name an identity no daemon could confirm.
fn a_caller_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        SessionTokenSigner::new(LK_API_SECRET.as_bytes()).mint_access(&GitHubUser {
            id: 4242,
            login: "testuser".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
            name: "Test User".to_string(),
        })
    })
}

/// Resolve the OS user the way a real daemon does: by verifying the credential's signature and
/// reading the login out of its claims (`auth::build_auth_entries`), never by recognising one
/// string. A daemon signs credentials of its own — a session room mints one per poll of a checkout
/// it does not hold — so a resolver that only accepted the browser's exact token would authenticate
/// the caller and nothing the deployment itself issued.
fn resolve_user_by_verifying_the_token(token: &str) -> Option<String> {
    SessionTokenSigner::new(LK_API_SECRET.as_bytes())
        .verify(token)
        .ok()
        .filter(|claims| claims.kind == TokenKind::Access)
        .map(|claims| claims.login)
}

/// The identity a daemon actually serves `connection.ConnectionService` on. Fixed `daemon-` prefix,
/// not a lookup — see `docs/ft/web/daemon-selector-livekit-rpc.md`.
fn rpc_identity(instance_id: &str) -> String {
    format!("daemon-{instance_id}")
}

fn true_bin() -> String {
    ["/bin/true", "/usr/bin/true"]
        .iter()
        .find(|p| Path::new(p).exists())
        .expect("a `true` binary must exist for the tool allowlist")
        .to_string()
}

// ---------------------------------------------------------------------------
// Daemon fixtures
// ---------------------------------------------------------------------------

/// A long-lived process standing in for the `claude` CLI: the agent side of a split session is a
/// real PTY spawn, so without a stub every start would fail reaching for `claude` on PATH — for
/// reasons that have nothing to do with placement.
///
/// It reads stdin forever, which is the shape a PTY session needs. Deliberately not `/bin/cat`:
/// `cat` treats a positional argument as a filename and exits, so the moment a fixture grew an
/// `initial_prompt` the stub would die and the failure would look like a spawn bug.
fn a_claude_stub(dir: &Path) -> PathBuf {
    a_stub_agent_script(dir, "stub-claude.sh")
        .then_reading_stdin()
        .build()
}

fn write_livekit_daemon_yaml(
    ws_url: &str,
    daemon_instance_id: &str,
    os_user: &str,
    claude_binary: &str,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let true_path = true_bin();
    let yaml = format!(
        r#"
daemon_instance_id: {daemon_instance_id}
users:
  - github_user: "testuser"
    os_user: "{os_user}"
allowed_tools:
  - path: {true_path}
    label: t
claude_cli:
  binary_path: {claude_binary}
livekit:
  url: {ws_url}
  api_key: {LK_API_KEY}
  api_secret: {LK_API_SECRET}
  common_room: {ROOM}
"#
    );
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: split-proj\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// A repo whose `origin` points at itself, so worktree setup's `git fetch origin` succeeds with no
/// server.
fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

struct Daemon {
    service: ConnectionServiceImpl,
    sessions_base: PathBuf,
    _sessions: tempfile::TempDir,
    _config: tempfile::TempDir,
}

async fn a_daemon(
    ws_url: &str,
    instance_id: &str,
    os_user: &str,
    repo: &Path,
    user_resolver: UserResolver,
    claude_binary: &str,
    codebase_provisioner: Option<Arc<dyn WorkspaceSandboxProvisioner>>,
) -> Daemon {
    let (config_dir, config_path) =
        write_livekit_daemon_yaml(ws_url, instance_id, os_user, claude_binary);
    let config = DaemonConfig::load(&config_path).unwrap();

    let sessions = tempfile::tempdir().unwrap();
    register_project(&sessions.path().join("projects"), repo);
    let base = sessions.path().to_path_buf();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));

    let config_arc = Arc::new(config.clone());
    let registry = Arc::new(CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    spawn_common_room_discovery_task(config_arc.clone(), registry.clone(), room_slot.clone());
    let eligible: Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource> = Arc::new(
        LiveKitEligibleDaemonSource::new(config_arc, registry, room_slot.clone()),
    );

    let service = ConnectionServiceImpl::new(
        config,
        resolver,
        sessions.path().to_path_buf(),
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: eligible,
            common_room_livekit_room: room_slot,
        }),
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    );
    let service = match codebase_provisioner {
        Some(provisioner) => service.with_workspace_sandbox_provisioner(provisioner),
        None => service,
    };

    Daemon {
        service,
        sessions_base: sessions.path().to_path_buf(),
        _sessions: sessions,
        _config: config_dir,
    }
}

async fn serve_rpc_participant(
    livekit: &LiveKitTestkit,
    ws_url: &str,
    instance_id: &str,
    service: ConnectionServiceImpl,
) -> tokio::task::JoinHandle<()> {
    let token = livekit
        .generate_token(ROOM, &rpc_identity(instance_id))
        .expect("LiveKit token for a daemon's RPC participant");
    let server = tddy_service::ConnectionServiceServer::new(service);
    let participant =
        LiveKitParticipant::connect(ws_url, &token, server, RoomOptions::default(), None, None)
            .await
            .expect("daemon joins the common room as its RPC participant");
    tokio::spawn(async move { participant.run().await })
}

/// 45s: both daemons publish their advertisement on the common room's own metadata cadence, and a
/// cold LiveKit container has to accept every participant first.
///
/// `eventually_awaiting` rather than a hand-rolled poll: when the peer never shows up it panics
/// with the list that *was* returned, which is the difference between "timed out" and "these three
/// daemons were visible and yours was not".
async fn wait_until_discovered(service: &ConnectionServiceImpl, peer_instance_id: &str) {
    eventually_awaiting(
        &format!("daemon {peer_instance_id} to be discovered in the common room"),
        Duration::from_secs(45),
        || async {
            let daemons = service
                .list_eligible_daemons(Request::new(ListEligibleDaemonsRequest {
                    session_token: a_caller_token().to_string(),
                }))
                .await
                .map_err(|e| format!("ListEligibleDaemons failed: {e}"))?
                .into_inner()
                .daemons;
            if daemons.iter().any(|d| d.instance_id == peer_instance_id) {
                return Ok(());
            }
            Err(format!(
                "eligible daemons so far: {:?}",
                daemons.iter().map(|d| &d.instance_id).collect::<Vec<_>>()
            ))
        },
    )
    .await;
}

/// Daemon A (agent) and daemon B (codebase), each serving on its production RPC identity and each
/// able to route to the other.
struct SplitHosts {
    agent: ConnectionServiceImpl,
    codebase: ConnectionServiceImpl,
    agent_sessions_base: PathBuf,
    codebase_sessions_base: PathBuf,
    _agent_rpc_run: tokio::task::JoinHandle<()>,
    _codebase_rpc_run: tokio::task::JoinHandle<()>,
    _livekit: LiveKitTestkit,
    _repo: tempfile::TempDir,
    _stubs: tempfile::TempDir,
    _agent: Daemon,
    _codebase: Daemon,
}

/// Both hosts get a working `claude` stub — the ordinary case, where a split session comes up.
async fn split_hosts() -> SplitHosts {
    split_hosts_with_agent_claude_binary(None).await
}

/// `agent_claude_binary` overrides daemon A's stub only. Pointing it at something unspawnable is
/// the one way to fail *after* the codebase host has already created its workspace session, which
/// is the only window in which teardown is observable. `None` gives A the working stub.
async fn split_hosts_with_agent_claude_binary(agent_claude_binary: Option<&str>) -> SplitHosts {
    split_hosts_with_codebase_provisioner_and_agent_binary(None, agent_claude_binary).await
}

/// A split pair whose codebase daemon B builds its workspace jail through `provisioner` rather
/// than the real kernel sandbox, so a test can assert which calls reached the jail and that the
/// host worktree was never touched — the same double `workspace_tool_sandbox_acceptance.rs` uses
/// for the co-located contract. `agent_claude_binary` is passed through unchanged.
async fn split_hosts_with_codebase_provisioner_and_agent_binary(
    provisioner: Option<Arc<dyn WorkspaceSandboxProvisioner>>,
    agent_claude_binary: Option<&str>,
) -> SplitHosts {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = livekit.get_ws_url();
    let os_user = std::env::var("USER").expect("USER required");

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let stub_dir = tempfile::tempdir().unwrap();
    let claude_stub = a_claude_stub(stub_dir.path());
    let claude_stub = claude_stub.to_str().expect("stub path is valid UTF-8");

    let user_resolver: UserResolver = Arc::new(resolve_user_by_verifying_the_token);

    let codebase = a_daemon(
        &ws_url,
        CODEBASE_INSTANCE_ID,
        &os_user,
        repo_dir.path(),
        user_resolver.clone(),
        claude_stub,
        provisioner,
    )
    .await;
    let agent = a_daemon(
        &ws_url,
        AGENT_INSTANCE_ID,
        &os_user,
        repo_dir.path(),
        user_resolver,
        agent_claude_binary.unwrap_or(claude_stub),
        None,
    )
    .await;

    let codebase_rpc_run = serve_rpc_participant(
        &livekit,
        &ws_url,
        CODEBASE_INSTANCE_ID,
        codebase.service.clone(),
    )
    .await;
    let agent_rpc_run =
        serve_rpc_participant(&livekit, &ws_url, AGENT_INSTANCE_ID, agent.service.clone()).await;

    wait_until_discovered(&agent.service, CODEBASE_INSTANCE_ID).await;
    wait_until_discovered(&codebase.service, AGENT_INSTANCE_ID).await;

    SplitHosts {
        agent: agent.service.clone(),
        codebase: codebase.service.clone(),
        agent_sessions_base: agent.sessions_base.clone(),
        codebase_sessions_base: codebase.sessions_base.clone(),
        _agent_rpc_run: agent_rpc_run,
        _codebase_rpc_run: codebase_rpc_run,
        _livekit: livekit,
        _repo: repo_dir,
        _stubs: stub_dir,
        _agent: agent,
        _codebase: codebase,
    }
}

/// A managed claude-cli session whose codebase is placed on daemon B.
fn a_split_session_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: a_caller_token().to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-5".to_string(),
        managed_codebase: true,
        codebase_daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        ..Default::default()
    }
}

/// A split placement that also asks to be sandboxed. On a split placement the sandbox confines the
/// codebase half on B (the host holding the checkout), not the agent half on A.
fn a_sandboxed_split_session_request() -> StartSessionRequest {
    StartSessionRequest {
        sandbox: true,
        ..a_split_session_request()
    }
}

fn session_metadata_on(sessions_base: &Path, session_id: &str) -> tddy_core::SessionMetadata {
    let dir = unified_session_dir_path(sessions_base, session_id);
    tddy_core::read_session_metadata(&dir)
        .unwrap_or_else(|e| panic!("session metadata for {session_id} must be readable: {e}"))
}

/// Every session directory physically present under a daemon's sessions base.
///
/// Deliberately not `ListSessions`: a start that failed part-way leaves a directory with no
/// readable `.session.yaml`, which the listing skips — and that leftover is exactly what an orphan
/// looks like from the outside.
fn session_directories_on(sessions_base: &Path) -> Vec<String> {
    let dir = sessions_base.join("sessions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

async fn sessions_on(service: &ConnectionServiceImpl) -> Vec<String> {
    service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: a_caller_token().to_string(),
        }))
        .await
        .expect("ListSessions")
        .into_inner()
        .sessions
        .into_iter()
        .map(|s| s.session_id)
        .collect()
}

// ---------------------------------------------------------------------------
// A recording workspace sandbox for the codebase host
// ---------------------------------------------------------------------------
//
// The codebase half of a sandboxed split session is confined by the workspace tool jail (#427).
// Proving confinement needs no kernel sandbox on the host running the test: a recording provisioner
// hands out a jail that answers every call with a marker and touches no filesystem, so "the call
// reached the jail" and "the host worktree was never touched" are both assertable — the same double
// `workspace_tool_sandbox_acceptance.rs` uses for the co-located contract.

/// The marker a jailed tool result carries. Its presence in a response proves the call went to the
/// jail on B; its absence proves it ran on the host worktree instead.
const SPLIT_JAIL_MARKER: &str = "ran-inside-the-workspace-jail";

#[derive(Debug, Clone, PartialEq, Eq)]
struct JailedCall {
    session_id: String,
    tool_name: String,
    args_json: String,
}

/// A jail that records what it was asked to run and answers with [`SPLIT_JAIL_MARKER`], touching no
/// filesystem — so a host worktree left unchanged is proof the call never reached the host tool
/// engine on B.
#[derive(Default)]
struct RecordingSandbox {
    calls: std::sync::Mutex<Vec<JailedCall>>,
}

impl RecordingSandbox {
    fn calls(&self) -> Vec<JailedCall> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl WorkspaceSandbox for RecordingSandbox {
    async fn execute_tool(&self, req: &ExecuteToolRequest) -> ExecuteToolResponse {
        self.calls.lock().unwrap().push(JailedCall {
            session_id: req.session_id.clone(),
            tool_name: req.tool_name.clone(),
            args_json: req.args_json.clone(),
        });
        ExecuteToolResponse {
            result_json: serde_json::json!({ "marker": SPLIT_JAIL_MARKER, "tool": req.tool_name })
                .to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }

    fn stop(&self) {}
}

/// A provisioner that hands out one [`RecordingSandbox`], so a test can assert both that the jail
/// was built for the right session and which calls it served.
#[derive(Default)]
struct RecordingProvisioner {
    sandbox: Arc<RecordingSandbox>,
}

impl RecordingProvisioner {
    fn sandbox(&self) -> Arc<RecordingSandbox> {
        Arc::clone(&self.sandbox)
    }
}

#[async_trait]
impl WorkspaceSandboxProvisioner for RecordingProvisioner {
    async fn provision(
        &self,
        _spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError> {
        Ok(Arc::clone(&self.sandbox) as Arc<dyn WorkspaceSandbox>)
    }
}

/// The `marker` a jailed result carries, or `None` when the result did not come from the jail.
fn jail_marker_of(response: &ExecuteToolResponse) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&response.result_json)
        .ok()?
        .get("marker")?
        .as_str()
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_split_session_creates_a_workspace_session_on_the_codebase_daemon() {
    // Given two daemons that can see each other
    let hosts = split_hosts().await;

    // When the agent host starts a session whose codebase is placed on the other host
    let started = hosts
        .agent
        .start_session(Request::new(a_split_session_request()))
        .await
        .expect("a split session must start")
        .into_inner();

    // Then the codebase host owns a workspace session holding a real worktree
    let metadata = session_metadata_on(&hosts.agent_sessions_base, &started.session_id);
    let codebase_session_id = metadata
        .codebase_session_id
        .expect("a split session must record the workspace session holding its codebase");

    let codebase_metadata =
        session_metadata_on(&hosts.codebase_sessions_base, &codebase_session_id);
    assert_eq!(
        codebase_metadata.session_type.as_deref(),
        Some("workspace"),
        "the codebase host's session must be a workspace session"
    );
    let worktree = PathBuf::from(
        codebase_metadata
            .repo_path
            .expect("the workspace session must record its worktree"),
    );
    assert!(
        worktree.exists(),
        "the worktree must exist on the codebase host at {worktree:?}"
    );
}

#[tokio::test]
#[serial]
async fn a_split_session_records_its_pairing_and_holds_no_local_repo_path() {
    // Given
    let hosts = split_hosts().await;

    // When
    let started = hosts
        .agent
        .start_session(Request::new(a_split_session_request()))
        .await
        .expect("a split session must start")
        .into_inner();

    // Then — the pairing is persisted, because "which host is this session on" has two answers and
    // neither can be recovered from which daemon happened to answer ListSessions
    let metadata = session_metadata_on(&hosts.agent_sessions_base, &started.session_id);
    assert_eq!(
        metadata.codebase_daemon_instance_id.as_deref(),
        Some(CODEBASE_INSTANCE_ID),
        "the agent host must record which daemon holds the codebase"
    );
    assert_eq!(
        metadata.repo_path, None,
        "a split session has no repository on the agent host; repo_path was {:?}",
        metadata.repo_path
    );
}

#[tokio::test]
#[serial]
async fn a_split_session_addresses_tools_at_the_codebase_session_not_its_own() {
    // Given a split session
    let hosts = split_hosts().await;
    let started = hosts
        .agent
        .start_session(Request::new(a_split_session_request()))
        .await
        .expect("a split session must start")
        .into_inner();
    let metadata = session_metadata_on(&hosts.agent_sessions_base, &started.session_id);
    let codebase_session_id = metadata
        .codebase_session_id
        .expect("a split session must record its codebase session");

    // When a tool is executed against the recorded codebase session on the codebase host
    let response = hosts
        .agent
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: a_caller_token().to_string(),
            session_id: codebase_session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: serde_json::json!({ "path": "split.txt", "contents": "from host a" })
                .to_string(),
            daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        }))
        .await
        .expect("a tool call routed to the codebase host must succeed")
        .into_inner();

    // Then — it landed in the worktree on the codebase host, not anywhere on the agent host. The
    // agent's own session id would resolve to nothing on B, which is exactly the mistake this pins.
    assert!(
        !response.is_error,
        "the tool call must succeed; error was '{}'",
        response.error_message
    );
    let codebase_metadata =
        session_metadata_on(&hosts.codebase_sessions_base, &codebase_session_id);
    let written = PathBuf::from(codebase_metadata.repo_path.expect("worktree")).join("split.txt");
    assert_eq!(
        std::fs::read_to_string(&written).expect("the tool must have written into B's worktree"),
        "from host a"
    );
}

#[tokio::test]
#[serial]
async fn a_tool_call_from_the_agent_host_reads_back_what_it_wrote_on_the_codebase_host() {
    // Given a split session with a file written through the split path
    let hosts = split_hosts().await;
    let started = hosts
        .agent
        .start_session(Request::new(a_split_session_request()))
        .await
        .expect("a split session must start")
        .into_inner();
    let codebase_session_id = session_metadata_on(&hosts.agent_sessions_base, &started.session_id)
        .codebase_session_id
        .expect("codebase session id");

    hosts
        .agent
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: a_caller_token().to_string(),
            session_id: codebase_session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: serde_json::json!({ "path": "round-trip.txt", "contents": "hello b" })
                .to_string(),
            daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        }))
        .await
        .expect("Write must succeed");

    // When it is read back over the same path
    let response = hosts
        .agent
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: a_caller_token().to_string(),
            session_id: codebase_session_id,
            tool_name: "Read".to_string(),
            args_json: serde_json::json!({ "path": "round-trip.txt" }).to_string(),
            daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        }))
        .await
        .expect("Read must succeed")
        .into_inner();

    // Then — a full write/read round trip crossed the hosts
    let result: serde_json::Value =
        serde_json::from_str(&response.result_json).expect("result_json must be JSON");
    assert_eq!(result["content"], "hello b");
}

#[tokio::test]
#[serial]
async fn deleting_a_split_session_deletes_the_paired_workspace_session_and_its_worktree() {
    // Given a split session
    let hosts = split_hosts().await;
    let started = hosts
        .agent
        .start_session(Request::new(a_split_session_request()))
        .await
        .expect("a split session must start")
        .into_inner();
    let codebase_session_id = session_metadata_on(&hosts.agent_sessions_base, &started.session_id)
        .codebase_session_id
        .expect("codebase session id");
    let worktree = PathBuf::from(
        session_metadata_on(&hosts.codebase_sessions_base, &codebase_session_id)
            .repo_path
            .expect("worktree"),
    );

    // When the agent-host session is deleted
    hosts
        .agent
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: a_caller_token().to_string(),
            session_id: started.session_id.clone(),
        }))
        .await
        .expect("deleting a split session must succeed");

    // Then the codebase host keeps neither the session nor its worktree — otherwise every split
    // session leaks a checkout on a host the operator may never look at
    let remaining = sessions_on(&hosts.codebase).await;
    assert!(
        !remaining.contains(&codebase_session_id),
        "the paired workspace session must be gone from the codebase host; still listed: {remaining:?}"
    );
    assert!(
        !worktree.exists(),
        "the worktree must be removed from the codebase host; {worktree:?} still exists"
    );
}

#[tokio::test]
#[serial]
async fn deleting_a_split_session_succeeds_when_the_codebase_daemon_no_longer_has_its_workspace_session(
) {
    // Given a split session whose paired workspace session has already been deleted on the codebase
    // host — what an operator deleting it directly there leaves behind, and equally the state after
    // a DeleteSession that succeeded on the peer and then failed locally
    let hosts = split_hosts().await;
    let started = hosts
        .agent
        .start_session(Request::new(a_split_session_request()))
        .await
        .expect("a split session must start")
        .into_inner();
    let codebase_session_id = session_metadata_on(&hosts.agent_sessions_base, &started.session_id)
        .codebase_session_id
        .expect("codebase session id");
    hosts
        .codebase
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: a_caller_token().to_string(),
            session_id: codebase_session_id.clone(),
        }))
        .await
        .expect("deleting the workspace session directly on the codebase host must succeed");

    // When the agent-host session is deleted
    hosts
        .agent
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: a_caller_token().to_string(),
            session_id: started.session_id.clone(),
        }))
        .await
        .expect("a split session whose codebase half is already gone must still be deletable");

    // Then the session is actually gone from the agent host — a peer answering "I do not have it"
    // is the state the deletion wanted, so treating it as a failure would make the session
    // permanently undeletable through the API
    assert!(
        !unified_session_dir_path(&hosts.agent_sessions_base, &started.session_id).exists(),
        "the agent-host session directory must be removed"
    );
}

#[tokio::test]
#[serial]
async fn a_worktree_failure_on_the_codebase_daemon_leaves_no_session_behind() {
    // Given a split request naming a branch that does not exist on the codebase host, so the peer
    // creates the session directory and then fails cutting the worktree — the shape of every
    // forwarded start that errors *after* the peer has begun building
    let hosts = split_hosts().await;
    let before = session_directories_on(&hosts.codebase_sessions_base);

    // When
    hosts
        .agent
        .start_session(Request::new(StartSessionRequest {
            branch_worktree_intent: "work_on_selected_branch".to_string(),
            selected_branch_to_work_on: "no-such-branch-anywhere".to_string(),
            ..a_split_session_request()
        }))
        .await
        .expect_err("a split start whose worktree cannot be cut must fail");

    // Then the codebase host is left exactly as it was. The agent host never saw a session id in the
    // answer, so it can only tear down a session it named itself — which is why it chooses that id
    // before forwarding.
    let after = session_directories_on(&hosts.codebase_sessions_base);
    assert_eq!(
        after, before,
        "a failed forward must leave no session directory on the codebase host"
    );
}

#[tokio::test]
#[serial]
async fn a_failed_agent_spawn_tears_down_the_workspace_session_on_the_codebase_daemon() {
    // Given an agent host whose claude binary does not exist, so the request is well-formed and
    // only fails once the spawn is attempted — after the codebase host has done its work
    let hosts =
        split_hosts_with_agent_claude_binary(Some("/nonexistent/claude-that-cannot-be-spawned"))
            .await;
    let before = sessions_on(&hosts.codebase).await;

    // When
    hosts
        .agent
        .start_session(Request::new(a_split_session_request()))
        .await
        .expect_err("a split start whose agent cannot spawn must fail");

    // Then the codebase host is left exactly as it was — a half-built split session would strand a
    // worktree on a host the operator may never look at, with no session left to reclaim it
    let after = sessions_on(&hosts.codebase).await;
    assert_eq!(
        after, before,
        "a failed split start must leave no workspace session behind on the codebase host"
    );
}

// ---------------------------------------------------------------------------
// Sandbox on the codebase half of a split placement
// ---------------------------------------------------------------------------
//
// `sandbox = true` on a split placement inverts from the co-located meaning: the codebase half on
// B is sandboxed (the workspace tool jail confines the repository-side tool calls the agent
// proxies to it) and the agent half on A stays unsandboxed. The refusal that used to block this
// combination is removed in `start_split_claude_cli_session`; the forward path already carries the
// flag through `workspace_start_request`'s `..req.clone()`, and the codebase host's workspace
// start already persists `sandbox: Some(true)` and provisions the jail (#427).

/// A split start with `sandbox = true` succeeds and splits the flag the way the placement demands:
/// the agent half on A is unsandboxed (`sandbox: None` metadata, no jail on A) and the codebase
/// half on B is sandboxed (`sandbox: Some(true)` metadata, a jail provisioned on B). The
/// recording provisioner stands in for B's kernel sandbox, so the assertion is about which half
/// the flag landed on rather than about confinement itself.
#[tokio::test]
#[serial]
async fn a_split_session_started_with_sandbox_sandboxes_the_codebase_half_and_leaves_the_agent_half_unsandboxed() {
    // Given a codebase host whose jail is a recording provisioner, so the sandboxed workspace
    // start on B can complete without a kernel sandbox on the host running the test
    let provisioner = Arc::new(RecordingProvisioner::default());
    let hosts = split_hosts_with_codebase_provisioner_and_agent_binary(
        Some(Arc::clone(&provisioner) as Arc<dyn WorkspaceSandboxProvisioner>),
        None,
    )
    .await;

    // When the agent host starts a split session that also asks to be sandboxed
    let started = hosts
        .agent
        .start_session(Request::new(a_sandboxed_split_session_request()))
        .await
        .expect("a sandboxed split start must succeed once the refusal is gone")
        .into_inner();

    // Then — the agent half on A carries no sandbox: it runs on the operator's host with managed
    // MCP tools, and keeping it unsandboxed preserves the existing spawn_split_agent / resume path.
    let agent_metadata = session_metadata_on(&hosts.agent_sessions_base, &started.session_id);
    assert_eq!(
        agent_metadata.sandbox,
        None,
        "the agent half of a sandboxed split session must stay unsandboxed"
    );

    // And the codebase half on B is sandboxed: the flag forwarded through `workspace_start_request`
    // is what every later tool dispatch on B reads to route through the jail.
    let codebase_session_id = agent_metadata
        .codebase_session_id
        .expect("a split session must record its codebase session");
    let codebase_metadata =
        session_metadata_on(&hosts.codebase_sessions_base, &codebase_session_id);
    assert_eq!(
        codebase_metadata.session_type.as_deref(),
        Some("workspace"),
        "the codebase half must be a workspace session"
    );
    assert_eq!(
        codebase_metadata.sandbox,
        Some(true),
        "the codebase half of a sandboxed split session must be sandboxed"
    );
}

/// A tool call the agent proxies at the codebase session on B reaches the jail on B, not the host
/// worktree on B. This is the confinement the split+sandbox combination exists to enforce: the
/// repository-side `Shell`/`Write` work the agent drives runs inside the kernel sandbox on the host
/// holding the checkout, and the host worktree on B is left untouched.
#[tokio::test]
#[serial]
async fn a_tool_call_on_a_sandboxed_split_session_runs_in_the_jail_on_the_codebase_host() {
    // Given a sandboxed split session whose codebase half on B is confined by a recording jail
    let provisioner = Arc::new(RecordingProvisioner::default());
    let jail = provisioner.sandbox();
    let hosts = split_hosts_with_codebase_provisioner_and_agent_binary(
        Some(Arc::clone(&provisioner) as Arc<dyn WorkspaceSandboxProvisioner>),
        None,
    )
    .await;
    let started = hosts
        .agent
        .start_session(Request::new(a_sandboxed_split_session_request()))
        .await
        .expect("a sandboxed split start must succeed")
        .into_inner();
    let codebase_session_id = session_metadata_on(&hosts.agent_sessions_base, &started.session_id)
        .codebase_session_id
        .expect("codebase session id");
    let codebase_worktree = PathBuf::from(
        session_metadata_on(&hosts.codebase_sessions_base, &codebase_session_id)
            .repo_path
            .expect("the workspace session must record its worktree"),
    );

    // When a tool is executed against the recorded codebase session on the codebase host
    let response = hosts
        .agent
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: a_caller_token().to_string(),
            session_id: codebase_session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: serde_json::json!({ "path": "from-the-jail.txt", "contents": "confined" })
                .to_string(),
            daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        }))
        .await
        .expect("a tool call routed to the codebase host must succeed")
        .into_inner();

    // Then — it ran in the jail on B, not on B's host worktree. The marker proves the call reached
    // the jail; the unchanged worktree proves the host tool engine on B never ran it.
    assert_eq!(jail_marker_of(&response).as_deref(), Some(SPLIT_JAIL_MARKER));
    assert_eq!(
        jail.calls(),
        vec![JailedCall {
            session_id: codebase_session_id,
            tool_name: "Write".to_string(),
            args_json: r#"{"path":"from-the-jail.txt","contents":"confined"}"#.to_string(),
        }],
        "exactly one call must have reached the jail on the codebase host"
    );
    assert!(
        !codebase_worktree.join("from-the-jail.txt").exists(),
        "the host tool engine on B must not have run: the jail owns this write"
    );
}
