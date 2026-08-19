//! Acceptance tests: a roster call names the daemon that serves it, and is routed there.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC12, AC28)
//!
//! Every roster request carries `daemon_instance_id` — "Routing, as on ExecuteTool: which daemon
//! serves the call. Empty = this one." A **split session** is the case that field exists for: the
//! agent's loop runs on one daemon while the session's codebase — and therefore its roster — lives
//! on another, so the in-jail `tddy-tools` addresses the agent host and names the codebase host.
//! `ExecuteTool` honours that and forwards; the roster calls read the field off the wire and resolve
//! the session locally anyway, which on the agent host is a session that does not exist.
//!
//! No LiveKit and no peer: the daemon is put in a common room with a peer it can name, with **no
//! room connected**, so a call that reaches the forwarding path is refused there by
//! `FailedPrecondition` — an observable that only the forwarding path produces. What is under test
//! is which of the two paths a call takes, not what the far side answers; the far side is covered
//! against a real room in `session_agent_remote_acceptance.rs`.

use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{
    ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver,
};
use tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    AttachSessionAgentRequest, CancelAgentConversationRequest,
    ConnectionService as ConnectionServiceTrait, DetachSessionAgentRequest,
    ListSessionAgentsRequest, OpenAgentConversationRequest, PromptAgentConversationRequest,
    StreamSessionAgentsRequest,
};

/// This daemon: where the agent's loop runs.
const AGENT_HOST: &str = "agent-host";
/// The peer: where the session's worktree, and therefore its roster, lives.
const CODEBASE_HOST: &str = "codebase-host";
/// A daemon that is not in this one's common room at all.
const STRANGER_HOST: &str = "stranger-host";
/// The session id the codebase host keys the roster by — the one a split session's tools name.
const SESSION_ON_THE_CODEBASE_HOST: &str = "1780828020298-codebase";

/// What a forwarded **unary** call reports when this daemon has no common-room connection.
const UNARY_WENT_TO_THE_PEER: &str =
    "LiveKit common room is not connected on this daemon; cannot forward an RPC to a peer";
/// What a forwarded **server-streaming** call reports when this daemon has no common-room connection.
const STREAM_WENT_TO_THE_PEER: &str =
    "LiveKit common room is not connected on this daemon; cannot forward a stream to a peer";

const CONFIG_YAML: &str = r#"
daemon_instance_id: "agent-host"
users:
  - github_user: "testuser"
    os_user: "testdev"
"#;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The common room this daemon sees: itself and the daemon holding the codebase. `STRANGER_HOST` is
/// deliberately absent, so naming it is a request no routing can satisfy.
struct ACommonRoomWithTheCodebaseHost;

#[async_trait::async_trait]
impl EligibleDaemonSource for ACommonRoomWithTheCodebaseHost {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        [AGENT_HOST, CODEBASE_HOST]
            .into_iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.to_string()),
                label: format!("{id} (common room)"),
            })
            .collect()
    }
}

/// A daemon in a common room with the codebase host, serving whatever sessions `_sessions` holds.
struct ADaemonInTheCommonRoom {
    service: ConnectionServiceImpl,
    _sessions: tempfile::TempDir,
}

