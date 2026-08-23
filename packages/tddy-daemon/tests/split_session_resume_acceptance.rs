//! Acceptance: resuming a **split** session — the agent runs here, its worktree lives on another
//! daemon.
//!
//! PRD: `docs/ft/daemon/remote-managed-worktree.md` § Resume,
//! `docs/ft/daemon/session-agent-roster.md` § Tool replacement (AC25).
//!
//! Nothing about a split session's tool transport survives a stop. The `TDDY_REMOTE_*` environment
//! was injected into a process that has exited, the join token it carried is scoped to a TTL that
//! may have elapsed, and there is no `repo_path` on this host to fall back to. All of it has to be
//! re-derived on resume from the one durable part — the `codebase_daemon_instance_id` /
//! `codebase_session_id` pairing in `.session.yaml`.
//!
//! The roster is not among the durable parts. A split session's own `.session.yaml` never holds
//! one: the roster lives beside the codebase, on the workspace session the pairing names, and the
//! agent attached at minute forty is recorded only there. So a resume **contacts the codebase
//! daemon** — which is why these tests stand up two real daemons in a real common room rather than
//! one daemon and a hand-written file. There is no seam to fake it at: the flags Claude is spawned
//! with are fixed for the life of the process, and a resume that assumed an empty roster because a
//! peer was unreachable would hand the main agent back, pre-approved, exactly the tools the
//! operator took away from it.
//!
//! What is real here is the PTY spawn, so the assertions read the environment and argv of the
//! process that was actually launched rather than an intermediate the daemon could get right on its
//! own and hand over wrong.
//!
//! These need the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`) and are
//! `#[serial]` so they own it alone.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use livekit::prelude::RoomOptions;
use serial_test::serial;
use tddy_core::session_agent::SessionAgentRecord;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    spawn_common_room_discovery_task, CommonRoomPeerRegistry, LiveKitDiscoveryHandles,
    LiveKitEligibleDaemonSource,
};
use tddy_github::{GitHubUser, SessionTokenSigner};
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListEligibleDaemonsRequest, ResumeSessionRequest,
};
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, read_recorded_argv};
use tddy_testing_commons::wait::{eventually_awaiting, eventually_blocking};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The deployment-wide secret both daemons' `livekit:` blocks hold. It signs LiveKit room JWTs *and*
/// session tokens, which is what lets the codebase daemon verify a credential minted here.
const LK_API_SECRET: &str = "secret";
const MODEL: &str = "claude-opus-5";
const PROJECT_ID: &str = "split-resume-proj";
/// The session on this daemon — the one being resumed.
const AGENT_SESSION_ID: &str = "0199aaaa-0000-7000-8000-00000000000a";
/// The paired `workspace` session on the codebase daemon, whose worktree the agent works in.
const CODEBASE_SESSION_ID: &str = "0199bbbb-0000-7000-8000-00000000000b";
/// The daemon holding the codebase, the worktree and the roster.
///
/// Deliberately not `split-agent-…`: that prefix is reserved for a split session's *agent*
/// participant and is refused as a daemon advertisement, so a daemon named that way would never be
/// discovered (`livekit_peer_discovery::eligible_daemon_from_participant_fields`).
const CODEBASE_INSTANCE_ID: &str = "split-resume-workstation-b";
/// This daemon — the one resuming the session, hosting its room, and running the agent.
const FACILITATING_INSTANCE_ID: &str = "split-resume-laptop-a";
const COMMON_ROOM: &str = "split-resume-room";

/// The exec-catalog tool the roster's agent takes over in the withdrawal test. `Grep` because it has
/// a native Claude built-in of the same name, so both routes to it are observable in one argv.
const WITHDRAWN_TOOL: &str = "Grep";

/// Lifetime the daemon mints a split agent's join token with (`split_session::SPLIT_AGENT_TOKEN_TTL`).
/// Spelled out rather than imported so that shortening the production TTL fails these tests instead
/// of silently moving the expectation with it.
const EXPECTED_TOKEN_TTL: Duration = Duration::from_secs(86_400);

