//! A split session reaches its room through the split-room port: the room is hosted over the
//! remote checkout, and the agent is spawned with the join the port issued — see
//! `docs/ft/daemon/remote-managed-worktree.md` and `tddy-daemon-extensions/docs/extension-ports.md`.
//!
//! In memory: the port records what it is asked for, and the codebase host is a scripted peer on
//! the common room.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_daemon_auth::{DaemonSigningKey, SessionTokens, StandaloneKeyDirectory, SIGNING_KEY_FILE};
use tddy_daemon_extensions::{
    NoRoomAdmission, NoRoomBroadcast, NoRoomDirectory, NoRoomTokens, NoSessionRooms,
    OpenedSessionRoom, PeerForwarder, ReachableSessions, ReachedSession, RemoteSnapshotSource,
    SessionTokenMinter, SplitAgentJoin, SplitRoomHosting,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_kernel::worktree_snapshot::WorktreeSnapshot;
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    JailedWorkspaceSandboxProvisioner, WorkspaceSandboxRegistry,
};
use tddy_github::{GitHubUser, SessionTokenVerifier, SESSION_TOKEN_TTL};
use tddy_host_service::multi_host::LocalOnlyEligibleDaemonSource;
use tddy_host_service::peer_routing::PeerRouting;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{
    DeleteSessionRequest, DeleteSessionResponse, StartSessionRequest, StartSessionResponse,
};
use tddy_session_agents::agent_host_callbacks::{AgentHostCallbacks, AgentRoster};
use tddy_session_agents::session_agent_clone::{
    HostedAgentClones, HostedClone, SessionAgentCloneStore,
};
use tddy_session_agents::session_agent_roster::SessionAgentRosterStore;
use tddy_session_agents::session_agent_status::SessionAgentActivityStore;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_session_split::split_ports::{
    SplitHost, SplitSessionAgents, SplitSessionFiles, SplitSessions,
};
use tddy_session_split::split_session::{split_remote_tool_env_from_join, SplitSpawnTarget};
use tokio::sync::mpsc;

#[test]
fn a_split_agent_is_spawned_with_the_join_the_port_issued() {
    // Given the join the port issued for the split session's room
    let join = SplitAgentJoin {
        url: "ws://10.0.0.5:7880".to_string(),
        room: "session-1780828020298-abc".to_string(),
        token: "the-agent-join-token".to_string(),
    };
    let target = SplitSpawnTarget {
        session_id: "1780828020298-abc",
        codebase_instance_id: "codebase-host",
        codebase_session_id: "1780828020298-ws",
        session_token: "the-caller-token",
    };

    // When the agent's tool environment is built from it
    let env = split_remote_tool_env_from_join(
        &join,
        "daemon-facilitating-host",
        "the-agent-session-token".to_string(),
        &target,
    );

    // Then the agent reaches the codebase session through that room, with that join and identity
    assert_eq!(
        (
            env.session_id.as_str(),
            env.daemon_instance_id.as_deref(),
            env.session_token.as_str(),
            env.livekit_url.as_deref(),
            env.livekit_room.as_deref(),
            env.livekit_token.as_deref(),
            env.server_identity.as_deref(),
            env.daemon_url.as_str(),
        ),
        (
            "1780828020298-ws",
            Some("codebase-host"),
            "the-agent-session-token",
            Some("ws://10.0.0.5:7880"),
            Some("session-1780828020298-abc"),
            Some("the-agent-join-token"),
            Some("daemon-facilitating-host"),
            "",
        )
    );
}