impl ADaemonInTheCommonRoom {
    async fn stream_roster_from(&self, daemon: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .stream_session_agents(Request::new(StreamSessionAgentsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ON_THE_CODEBASE_HOST.to_string(),
                daemon_instance_id: daemon.to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn list_roster_from(
        &self,
        daemon: &str,
    ) -> Result<tddy_service::proto::connection::SessionAgentRoster, tddy_rpc::Status> {
        self.service
            .list_session_agents(Request::new(ListSessionAgentsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ON_THE_CODEBASE_HOST.to_string(),
                daemon_instance_id: daemon.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn attach_on(&self, daemon: &str, agent_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .attach_session_agent(Request::new(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ON_THE_CODEBASE_HOST.to_string(),
                daemon_instance_id: daemon.to_string(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn detach_on(&self, daemon: &str, agent_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .detach_session_agent(Request::new(DetachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ON_THE_CODEBASE_HOST.to_string(),
                daemon_instance_id: daemon.to_string(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn open_conversation_on(
        &self,
        daemon: &str,
        agent_id: &str,
    ) -> Result<(), tddy_rpc::Status> {
        self.service
            .open_agent_conversation(Request::new(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ON_THE_CODEBASE_HOST.to_string(),
                daemon_instance_id: daemon.to_string(),
                agent_id: agent_id.to_string(),
                conversation_id: "conversation-under-routing".to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn prompt_conversation_on(&self, daemon: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .prompt_agent_conversation(Request::new(PromptAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ON_THE_CODEBASE_HOST.to_string(),
                daemon_instance_id: daemon.to_string(),
                conversation_id: "conversation-under-routing".to_string(),
                prompt: "which files define the roster store?".to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn cancel_conversation_on(&self, daemon: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .cancel_agent_conversation(Request::new(CancelAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ON_THE_CODEBASE_HOST.to_string(),
                daemon_instance_id: daemon.to_string(),
                conversation_id: "conversation-under-routing".to_string(),
            }))
            .await
            .map(|_| ())
    }
}

/// A daemon in the common room, serving sessions out of `sessions`, with **no room connected** — so
/// anything it forwards is refused by the transport instead of reaching a peer.
fn a_daemon_in_the_common_room(sessions: tempfile::TempDir) -> ADaemonInTheCommonRoom {
    let config_path = sessions.path().join("config.yaml");
    std::fs::write(&config_path, CONFIG_YAML).expect("write daemon config");
    let config = DaemonConfig::load(&config_path).expect("load daemon config");

    let sessions_base = sessions.path().to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        (token == TEST_TOKEN).then(|| tddy_daemon::test_util::TEST_USER.to_string())
    });

    let service = ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        sessions.path().to_path_buf(),
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: Arc::new(ACommonRoomWithTheCodebaseHost),
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
        None,
        Arc::new(CliSessionManager::new()),
    );

    ADaemonInTheCommonRoom {
        service,
        _sessions: sessions,
    }
}

/// The agent host of a split session: it runs the agent's loop and holds no such session of its own,
/// because the session — worktree, `.session.yaml`, roster — lives on the codebase host.
fn an_agent_host_whose_codebase_lives_on_a_peer() -> ADaemonInTheCommonRoom {
    a_daemon_in_the_common_room(tempfile::tempdir().expect("sessions tempdir"))
}

/// A daemon holding the session itself — the ordinary single-host case.
fn a_daemon_holding_the_session_itself() -> ADaemonInTheCommonRoom {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    write_session(sessions.path(), SESSION_ON_THE_CODEBASE_HOST);
    a_daemon_in_the_common_room(sessions)
}

/// A managed claude-cli session on disk under `sessions_base`.
fn write_session(sessions_base: &Path, session_id: &str) {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(
        &session_dir,
        &SessionMetadata {
            session_id: session_id.to_string(),
            project_id: "project-under-routing".to_string(),
            created_at: "2026-08-19T10:00:00Z".to_string(),
            updated_at: "2026-08-19T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp/worktrees/routing".to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: None,
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
        },
    )
    .expect("write session metadata");
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait RoutingAssertions {
    /// The call left this daemon for the named peer — the only path that reports `refusal`.
    fn assert_went_to_the_peer(&self, refusal: &str);
    /// The call named a daemon that cannot serve it, and was refused as a bad request.
    fn assert_refused_as_unroutable(&self, daemon: &str);
}

impl<T> RoutingAssertions for Result<T, tddy_rpc::Status> {
    fn assert_went_to_the_peer(&self, refusal: &str) {
        let status = self
            .as_ref()
            .err()
            .expect("a call forwarded with no room connected must fail, not be served locally");
        assert_eq!(
            (status.code(), status.message()),
            (Code::FailedPrecondition, refusal),
            "the call was not routed to the peer"
        );
    }

    fn assert_refused_as_unroutable(&self, daemon: &str) {
        let status = self
            .as_ref()
            .err()
            .expect("a call naming a daemon outside the common room must fail");
        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(
            status.message().contains(daemon),
            "the refusal must name the daemon that cannot serve the call, got: {}",
            status.message()
        );
    }
}

// ---------------------------------------------------------------------------
// The roster of a session on another daemon is served there
// ---------------------------------------------------------------------------

/// The headline of the split session, and the call the in-jail `tddy-tools` makes first: without it
/// the roster stays empty on the agent host, every subagent call is refused, and the main agent is
/// told it has no such tool.
#[tokio::test]
async fn the_roster_stream_for_a_session_on_another_daemon_is_forwarded_there() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let streamed = daemon.stream_roster_from(CODEBASE_HOST).await;

    // Then
    streamed.assert_went_to_the_peer(STREAM_WENT_TO_THE_PEER);
}

/// The snapshot read the web makes when it opens a session's Agents panel.
#[tokio::test]
async fn listing_the_agents_of_a_session_on_another_daemon_is_forwarded_there() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let listed = daemon.list_roster_from(CODEBASE_HOST).await;

    // Then
    listed.assert_went_to_the_peer(UNARY_WENT_TO_THE_PEER);
}

/// Selecting an agent has to land on the daemon that keeps the roster, or it is written where
/// nothing reads it.
#[tokio::test]
async fn attaching_an_agent_to_a_session_on_another_daemon_is_forwarded_there() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let attached = daemon
        .attach_on(CODEBASE_HOST, "fastcontext@codebase-host")
        .await;

    // Then
    attached.assert_went_to_the_peer(UNARY_WENT_TO_THE_PEER);
}

/// And deselecting it, for the same reason: a detach served locally leaves the entry standing where
/// the roster actually lives.
#[tokio::test]
async fn detaching_an_agent_from_a_session_on_another_daemon_is_forwarded_there() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let detached = daemon
        .detach_on(CODEBASE_HOST, "fastcontext@codebase-host")
        .await;

    // Then
    detached.assert_went_to_the_peer(UNARY_WENT_TO_THE_PEER);
}

/// The main agent invoking its specialized agent: the conversation is opened against the roster, so
/// it opens on the daemon holding it.
#[tokio::test]
async fn opening_a_conversation_on_a_session_on_another_daemon_is_forwarded_there() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let opened = daemon
        .open_conversation_on(CODEBASE_HOST, "fastcontext@codebase-host")
        .await;

    // Then
    opened.assert_went_to_the_peer(UNARY_WENT_TO_THE_PEER);
}

/// The turn itself. It follows the open: a conversation opened on the codebase host is not a
/// conversation this daemon can prompt, so a prompt served locally reports "not open" for something
/// that is.
#[tokio::test]
async fn prompting_a_conversation_on_a_session_on_another_daemon_is_forwarded_there() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let prompted = daemon.prompt_conversation_on(CODEBASE_HOST).await;

    // Then
    prompted.assert_went_to_the_peer(STREAM_WENT_TO_THE_PEER);
}

/// And the interrupt, which has to reach the same daemon the turn is running on or it cancels
/// nothing.
#[tokio::test]
async fn cancelling_a_conversation_on_a_session_on_another_daemon_is_forwarded_there() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let cancelled = daemon.cancel_conversation_on(CODEBASE_HOST).await;

    // Then
    cancelled.assert_went_to_the_peer(UNARY_WENT_TO_THE_PEER);
}

/// A daemon nobody in this room can reach is a bad request, named as such. Answering it out of local
/// state instead is what turned a misrouted roster read into "this session has no agents" — an
/// answer about the wrong host, indistinguishable from the truth.
#[tokio::test]
async fn naming_a_daemon_outside_the_common_room_is_refused_as_a_bad_request() {
    // Given
    let daemon = an_agent_host_whose_codebase_lives_on_a_peer();

    // When
    let listed = daemon.list_roster_from(STRANGER_HOST).await;

    // Then
    listed.assert_refused_as_unroutable(STRANGER_HOST);
}

/// The other side of the contract: a call naming *this* daemon is served here, so routing by an
/// explicit id costs the single-host case nothing.
#[tokio::test]
async fn a_roster_naming_this_daemon_is_served_locally() {
    // Given
    let daemon = a_daemon_holding_the_session_itself();

    // When
    let roster = daemon.list_roster_from(AGENT_HOST).await;

    // Then
    assert_eq!(
        roster
            .expect("a session on this daemon must be answered here")
            .agents,
        Vec::new()
    );
}

// ---------------------------------------------------------------------------
// The forwarded subscription outlives the relay's idle deadline
// ---------------------------------------------------------------------------

/// A forwarded roster subscription is the one stream in this feature that nobody may let go quiet.
/// The relay carrying it terminates a stream that stops producing — deliberately, so a truncated
/// forward can never read as a complete one — and a roster nobody changes produces nothing for
/// hours. The keepalive is what reconciles the two, so its cadence has to leave room for a lost
/// frame rather than merely beating the deadline once.
#[tokio::test]
async fn re_sends_the_roster_well_inside_the_deadline_a_relay_gives_a_forwarded_stream() {
    // Given
    let deadline = tddy_daemon::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT;

    // When
    let cadence = tddy_daemon::connection_service::ROSTER_KEEPALIVE_INTERVAL;

    // Then — two keepalives fit, so one lost frame does not end the subscription
    assert!(
        cadence * 2 < deadline,
        "the roster keepalive ({cadence:?}) must fit twice inside the relay's idle deadline \
         ({deadline:?})"
    );
}