/// Ceiling on waiting for the spawned stub to record its environment, not an expected duration:
/// locally the file appears in well under a second, and this only guards a PTY spawn starved under
/// a parallel test run.
const STUB_RECORD_TIMEOUT: Duration = Duration::from_secs(10);

/// 45s: both daemons publish their advertisement on the common room's own metadata cadence, and a
/// cold LiveKit container has to accept every participant first.
const PEER_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(45);

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The credential the browser presents on `ResumeSession`, signed with [`LK_API_SECRET`] so both
/// daemons can verify it. Minted once and shared, because the request and the daemons' user
/// resolvers have to agree on the very same string — and the routed roster read carries this token
/// to the codebase host, which authorizes the session directory it reads with it.
fn a_caller_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        SessionTokenSigner::new(LK_API_SECRET.as_bytes()).mint_access(&GitHubUser {
            id: 4242,
            login: current_os_user(),
            avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
            name: "Test User".to_string(),
        })
    })
}

/// The OS user the test process runs as: a real, resolvable user, so the claude-cli spawn needs no
/// privilege drop.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// The identity a daemon actually serves `connection.ConnectionService` on. Fixed `daemon-` prefix,
/// not a lookup — see `docs/ft/web/daemon-selector-livekit-rpc.md`.
fn rpc_identity(instance_id: &str) -> String {
    format!("daemon-{instance_id}")
}

/// A stub `claude` that dumps its environment and its argv, then holds its PTY open.
///
/// Each record is written to a temp file and `mv -f`d into place rather than appended, so a reader
/// polling it sees either nothing or the whole record, never a half-written one that would look like
/// the daemon having built the wrong command line.
fn a_claude_stub_recording_its_launch(dir: &Path, env_file: &Path, argv_file: &Path) -> PathBuf {
    let target = env_file.display();
    a_stub_agent_script(dir, "stub-claude.sh")
        .with_prelude(&format!(
            "env > \"{target}.tmp.$$\"\nmv -f \"{target}.tmp.$$\" \"{target}\""
        ))
        .recording_argv_to(argv_file)
        .then_reading_stdin()
        .build()
}

/// A daemon that can wire a split session: LiveKit credentials for the real common room, and a
/// spawnable `claude`.
fn a_daemon_config(
    ws_url: &str,
    instance_id: &str,
    claude_binary: &Path,
) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let claude_binary = claude_binary.display();
    let yaml = format!(
        r#"
daemon_instance_id: "{instance_id}"
users:
  - github_user: "{user}"
    os_user: "{user}"
allowed_tools:
  - path: /bin/true
    label: true
claude_cli:
  binary_path: {claude_binary}
livekit:
  url: {ws_url}
  api_key: devkey
  api_secret: {LK_API_SECRET}
  common_room: {COMMON_ROOM}
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

/// A service wired to the real common room, so it can be discovered as a peer and can route a call
/// to one.
fn a_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == a_caller_token()).then(|| resolved_user.clone()));

    let config_arc = Arc::new(config.clone());
    let registry = Arc::new(CommonRoomPeerRegistry::new());
    let room_slot = Arc::new(tokio::sync::RwLock::new(None));
    spawn_common_room_discovery_task(config_arc.clone(), registry.clone(), room_slot.clone());
    let eligible: Arc<dyn tddy_daemon::multi_host::EligibleDaemonSource> = Arc::new(
        LiveKitEligibleDaemonSource::new(config_arc, registry, room_slot.clone()),
    );

    ConnectionServiceImpl::new(
        config,
        resolver,
        tddy_data_dir,
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: eligible,
            common_room_livekit_room: room_slot,
        }),
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