#[tokio::test]
async fn a_split_start_hosts_the_room_over_the_remote_checkout() {
    // Given a facilitating daemon, and a codebase host that creates the checkout it is asked for
    let daemon = a_facilitating_daemon();

    // When an operator whose access token has 88 seconds left starts a split session placing its
    // codebase on that host
    daemon
        .start_a_split_session_on(CODEBASE_HOST, &daemon.a_caller_token_with_seconds_left(88))
        .await;

    // Then this session's own room is hosted here, over the checkout the codebase host created, and
    // measured with a credential minted per poll under the operator's verified identity — a fresh
    // access token's full life, not what was left on the one the operator started with
    let checkout = daemon.codebase_host.the_checkout_it_created();
    let hosted = daemon.split_rooms.the_room_hosted();
    assert_eq!(
        (
            hosted.session_id.as_str(),
            hosted.session_dir.as_path(),
            hosted.codebase_session_id.as_str(),
            hosted.codebase_instance_id.as_str(),
            daemon.identity_and_lifetime_of(&hosted.poll_credential),
        ),
        (
            checkout.agent_session_id.as_str(),
            daemon.session_dir_of(&checkout.agent_session_id).as_path(),
            checkout.codebase_session_id.as_str(),
            CODEBASE_HOST,
            (OPERATOR_LOGIN.to_string(), SESSION_TOKEN_TTL.as_secs()),
        )
    );
}

// ─── the facilitating daemon ────────────────────────────────────────────────────

const FACILITATING_HOST: &str = "workstation-a";
const CODEBASE_HOST: &str = "workstation-b";
const OPERATOR_LOGIN: &str = "uppin";

/// The daemon running the split agent: its split topic as the host builds it, with the split-room
/// port and the common room both in memory.
struct AFacilitatingDaemon {
    split: SplitSessions,
    split_rooms: Arc<RecordedSplitRooms>,
    codebase_host: Arc<ACodebaseHost>,
    key: DaemonSigningKey,
    tokens: SessionTokens,
    data_dir: tempfile::TempDir,
    _identity: tempfile::TempDir,
    _toolchain: tempfile::TempDir,
    _staging: tempfile::TempDir,
}

fn a_facilitating_daemon() -> AFacilitatingDaemon {
    let data_dir = tempfile::tempdir().expect("data dir");
    let staging = tempfile::tempdir().expect("staging dir");
    let toolchain = a_toolchain_holding_tddy_tools();
    let identity = tempfile::tempdir().expect("identity dir");
    let key = DaemonSigningKey::load_or_generate(&identity.path().join(SIGNING_KEY_FILE))
        .expect("the facilitating daemon generates its keypair");
    let tokens = SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory));
    let config = DaemonConfig {
        daemon_instance_id: Some(FACILITATING_HOST.to_string()),
        toolchain_dir: Some(toolchain.path().to_string_lossy().into_owned()),
        ..DaemonConfig::default()
    };

    let codebase_host = Arc::new(ACodebaseHost::default());
    let peer_routing = PeerRouting::new(
        config.clone(),
        Arc::new(LocalOnlyEligibleDaemonSource::for_config(&config)),
        Some(CommonRoom::new(
            Arc::clone(&codebase_host) as Arc<dyn PeerForwarder>,
            &config,
        )),
    );
    let split_rooms = Arc::new(RecordedSplitRooms::default());
    let rest_of_the_host = Arc::new(TheRestOfTheHost);
    let split = SplitSessions {
        config: config.clone(),
        tddy_data_dir: data_dir.path().to_path_buf(),
        staging_base_dir: staging.path().to_path_buf(),
        peer_routing: peer_routing.clone(),
        split_rooms: Arc::clone(&split_rooms) as Arc<dyn SplitRoomHosting>,
        reachable_sessions: Arc::clone(&rest_of_the_host) as Arc<dyn ReachableSessions>,
        workspace_sandboxes: Arc::new(WorkspaceSandboxRegistry::new()),
        workspace_sandbox_provisioner: Arc::new(JailedWorkspaceSandboxProvisioner),
        claude_cli_manager: Arc::new(CliSessionManager::new()),
        session_tokens: Some(tokens.clone()),
        agent_roster: an_agent_roster_with_nothing_attached(
            config,
            data_dir.path(),
            peer_routing,
            Arc::clone(&rest_of_the_host) as Arc<dyn AgentHostCallbacks>,
        ),
        host: rest_of_the_host,
    };

    AFacilitatingDaemon {
        split,
        split_rooms,
        codebase_host,
        key,
        tokens,
        data_dir,
        _identity: identity,
        _toolchain: toolchain,
        _staging: staging,
    }
}

