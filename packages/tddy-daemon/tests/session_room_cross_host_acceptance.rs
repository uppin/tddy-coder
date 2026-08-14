//! Acceptance: a **split** session's room lives with its agent, and hides where the files are.
//!
//! Product contract: `docs/ft/daemon/session-room.md`.
//!
//! The agent runs on the facilitating daemon; the checkout lives on the codebase daemon. The room is
//! the facilitating daemon's — it is named after the session that daemon owns, it is hosted by that
//! daemon, and every participant addresses that daemon's identity inside it. Both of the things a
//! participant does there therefore have to work across the split without looking any different:
//! reading a file (forwarded to the codebase daemon) and hearing that the checkout moved (measured by
//! asking the codebase daemon).
//!
//! These need the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`) and are `#[serial]`
//! so they own it alone.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use livekit::{Room, RoomEvent};
use prost::Message;
use serial_test::serial;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    spawn_common_room_discovery_task, CommonRoomPeerRegistry, LiveKitDiscoveryHandles,
    LiveKitEligibleDaemonSource,
};
use tddy_daemon::session_room::{session_room_name, WORKTREE_ACTIVITY_TOPIC};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_livekit::{LiveKitParticipant, LiveKitRpcClientFactory, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ExecuteToolRequest, ExecuteToolResponse,
    ListEligibleDaemonsRequest, StartSessionRequest,
};
use tddy_service::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually_awaiting;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const COMMON_ROOM: &str = "session-room-split-lobby";
/// The daemon that runs the agent — this session's facilitating daemon, and its room's host.
const AGENT_INSTANCE_ID: &str = "session-room-agent-host";
/// The daemon that holds the checkout. It hosts no room.
const CODEBASE_INSTANCE_ID: &str = "session-room-codebase-host";
const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const TEST_PROJECT_ID: &str = "session-room-split-proj";
const POLL_INTERVAL_MS: u64 = 200;

/// Committed before any worktree is cut, so a checkout has a tracked file from the moment its room
/// opens and no test needs a setup commit of its own.
const SEEDED_FILE: &str = "seeded.txt";
const SEEDED_CONTENTS: &str = "one\ntwo\n";