/// Serve a daemon's `connection.ConnectionService` on the common room under its production identity
/// — the one `forward_to_peer` addresses. Without this the routed roster read reaches nobody.
async fn serve_on_the_common_room(
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

/// `eventually_awaiting` rather than a hand-rolled poll: when the peer never shows up it panics with
/// the list that *was* returned, which is the difference between "timed out" and "these daemons were
/// visible and yours was not".
async fn wait_until_discovered(service: &ConnectionServiceImpl, peer_instance_id: &str) {
    eventually_awaiting(
        &format!("daemon {peer_instance_id} to be discovered in the common room"),
        PEER_DISCOVERY_TIMEOUT,
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

/// A stopped split session as `StartSession` left it: paired with a workspace session on another
/// daemon, and with **no** `repo_path`, because there is no repository on this host. Its own roster
/// is empty and always will be — a split session's agents are recorded beside the codebase.
fn a_stopped_split_session() -> SessionMetadata {
    SessionMetadata {
        session_id: AGENT_SESSION_ID.to_string(),
        project_id: PROJECT_ID.to_string(),
        created_at: "2026-08-13T10:00:00Z".to_string(),
        updated_at: "2026-08-13T10:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: None,
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(MODEL.to_string()),
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: Some(CODEBASE_INSTANCE_ID.to_string()),
        codebase_session_id: Some(CODEBASE_SESSION_ID.to_string()),
    }
}

/// The workspace session on the codebase daemon that holds the split session's worktree — and its
/// agent roster.
fn a_workspace_session_holding(agents: Vec<SessionAgentRecord>) -> SessionMetadata {
    let agents_rev = agents.len() as u64;
    SessionMetadata {
        session_id: CODEBASE_SESSION_ID.to_string(),
        session_type: Some("workspace".to_string()),
        status: "active".to_string(),
        repo_path: None,
        agents,
        agents_rev,
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        ..a_stopped_split_session()
    }
}

/// An agent on the codebase host's roster that takes `tool` over from the main agent.
///
/// `codebase_session_id: None` because this agent's owner *is* the daemon holding the roster, so it
/// works the real worktree rather than a clone.
fn an_agent_withdrawing(tool: &str) -> SessionAgentRecord {
    SessionAgentRecord {
        agent_id: format!("fastcontext@{CODEBASE_INSTANCE_ID}"),
        name: "fastcontext".to_string(),
        daemon_instance_id: CODEBASE_INSTANCE_ID.to_string(),
        label: Some("FastContext".to_string()),
        model: "microsoft/FastContext-1.0-4B-RL".to_string(),
        replaces: vec![tool.to_string()],
        tools: vec![tool.to_string()],
        codebase_session_id: None,
    }
}

/// Everything a resume needs across both hosts, plus the files the relaunched agent records its
/// launch to.
struct ResumedSplitSession {
    env_file: PathBuf,
    argv_file: PathBuf,
    livekit_url: String,
    _agent_rpc_run: tokio::task::JoinHandle<()>,
    _codebase_rpc_run: tokio::task::JoinHandle<()>,
    _livekit: LiveKitTestkit,
    _agent_sessions: tempfile::TempDir,
    _codebase_sessions: tempfile::TempDir,
    _stubs: tempfile::TempDir,
    _agent_config: tempfile::TempDir,
    _codebase_config: tempfile::TempDir,
}

impl ResumedSplitSession {
    /// The environment of the process the daemon actually launched.
    fn agent_env(&self) -> Vec<(String, String)> {
        let path = self.env_file.clone();
        eventually_blocking(
            "the relaunched agent to record its environment",
            STUB_RECORD_TIMEOUT,
            move || {
                let recorded = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{} not readable yet: {e}", path.display()))?;
                Ok(recorded
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<Vec<_>>())
            },
        )
    }

    fn agent_env_var(&self, name: &str) -> String {
        let env = self.agent_env();
        env.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                // Only the remote-tool half of the environment: the inherited shell environment is
                // hundreds of variables of noise around the handful this feature sets.
                let remote: Vec<&str> = env
                    .iter()
                    .map(|(k, _)| k.as_str())
                    .filter(|k| k.starts_with("TDDY_REMOTE_"))
                    .collect();
                panic!("the relaunched agent had no {name}; its TDDY_REMOTE_* env was {remote:?}")
            })
    }

    /// Every value the relaunched agent was given for `flag`, in argv order.
    fn agent_flag_values(&self, flag: &str) -> Vec<String> {
        let path = self.argv_file.clone();
        let flag = flag.to_string();
        eventually_blocking(
            "the relaunched agent to record its argv",
            STUB_RECORD_TIMEOUT,
            move || {
                let argv = read_recorded_argv(&path)?;
                Ok(argv
                    .windows(2)
                    .filter(|pair| pair[0] == flag)
                    .map(|pair| pair[1].clone())
                    .collect::<Vec<_>>())
            },
        )
    }
}

/// Resume the stopped split session against a codebase daemon whose roster holds `agents`.
async fn resume_a_split_session_whose_roster_holds(
    agents: Vec<SessionAgentRecord>,
) -> ResumedSplitSession {
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let ws_url = livekit.get_ws_url();

    let stub_dir = tempfile::tempdir().unwrap();
    let env_file = stub_dir.path().join("agent-env.txt");
    let argv_file = stub_dir.path().join("agent-argv.txt");
    let claude_stub = a_claude_stub_recording_its_launch(stub_dir.path(), &env_file, &argv_file);

    // The codebase host: the workspace session the pairing names, and the roster beside it.
    let codebase_sessions = tempfile::tempdir().unwrap();
    let codebase_base = codebase_sessions.path().join(current_os_user());
    let codebase_session_dir = codebase_base.join("sessions").join(CODEBASE_SESSION_ID);
    std::fs::create_dir_all(&codebase_session_dir).unwrap();
    write_session_metadata(&codebase_session_dir, &a_workspace_session_holding(agents)).unwrap();
    let (codebase_config_dir, codebase_config) =
        a_daemon_config(&ws_url, CODEBASE_INSTANCE_ID, &claude_stub);
    let codebase_service = a_service(codebase_config, codebase_base);

    // The agent host: the stopped session, and no repository at all.
    let agent_sessions = tempfile::tempdir().unwrap();
    let agent_base = agent_sessions.path().join(current_os_user());
    let agent_session_dir = agent_base.join("sessions").join(AGENT_SESSION_ID);
    std::fs::create_dir_all(&agent_session_dir).unwrap();
    write_session_metadata(&agent_session_dir, &a_stopped_split_session()).unwrap();
    let (agent_config_dir, agent_config) =
        a_daemon_config(&ws_url, FACILITATING_INSTANCE_ID, &claude_stub);
    let agent_service = a_service(agent_config, agent_base);

    let codebase_rpc_run = serve_on_the_common_room(
        &livekit,
        &ws_url,
        CODEBASE_INSTANCE_ID,
        codebase_service.clone(),
    )
    .await;
    let agent_rpc_run = serve_on_the_common_room(
        &livekit,
        &ws_url,
        FACILITATING_INSTANCE_ID,
        agent_service.clone(),
    )
    .await;
    wait_until_discovered(&agent_service, CODEBASE_INSTANCE_ID).await;

    agent_service
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: a_caller_token().to_string(),
            session_id: AGENT_SESSION_ID.to_string(),
        }))
        .await
        .expect("a stopped split session must resume");

    ResumedSplitSession {
        env_file,
        argv_file,
        livekit_url: ws_url,
        _agent_rpc_run: agent_rpc_run,
        _codebase_rpc_run: codebase_rpc_run,
        _livekit: livekit,
        _agent_sessions: agent_sessions,
        _codebase_sessions: codebase_sessions,
        _stubs: stub_dir,
        _agent_config: agent_config_dir,
        _codebase_config: codebase_config_dir,
    }
}