impl AFacilitatingDaemon {
    /// The credential the web caller presents on `StartSession`, with `seconds` of life left.
    fn a_caller_token_with_seconds_left(&self, seconds: u64) -> String {
        self.tokens
            .signer()
            .mint(&the_signed_in_operator(), Duration::from_secs(seconds))
    }

    /// Start a split session placing its codebase on `codebase_host`.
    ///
    /// The recorded port opens no room, so the start stops there — and this checks that it was the
    /// room it stopped at, so a start that failed earlier names where instead of reading as a room
    /// nobody asked for.
    async fn start_a_split_session_on(&self, codebase_host: &str, caller_token: &str) {
        let started = self
            .split
            .start_split_claude_cli_session(
                "operator",
                codebase_host,
                &a_split_claude_cli_request(codebase_host, caller_token),
                &AttachmentProgressSink::discarding(),
            )
            .await;
        let stopped_at = match started {
            Ok(_) => panic!("the recorded port opens no room, so the split start cannot complete"),
            Err(status) => status,
        };
        assert_eq!(
            stopped_at.message(),
            RecordedSplitRooms::OPENS_NO_ROOM,
            "the split start must have reached the room before stopping"
        );
    }

    /// Where this daemon keeps a session's directory.
    fn session_dir_of(&self, session_id: &str) -> PathBuf {
        tddy_core::session_lifecycle::unified_session_dir_path(self.data_dir.path(), session_id)
    }

    /// The login a codebase daemon that learned this daemon's key reads off `token`, and how long
    /// the token was minted to live.
    fn identity_and_lifetime_of(&self, token: &str) -> (String, u64) {
        let claims =
            SessionTokenVerifier::verify(token, &self.key.verifying_key(), SystemTime::now())
                .expect("a poll credential must verify under the facilitating daemon's key");
        (claims.login, claims.exp - claims.iat)
    }
}

/// The operator who pressed *start* in the browser.
fn the_signed_in_operator() -> GitHubUser {
    GitHubUser {
        id: 22_573_333,
        login: OPERATOR_LOGIN.to_string(),
        avatar_url: "https://avatars.githubusercontent.com/u/22573333?v=4".to_string(),
        name: "Mantas Indrasius".to_string(),
    }
}

/// A managed claude-cli start placing its codebase on `codebase_host`, with no agents seeded.
fn a_split_claude_cli_request(codebase_host: &str, caller_token: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: caller_token.to_string(),
        project_id: "tddy-coder".to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-5".to_string(),
        managed_codebase: true,
        codebase_daemon_instance_id: codebase_host.to_string(),
        ..Default::default()
    }
}

/// The deployment's toolchain directory, holding the `tddy-tools` the agent's MCP config names.
/// The agent is never spawned here, so the binary is a placeholder file.
fn a_toolchain_holding_tddy_tools() -> tempfile::TempDir {
    let toolchain = tempfile::tempdir().expect("toolchain dir");
    std::fs::write(toolchain.path().join("tddy-tools"), b"").expect("place tddy-tools");
    toolchain
}