/// A cold container, two daemons discovering each other, and a measurement that crosses the wire
/// once per poll. Ceilings, not expected durations.
const ACTIVITY_TIMEOUT: Duration = Duration::from_secs(45);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(45);

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
// Fixtures
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .expect("git must be on PATH");
    assert!(
        output.status.success(),
        "git {args:?} failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The sha `HEAD` resolves to in `dir`, checked rather than trusted — a snapshot whose git could not
/// run reports an empty `head_commit`, so an unchecked helper would compare one failure to another.
fn head_commit_of(dir: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git must be on PATH");
    assert!(
        output.status.success(),
        "git rev-parse HEAD failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // Minted by git at commit time — only its shape can be pinned here.
    assert!(
        sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "git rev-parse HEAD in {dir:?} answered {sha:?}, which is not a commit sha"
    );
    sha
}

fn create_test_repo_with_origin(dir: &Path) {
    git(dir, &["init", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(SEEDED_FILE), SEEDED_CONTENTS).expect("seed the repository");
    git(dir, &["add", SEEDED_FILE]);
    git(dir, &["commit", "-m", "seed"]);
    git(dir, &["remote", "add", "origin", dir.to_str().unwrap()]);
    git(dir, &["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: split-proj\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn write_daemon_yaml(
    ws_url: &str,
    instance_id: &str,
    os_user: &str,
    claude_binary: &str,
) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    let true_path = true_bin();
    let yaml = format!(
        r#"
daemon_instance_id: {instance_id}
users:
  - github_user: "testuser"
    os_user: "{os_user}"
allowed_tools:
  - path: {true_path}
    label: t
claude_cli:
  binary_path: {claude_binary}
session_room:
  poll_interval_ms: {POLL_INTERVAL_MS}
livekit:
  url: {ws_url}
  api_key: {LK_API_KEY}
  api_secret: {LK_API_SECRET}
  common_room: {COMMON_ROOM}
"#
    );
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
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
    claude_binary: &str,
) -> Daemon {
    let (config_dir, config_path) = write_daemon_yaml(ws_url, instance_id, os_user, claude_binary);
    let config = DaemonConfig::load(&config_path).expect("daemon.yaml must load");

    let sessions = tempfile::tempdir().unwrap();
    register_project(&sessions.path().join("projects"), repo);
    let base = sessions.path().to_path_buf();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == TEST_TOKEN).then(|| "testuser".to_string()));

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
        .generate_token(COMMON_ROOM, &rpc_identity(instance_id))
        .expect("LiveKit token for a daemon's RPC participant");
    let server = tddy_service::ConnectionServiceServer::new(service);
    let participant =
        LiveKitParticipant::connect(ws_url, &token, server, RoomOptions::default(), None, None)
            .await
            .expect("daemon joins the common room as its RPC participant");
    tokio::spawn(async move { participant.run().await })
}

async fn wait_until_discovered(service: &ConnectionServiceImpl, peer_instance_id: &str) {
    eventually_awaiting(
        &format!("daemon {peer_instance_id} to be discovered in the common room"),
        DISCOVERY_TIMEOUT,
        || async {
            let daemons = service
                .list_eligible_daemons(Request::new(ListEligibleDaemonsRequest {
                    session_token: TEST_TOKEN.to_string(),
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

/// Both daemons, discovered, plus the split session they were built for.
struct SplitSession {
    /// The session on the facilitating daemon — the one the room is named after.
    session_id: String,
    /// The paired `workspace` session on the codebase daemon, whose worktree holds the files.
    codebase_session_id: String,
    /// The checkout, on the codebase daemon's filesystem.
    worktree: PathBuf,
    ws_url: String,
    livekit: LiveKitTestkit,
    _agent: Daemon,
    _codebase: Daemon,
    _agent_rpc: tokio::task::JoinHandle<()>,
    _codebase_rpc: tokio::task::JoinHandle<()>,
    _repo: tempfile::TempDir,
    _stubs: tempfile::TempDir,
}

impl SplitSession {
    async fn start() -> Self {
        let livekit = LiveKitTestkit::start()
            .await
            .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
        let ws_url = livekit.get_ws_url();
        let os_user = std::env::var("USER").expect("USER required");

        let repo_dir = tempfile::tempdir().unwrap();
        create_test_repo_with_origin(repo_dir.path());

        let stub_dir = tempfile::tempdir().unwrap();
        let claude_stub = a_stub_agent_script(stub_dir.path(), "stub-claude.sh")
            .then_reading_stdin()
            .build();
        let claude_stub = claude_stub.to_str().expect("stub path is valid UTF-8");

        let codebase = a_daemon(
            &ws_url,
            CODEBASE_INSTANCE_ID,
            &os_user,
            repo_dir.path(),
            claude_stub,
        )
        .await;
        let agent = a_daemon(
            &ws_url,
            AGENT_INSTANCE_ID,
            &os_user,
            repo_dir.path(),
            claude_stub,
        )
        .await;

        let codebase_rpc = serve_rpc_participant(
            &livekit,
            &ws_url,
            CODEBASE_INSTANCE_ID,
            codebase.service.clone(),
        )
        .await;
        let agent_rpc =
            serve_rpc_participant(&livekit, &ws_url, AGENT_INSTANCE_ID, agent.service.clone())
                .await;

        wait_until_discovered(&agent.service, CODEBASE_INSTANCE_ID).await;
        wait_until_discovered(&codebase.service, AGENT_INSTANCE_ID).await;

        let started = agent
            .service
            .start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                session_type: "claude-cli".to_string(),
                model: "claude-opus-5".to_string(),
                managed_codebase: true,
                codebase_daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
                ..Default::default()
            }))
            .await
            .expect("a split session must start")
            .into_inner();

        let agent_meta = tddy_core::read_session_metadata(&unified_session_dir_path(
            &agent.sessions_base,
            &started.session_id,
        ))
        .expect("the agent session's metadata must be readable");
        let codebase_session_id = agent_meta
            .codebase_session_id
            .expect("a split session records the workspace session holding its codebase");
        let worktree = PathBuf::from(
            tddy_core::read_session_metadata(&unified_session_dir_path(
                &codebase.sessions_base,
                &codebase_session_id,
            ))
            .expect("the workspace session's metadata must be readable")
            .repo_path
            .expect("the workspace session records its worktree"),
        );

        Self {
            session_id: started.session_id,
            codebase_session_id,
            worktree,
            ws_url,
            livekit,
            _agent: agent,
            _codebase: codebase,
            _agent_rpc: agent_rpc,
            _codebase_rpc: codebase_rpc,
            _repo: repo_dir,
            _stubs: stub_dir,
        }
    }

    /// The room the facilitating daemon hosts for this session.
    fn room(&self) -> String {
        session_room_name(&self.session_id)
    }
}

// ---------------------------------------------------------------------------
// A participant of the session room
// ---------------------------------------------------------------------------

struct AgentProbe {
    room: Arc<Room>,
    activity: tokio::sync::mpsc::UnboundedReceiver<WorktreeActivityEvent>,
}

impl AgentProbe {
    async fn join(split: &SplitSession, identity: &str) -> Self {
        let token = split
            .livekit
            .generate_token(&split.room(), identity)
            .expect("LiveKit token for an agent probe");
        let (room, events) = Room::connect(&split.ws_url, &token, RoomOptions::default())
            .await
            .unwrap_or_else(|e| panic!("probe {identity} must join {}: {e}", split.room()));
        Self {
            room: Arc::new(room),
            activity: spawn_activity_subscription(events),
        }
    }

    /// An RPC client aimed at the *facilitating* daemon — the only identity a participant of this
    /// room ever addresses, whichever host the files are on.
    fn rpc_to_facilitating_daemon(&self) -> RpcClient {
        LiveKitRpcClientFactory::for_room(self.room.clone()).client(rpc_identity(AGENT_INSTANCE_ID))
    }

    /// The next `commit` event, discarding other kinds.
    ///
    /// Discarding rather than asserting on the first arrival: a checkout can legitimately report a
    /// files-changed event alongside a commit, and this test is about the commit crossing the wire.
    async fn next_commit(&mut self) -> WorktreeActivityEvent {
        let deadline = tokio::time::Instant::now() + ACTIVITY_TIMEOUT;
        let mut seen = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, self.activity.recv())
                .await
                .unwrap_or_else(|_| {
                    panic!("no commit event arrived within {ACTIVITY_TIMEOUT:?}; saw {seen:?}")
                })
                .expect("the activity subscription must stay open");
            if event.kind() == WorktreeActivityKind::Commit {
                return event;
            }
            seen.push(event.kind());
        }
    }
}

fn spawn_activity_subscription(
    mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
) -> tokio::sync::mpsc::UnboundedReceiver<WorktreeActivityEvent> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let RoomEvent::DataReceived { payload, topic, .. } = event else {
                continue;
            };
            if topic.as_deref() != Some(WORKTREE_ACTIVITY_TOPIC) {
                continue;
            }
            let decoded = WorktreeActivityEvent::decode(&payload[..])
                .expect("a worktree.activity payload must decode as a WorktreeActivityEvent");
            if tx.send(decoded).is_err() {
                return;
            }
        }
    });
    rx
}