/// Resume the stopped split session against a codebase daemon holding no agents.
async fn resume_a_split_session() -> ResumedSplitSession {
    resume_a_split_session_whose_roster_holds(Vec::new()).await
}

/// Seconds-since-epoch of a JWT's `exp` claim, read from the unverified payload.
fn token_expiry_epoch_secs(jwt: &str) -> i64 {
    use base64::Engine;
    let payload = jwt
        .split('.')
        .nth(1)
        .unwrap_or_else(|| panic!("expected a three-part JWT; got {jwt:?}"));
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .unwrap_or_else(|e| panic!("JWT payload must be base64url: {e}; got {payload:?}"));
    let claims: serde_json::Value = serde_json::from_slice(&decoded)
        .unwrap_or_else(|e| panic!("JWT payload must be JSON: {e}; payload was {payload:?}"));
    claims["exp"]
        .as_i64()
        .unwrap_or_else(|| panic!("JWT must carry an exp claim; claims were {claims}"))
}

fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the epoch")
        .as_secs() as i64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn a_resumed_split_agent_is_pointed_at_the_codebase_hosts_session_not_its_own() {
    // Given a stopped split session paired with a workspace session on another daemon
    // When it is resumed
    let resumed = resume_a_split_session().await;

    // Then — the codebase daemon resolves the worktree from its own sessions base by this id, so
    // the agent's own session id would resolve to nothing there. A co-located resume gets this
    // right by accident because both ids name the same thing; a split resume cannot.
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_SESSION_ID"),
        CODEBASE_SESSION_ID,
        "the resumed agent must address the workspace session on the codebase host"
    );
}