/// The agent topic as a host with no room integration and no agents builds it.
fn an_agent_roster_with_nothing_attached(
    config: DaemonConfig,
    data_dir: &Path,
    peer_routing: PeerRouting,
    host: Arc<dyn AgentHostCallbacks>,
) -> AgentRoster {
    let clones = Arc::new(SessionAgentCloneStore::new());
    AgentRoster {
        config,
        tddy_data_dir: data_dir.to_path_buf(),
        user_resolver: Arc::new(|_| Some(OPERATOR_LOGIN.to_string())),
        peer_routing,
        room_directory: Arc::new(NoRoomDirectory),
        session_rooms: Arc::new(NoSessionRooms),
        room_broadcast: Arc::new(NoRoomBroadcast),
        room_tokens: Arc::new(NoRoomTokens),
        room_connector: Arc::new(tddy_room_client::NoRoomTransport),
        session_agent_rosters: Arc::new(SessionAgentRosterStore::new(
            Arc::clone(&clones),
            Arc::new(SessionAgentActivityStore::new()),
        )),
        session_agent_clones: clones,
        hosted_agent_clones: Arc::new(HostedAgentClones::new()),
        roster_keepalive_interval: Duration::from_secs(8),
        session_admissions: Arc::new(NoRoomAdmission),
        model_registry: None,
        host,
    }
}

// ─── the split-room port, in memory ─────────────────────────────────────────────

/// Records each room it is asked to host and hosts none — the start stops there, as it does for a
/// room that cannot be opened. Joins are issued for any room, so a start reaches the room at all.
#[derive(Default)]
struct RecordedSplitRooms {
    hosted: Mutex<Vec<HostedSplitRoom>>,
}

/// One `host_remote_worktree` call, with the credential its minter issued for the first poll.
struct HostedSplitRoom {
    session_id: String,
    session_dir: PathBuf,
    codebase_session_id: String,
    codebase_instance_id: String,
    poll_credential: String,
}

impl RecordedSplitRooms {
    const OPENS_NO_ROOM: &'static str = "this room is recorded, not opened";

    /// The one room a start asked for.
    fn the_room_hosted(&self) -> HostedSplitRoom {
        let mut hosted = self.hosted.lock().expect("hosted rooms lock");
        assert_eq!(hosted.len(), 1, "a split start hosts exactly one room");
        hosted.remove(0)
    }
}

#[async_trait::async_trait]
impl SplitRoomHosting for RecordedSplitRooms {
    async fn host_remote_worktree(
        &self,
        session_id: &str,
        session_dir: &Path,
        codebase_session_id: &str,
        codebase_instance_id: &str,
        _snapshots: Arc<dyn RemoteSnapshotSource>,
        minter: Arc<dyn SessionTokenMinter>,
        _services: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        self.hosted
            .lock()
            .expect("hosted rooms lock")
            .push(HostedSplitRoom {
                session_id: session_id.to_string(),
                session_dir: session_dir.to_path_buf(),
                codebase_session_id: codebase_session_id.to_string(),
                codebase_instance_id: codebase_instance_id.to_string(),
                poll_credential: minter.mint(),
            });
        Err(Status::unavailable(Self::OPENS_NO_ROOM))
    }

    fn agent_join(
        &self,
        room: &str,
        identity: &str,
        _ttl: Duration,
    ) -> Result<SplitAgentJoin, Status> {
        Ok(SplitAgentJoin {
            url: "ws://livekit.test:7880".to_string(),
            room: room.to_string(),
            token: format!("a-join-for-{identity}"),
        })
    }
}

// ─── the codebase host, on the common room ──────────────────────────────────────

/// The peer holding the codebase: it creates the workspace session it is asked for, under the id it
/// was given, and tears one down when asked.
#[derive(Default)]
struct ACodebaseHost {
    started: Mutex<Vec<StartSessionRequest>>,
}

/// The checkout the codebase host created, and the agent session it was created for.
struct CreatedCheckout {
    codebase_session_id: String,
    agent_session_id: String,
}

impl ACodebaseHost {
    fn the_checkout_it_created(&self) -> CreatedCheckout {
        let started = self.started.lock().expect("started sessions lock");
        assert_eq!(
            started.len(),
            1,
            "a split start creates exactly one checkout"
        );
        CreatedCheckout {
            codebase_session_id: started[0].requested_session_id.clone(),
            agent_session_id: started[0]
                .split_agent
                .as_ref()
                .expect("a split start names the agent session its checkout serves")
                .session_id
                .clone(),
        }
    }
}