async fn read_file_in_room(
    client: &RpcClient,
    session_id: &str,
    path: &str,
) -> ExecuteToolResponse {
    let bytes = tokio::time::timeout(
        CALL_TIMEOUT,
        client.call_unary(
            "connection.ConnectionService",
            "ExecuteTool",
            ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: "Read".to_string(),
                args_json: serde_json::json!({ "path": path }).to_string(),
                daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .expect("ExecuteTool in the session room must return within the timeout")
    .expect("ExecuteTool in the session room must succeed");
    ExecuteToolResponse::decode(&bytes[..]).expect("ExecuteToolResponse must decode")
}

// ---------------------------------------------------------------------------
// AC3b — a forwarded read is indistinguishable from a local one
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_participant_reads_a_file_held_by_the_codebase_daemon_through_the_facilitating_daemon() {
    // Given a split session, and an agent in the room its facilitating daemon hosts
    let split = SplitSession::start().await;
    let probe = AgentProbe::join(&split, "probe-split-reader").await;

    // When it reads a file that exists only on the codebase daemon's filesystem
    let response = read_file_in_room(
        &probe.rpc_to_facilitating_daemon(),
        &split.codebase_session_id,
        SEEDED_FILE,
    )
    .await;

    // Then it gets the checkout's contents. The call was addressed to the facilitating daemon, which
    // holds no copy of this file — it forwarded to the codebase daemon and answered as if it had.
    // That is the whole claim of putting the room on the agent's daemon: a participant addresses one
    // identity in one room and never learns where the files are.
    assert!(
        !response.is_error,
        "the forwarded tool call must succeed; error was '{}'",
        response.error_message
    );
    let result: serde_json::Value =
        serde_json::from_str(&response.result_json).expect("result_json must be JSON");
    assert_eq!(result["content"], SEEDED_CONTENTS);
}

// ---------------------------------------------------------------------------
// AC13 — a commit on the codebase daemon is broadcast in the facilitating daemon's room
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_commit_on_the_codebase_daemon_is_broadcast_in_the_facilitating_daemons_room() {
    // Given a split session, and an agent watching the room its facilitating daemon hosts
    let split = SplitSession::start().await;
    let mut probe = AgentProbe::join(&split, "probe-split-watcher").await;

    // When a commit lands in the checkout — on the other daemon entirely
    std::fs::write(split.worktree.join("committed.txt"), "work in progress")
        .expect("write into the codebase daemon's worktree");
    git(&split.worktree, &["add", "committed.txt"]);
    git(&split.worktree, &["commit", "-m", "add committed.txt"]);
    let head = head_commit_of(&split.worktree);

    // Then the facilitating daemon announces it in its own room, carrying the new sha. It has no
    // checkout to watch, so the only way it could know is the `GetWorktreeSnapshot` round trip —
    // which makes this the proof that the remote measurement feeds the same events the local one does.
    let event = probe.next_commit().await;
    assert_eq!(event.head_commit, head);
}