#[tokio::test]
#[serial]
async fn a_resumed_split_agent_is_re_wired_to_its_rooms_host_and_the_daemon_it_was_paired_with() {
    // Given a stopped split session whose only record of the pairing is its .session.yaml
    // When it is resumed
    let resumed = resume_a_split_session().await;

    // Then — the transport is rebuilt from the persisted pairing. A resume that dropped it would
    // leave the agent with no route to a worktree at all, since there is no repository on this host
    // to fall back to.
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_SERVER_IDENTITY"),
        format!("daemon-{FACILITATING_INSTANCE_ID}"),
        "tool calls are addressed at the host of the room the agent rejoins — this daemon; the \
         codebase daemon serves the common room, not the session's own"
    );
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_DAEMON_INSTANCE_ID"),
        CODEBASE_INSTANCE_ID,
        "the resumed agent must name the daemon holding its codebase"
    );
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_LIVEKIT_ROOM"),
        tddy_daemon::session_room::session_room_name(AGENT_SESSION_ID),
        "a resumed agent rejoins its own session's room — the one this daemon hosts as the session's \
         facilitating daemon, not one named after the codebase session on the other host"
    );
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_LIVEKIT_URL"),
        resumed.livekit_url,
        "the tool transport dials the configured LiveKit server"
    );
}

#[tokio::test]
#[serial]
async fn a_resumed_split_agent_receives_a_join_token_minted_at_the_resume() {
    // Given a split session stopped long enough for its original token to matter
    // When it is resumed
    let resumed = resume_a_split_session().await;

    // Then — the token is good for a full lifetime measured from *this* moment, not whatever was
    // left of the one the original agent process carried. The persisted pairing is the only durable
    // part of a split session's wiring; the credential is minted afresh every spawn.
    let expiry = token_expiry_epoch_secs(&resumed.agent_env_var("TDDY_REMOTE_LIVEKIT_TOKEN"));
    let expected = now_epoch_secs() + EXPECTED_TOKEN_TTL.as_secs() as i64;
    // Wall-clock: the token was minted a moment before this line ran, so its expiry trails the
    // expectation by however long the resume took. 60s covers a starved PTY spawn.
    assert!(
        (expected - expiry) >= 0 && (expected - expiry) <= 60,
        "expected the resumed agent's token to expire around {expected} (now + {}s); it expires at {expiry}, {} seconds off",
        EXPECTED_TOKEN_TTL.as_secs(),
        expected - expiry
    );
}