#[async_trait::async_trait]
impl PeerForwarder for ACodebaseHost {
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
        assert_eq!(
            (peer_id, service),
            (CODEBASE_HOST, "session.SessionService"),
            "a split start talks to the codebase host's session service only"
        );
        match method {
            "StartSession" => {
                let request = StartSessionRequest::decode(body.as_slice())
                    .expect("a forwarded StartSession decodes");
                let created = StartSessionResponse {
                    session_id: request.requested_session_id.clone(),
                    ..Default::default()
                };
                self.started
                    .lock()
                    .expect("started sessions lock")
                    .push(request);
                Ok(created.encode_to_vec())
            }
            "DeleteSession" => Ok(DeleteSessionResponse::default().encode_to_vec()),
            other => panic!("a split start does not forward {other}"),
        }
    }

    async fn call_server_stream(
        &self,
        _peer_id: &str,
        _service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        panic!("a split start with no agents opens no stream, but {method} was opened")
    }
}

// ─── the rest of the session host ───────────────────────────────────────────────

/// The session host's capabilities a split start does not reach before its room is hosted: each
/// one fails the test naming what was reached.
struct TheRestOfTheHost;

fn not_reached_before_the_room(what: &str) -> ! {
    panic!("a split start reached {what} before its room was hosted")
}

#[async_trait::async_trait]
impl AgentHostCallbacks for TheRestOfTheHost {
    async fn worktree_snapshot(
        &self,
        _session_token: &str,
        _codebase_session_id: &str,
        _codebase_instance_id: &str,
    ) -> Result<WorktreeSnapshot, Status> {
        not_reached_before_the_room("worktree_snapshot")
    }

    async fn run_exec_tool_locally(
        &self,
        _req: &ExecuteToolRequest,
        _sessions_base: &Path,
        _worktree_root: &Path,
    ) -> ExecuteToolResponse {
        not_reached_before_the_room("run_exec_tool_locally")
    }

    fn hosted_clone_for(&self, _session_id: &str) -> Option<Arc<HostedClone>> {
        not_reached_before_the_room("hosted_clone_for")
    }

    async fn run_hosted_clone_tool(
        &self,
        _req: &ExecuteToolRequest,
        _clone: &HostedClone,
    ) -> ExecuteToolResponse {
        not_reached_before_the_room("run_hosted_clone_tool")
    }

    async fn ensure_session_room(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        _worktree_root: &Path,
    ) -> Result<Option<OpenedSessionRoom>, Status> {
        not_reached_before_the_room("ensure_session_room")
    }
}

#[async_trait::async_trait]
impl SplitHost for TheRestOfTheHost {
    async fn start_workspace_session(
        &self,
        _req: StartSessionRequest,
        _progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        not_reached_before_the_room("start_workspace_session")
    }

    async fn delete_session(
        &self,
        _request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        not_reached_before_the_room("delete_session")
    }

    fn session_files(&self) -> Arc<SplitSessionFiles> {
        not_reached_before_the_room("session_files")
    }

    fn session_agents(&self) -> Arc<SplitSessionAgents> {
        not_reached_before_the_room("session_agents")
    }
}

#[async_trait::async_trait]
impl ReachableSessions for TheRestOfTheHost {
    async fn resolve(
        &self,
        _session_token: &str,
        _session_id: &str,
    ) -> Result<ReachedSession, Status> {
        not_reached_before_the_room("resolve")
    }

    fn session_services(&self) -> Result<tddy_rpc::MultiRpcService, Status> {
        not_reached_before_the_room("session_services")
    }

    async fn expose_terminal(&self, _session_id: &str) -> Result<(), Status> {
        not_reached_before_the_room("expose_terminal")
    }
}
