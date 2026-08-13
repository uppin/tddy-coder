//! Acceptance: a **split** session — the agent runs on daemon A, its git worktree lives on daemon B.
//!
//! PRD: `docs/ft/daemon/remote-managed-worktree.md`
//! Changeset: `docs/dev/1-WIP/remote-managed-worktree.md`
//!
//! `StartSessionRequest.codebase_daemon_instance_id` names the daemon whose filesystem holds the
//! worktree. When it differs from the daemon running the agent, A creates a `workspace` session on B
//! over the existing peer-forward path and spawns the agent locally with **no repository on disk**;
//! the agent reaches the worktree only through `mcp__tddy-tools__*` over LiveKit.
//!
//! Each daemon serves RPC on its production identity `daemon-{instance_id}` — the one a real peer
//! answers on — so a call landing on B's service proves the split actually crossed hosts rather than
//! quietly resolving locally.
//!
//! These need the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`) and are `#[serial]`
//! so they own it alone. Placement *validation* needs none of this and lives in
//! `tests/remote_managed_worktree_acceptance.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    spawn_common_room_discovery_task, CommonRoomPeerRegistry, LiveKitDiscoveryHandles,
    LiveKitEligibleDaemonSource,
};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, ExecuteToolRequest,
    ListEligibleDaemonsRequest, ListSessionsRequest, StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const ROOM: &str = "split-placement-room";
/// Daemon A — runs the agent.
const AGENT_INSTANCE_ID: &str = "split-agent-host";
/// Daemon B — holds the codebase.
const CODEBASE_INSTANCE_ID: &str = "split-codebase-host";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const TEST_PROJECT_ID: &str = "split-placement-proj";

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
/// reasons that have nothing to do with placement. `/bin/cat` blocks on stdin, which is exactly the
/// shape a PTY session needs. Mirrors `claude_cli_session_acceptance.rs`.
const CLAUDE_STUB: &str = "/bin/cat";

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
async fn wait_until_discovered(service: &ConnectionServiceImpl, peer_instance_id: &str) {
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let daemons = service
                .list_eligible_daemons(Request::new(ListEligibleDaemonsRequest {
                    session_token: TEST_TOKEN.to_string(),
                }))
                .await
                .expect("ListEligibleDaemons")
                .into_inner()
                .daemons;
            if daemons.iter().any(|d| d.instance_id == peer_instance_id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timeout waiting for daemon {peer_instance_id} to be discovered"));
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
    _agent: Daemon,
    _codebase: Daemon,
}

async fn split_hosts() -> SplitHosts {
    split_hosts_with_agent_claude_binary(CLAUDE_STUB).await
}

/// `agent_claude_binary` belongs to daemon A only. Pointing it at something unspawnable is the one
/// way to fail *after* the codebase host has already created its workspace session, which is the
/// only window in which teardown is observable.
async fn split_hosts_with_agent_claude_binary(agent_claude_binary: &str) -> SplitHosts {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = livekit.get_ws_url();
    let os_user = std::env::var("USER").expect("USER required");

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));

    let codebase = a_daemon(
        &ws_url,
        CODEBASE_INSTANCE_ID,
        &os_user,
        repo_dir.path(),
        user_resolver.clone(),
        CLAUDE_STUB,
    )
    .await;
    let agent = a_daemon(
        &ws_url,
        AGENT_INSTANCE_ID,
        &os_user,
        repo_dir.path(),
        user_resolver,
        agent_claude_binary,
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
        _agent: agent,
        _codebase: codebase,
    }
}

/// A managed claude-cli session whose codebase is placed on daemon B.
fn a_split_session_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-5".to_string(),
        managed_codebase: true,
        codebase_daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        ..Default::default()
    }
}

fn session_metadata_on(sessions_base: &Path, session_id: &str) -> tddy_core::SessionMetadata {
    let dir = unified_session_dir_path(sessions_base, session_id);
    tddy_core::read_session_metadata(&dir)
        .unwrap_or_else(|e| panic!("session metadata for {session_id} must be readable: {e}"))
}

async fn sessions_on(service: &ConnectionServiceImpl) -> Vec<String> {
    service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
            ..Default::default()
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
            session_token: TEST_TOKEN.to_string(),
            session_id: codebase_session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: serde_json::json!({ "path": "split.txt", "content": "from host a" })
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
            session_token: TEST_TOKEN.to_string(),
            session_id: codebase_session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: serde_json::json!({ "path": "round-trip.txt", "content": "hello b" })
                .to_string(),
            daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        }))
        .await
        .expect("Write must succeed");

    // When it is read back over the same path
    let response = hosts
        .agent
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
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
            session_token: TEST_TOKEN.to_string(),
            session_id: started.session_id.clone(),
            ..Default::default()
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
async fn a_failed_agent_spawn_tears_down_the_workspace_session_on_the_codebase_daemon() {
    // Given an agent host whose claude binary does not exist, so the request is well-formed and
    // only fails once the spawn is attempted — after the codebase host has done its work
    let hosts =
        split_hosts_with_agent_claude_binary("/nonexistent/claude-that-cannot-be-spawned").await;
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