#[tokio::test]
#[serial]
async fn a_resumed_split_agent_loses_the_tools_the_codebase_hosts_roster_withdrew() {
    // Given an agent attached after the session started, recorded only on the codebase host
    let resumed =
        resume_a_split_session_whose_roster_holds(vec![an_agent_withdrawing(WITHDRAWN_TOOL)]).await;

    // When the session is resumed — the roster is read back over the common room

    // Then the withdrawn tool is unreachable, not merely un-pre-approved. Claude's flags are fixed
    // for the life of the process, so this is the only moment the withdrawal can be imposed: a
    // resume that read no roster would hand the main agent the tool the operator gave away.
    let proxied = format!("mcp__tddy-tools__{WITHDRAWN_TOOL}");
    let disallowed = resumed.agent_flag_values("--disallowedTools");
    assert!(
        disallowed.contains(&proxied),
        "the resumed agent must be denied {proxied}; its --disallowedTools were {disallowed:?}"
    );
    let allowed = resumed.agent_flag_values("--allowedTools");
    assert!(
        !allowed.contains(&proxied),
        "the resumed agent must not be pre-approved for {proxied}; its --allowedTools were {allowed:?}"
    );
}

#[tokio::test]
#[serial]
async fn a_resumed_split_agent_keeps_every_tool_when_the_roster_is_empty() {
    // Given a codebase host holding no agents for this session
    let resumed = resume_a_split_session().await;

    // When it is resumed
    // Then the proxied catalog is pre-approved in full. Withdrawal is the roster's doing, so a
    // resume that could not tell "no agents" from "could not ask" would quietly narrow every
    // ordinary split session.
    let proxied = format!("mcp__tddy-tools__{WITHDRAWN_TOOL}");
    let allowed = resumed.agent_flag_values("--allowedTools");
    assert!(
        allowed.contains(&proxied),
        "an unwithdrawn {proxied} must stay pre-approved; --allowedTools were {allowed:?}"
    );
}

/// How much of the codebase host this daemon can see — the two shapes "unreachable" takes, which
/// the resume must refuse identically.
enum CodebaseHostVisibility {
    /// No multi-host configuration at all: nothing to classify the pairing's daemon against, so the
    /// route is refused before any transport is involved.
    NotRoutable,
    /// The peer is in the eligible list, so the call is routed — and then finds no connected room to
    /// carry it. This is the shape a peer that has gone quiet takes, and the one that could regress
    /// into "the roster came back empty".
    RoutedButUnanswered,
}

/// The eligible list a daemon has when it can name the codebase host but not talk to it.
struct ACommonRoomHoldingTheCodebaseHost;

impl tddy_daemon::multi_host::EligibleDaemonSource for ACommonRoomHoldingTheCodebaseHost {
    fn list_eligible_daemons(&self) -> Vec<tddy_daemon::multi_host::EligibleDaemonInfo> {
        vec![tddy_daemon::multi_host::EligibleDaemonInfo {
            instance_id: tddy_daemon::multi_host::DaemonInstanceId(
                CODEBASE_INSTANCE_ID.to_string(),
            ),
            label: "workstation-b".to_string(),
        }]
    }
}

/// A refused resume, and the evidence that nothing was launched by it.
struct RefusedResume {
    status: tddy_rpc::Status,
    env_file: PathBuf,
    argv_file: PathBuf,
    session_dir: PathBuf,
    _stub_dir: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

impl RefusedResume {
    /// The status a peer-less route produces, spelled as the caller sees it.
    fn assert_refused_naming_the_codebase_host(&self, expected: tddy_rpc::Code) {
        assert_eq!(
            self.status.code(),
            expected,
            "the refusal must carry {expected:?}; got {:?}",
            self.status
        );
        assert!(
            self.status.message().contains(CODEBASE_INSTANCE_ID),
            "the refusal must name the host it could not reach; got {:?}",
            self.status
        );
    }

    /// Nothing was launched. Both of the stub's records are checked, the environment one included:
    /// it is written *before* the argv (`a_claude_stub_recording_its_launch`), so a resume that
    /// spawned and then failed shows up here even if the argv write had not landed yet.
    fn assert_no_agent_was_launched(&self) {
        assert!(
            !self.env_file.exists(),
            "no agent may be launched by a resume that was refused; {} was written",
            self.env_file.display()
        );
        assert!(
            !self.argv_file.exists(),
            "no agent may be launched by a resume that was refused; {} was written",
            self.argv_file.display()
        );
    }

    /// The session was left as it was found. Unlike the two file checks this is synchronous with the
    /// refusal — the daemon marks a session active on the path that spawns it — so it holds however
    /// far a regressed resume got before failing.
    fn assert_the_session_was_left_stopped(&self) {
        let meta = tddy_core::session_metadata::read_session_metadata(&self.session_dir)
            .expect("the refused session must still be readable");
        assert_eq!(meta.status, "inactive");
        assert_eq!(meta.pid, None);
    }
}

/// Resume a stopped split session on a daemon that cannot reach the host its pairing names.
///
/// No LiveKit container: the point is what happens when the peer is *not* there, and both shapes of
/// that are reachable without one.
async fn refuse_a_resume_that_cannot_read_the_roster(
    visibility: CodebaseHostVisibility,
) -> RefusedResume {
    let stub_dir = tempfile::tempdir().unwrap();
    let env_file = stub_dir.path().join("agent-env.txt");
    let argv_file = stub_dir.path().join("agent-argv.txt");
    let claude_stub = a_claude_stub_recording_its_launch(stub_dir.path(), &env_file, &argv_file);

    let sessions = tempfile::tempdir().unwrap();
    let sessions_base = sessions.path().join(current_os_user());
    let session_dir = sessions_base.join("sessions").join(AGENT_SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_metadata(&session_dir, &a_stopped_split_session()).unwrap();

    let (_config_dir, config) = a_daemon_config(
        "ws://livekit.invalid:7880",
        FACILITATING_INSTANCE_ID,
        &claude_stub,
    );
    let tddy_data_dir = sessions_base.clone();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == a_caller_token()).then(|| resolved_user.clone()));
    let discovery = match visibility {
        CodebaseHostVisibility::NotRoutable => None,
        CodebaseHostVisibility::RoutedButUnanswered => Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: Arc::new(ACommonRoomHoldingTheCodebaseHost),
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
    };
    let service = ConnectionServiceImpl::new(
        config,
        resolver,
        tddy_data_dir,
        user_resolver,
        None,
        discovery,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    );

    let status = service
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: a_caller_token().to_string(),
            session_id: AGENT_SESSION_ID.to_string(),
        }))
        .await
        .expect_err("a resume that cannot read the roster must be refused");

    RefusedResume {
        status,
        env_file,
        argv_file,
        session_dir,
        _stub_dir: stub_dir,
        _sessions: sessions,
    }
}

/// "The codebase host is unreachable" and "nothing is attached" produce the same empty roster, and
/// reading the second from the first is how a relaunch silently restores a withdrawn tool. A split
/// session whose codebase host cannot be reached has no working tool call in any case.
#[tokio::test]
async fn a_split_session_whose_codebase_host_is_not_routable_is_refused_rather_than_resumed() {
    // Given a daemon with no multi-host configuration at all
    let refused =
        refuse_a_resume_that_cannot_read_the_roster(CodebaseHostVisibility::NotRoutable).await;

    // When it resumes a session paired to a host it cannot classify
    // Then
    refused.assert_refused_naming_the_codebase_host(tddy_rpc::Code::InvalidArgument);
    refused.assert_no_agent_was_launched();
    refused.assert_the_session_was_left_stopped();
}

/// The likelier production shape: the peer is listed in the common room and simply does not answer.
/// The route is taken, so the refusal comes from the transport rather than the classifier — and it
/// must still be a refusal, because this is exactly the path an empty roster could come back on.
#[tokio::test]
async fn a_split_session_whose_codebase_host_does_not_answer_is_refused_rather_than_resumed() {
    // Given a daemon that can name the codebase host but has no room to carry the call
    let refused =
        refuse_a_resume_that_cannot_read_the_roster(CodebaseHostVisibility::RoutedButUnanswered)
            .await;

    // When it resumes the split session
    // Then
    refused.assert_refused_naming_the_codebase_host(tddy_rpc::Code::FailedPrecondition);
    refused.assert_no_agent_was_launched();
    refused.assert_the_session_was_left_stopped();
}
