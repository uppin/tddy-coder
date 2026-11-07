//! A session delete asks [`AgentRoster::refuse_an_unreachable_clone_host`] before it tears anything
//! down (`#agent-tools 30/32`): a session with a clone on another daemon cannot lose that clone
//! while this daemon has no common room, so the delete is refused with the session intact rather
//! than refused halfway.
//!
//! In memory: a roster over the null room ports, on a daemon configured for no common room.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_extensions::{
    NoRoomAdmission, NoRoomBroadcast, NoRoomDirectory, NoRoomTokens, NoSessionRooms, OpenedRoom,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::worktree_snapshot::WorktreeSnapshot;
use tddy_host_service::peer_routing::PeerRouting;
use tddy_rpc::{Code, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};

use crate::agent_host_callbacks::{AgentHostCallbacks, AgentRoster, NoRoomTransport};
use crate::session_agent_clone::{HostedAgentClones, HostedClone, SessionAgentCloneStore};
use crate::session_agent_roster::SessionAgentRosterStore;
use crate::session_agent_status::SessionAgentActivityStore;

const SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-40973114dd01";
const OWNING_DAEMON: &str = "workstation-b";
const CLONE_SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-40973114dd02";

#[tokio::test]
async fn a_session_with_a_clone_is_refused_while_no_common_room_is_configured() {
    // Given a session with a clone on another daemon, on a daemon with no common room
    let roster = a_roster_with_no_common_room();
    roster.has_a_clone_of(SESSION_ID, on(OWNING_DAEMON), CLONE_SESSION_ID);

    // When a delete asks whether its clones can go
    let refusal = roster
        .refuse_an_unreachable_clone_host(SESSION_ID)
        .await
        .expect_err("a clone no common room can reach refuses the delete up front");

    // Then it is refused as the teardown would refuse it, naming the clone left behind
    assert_eq!(
        refusal.code(),
        Code::FailedPrecondition,
        "refused as the teardown refuses; got {:?}: {}",
        refusal.code(),
        refusal.message()
    );
    assert!(
        refusal.message().contains(CLONE_SESSION_ID) && refusal.message().contains(OWNING_DAEMON),
        "the refusal names the clone and the daemon holding it: {}",
        refusal.message()
    );
}

#[tokio::test]
async fn a_session_without_clones_is_not_refused() {
    // Given a session with no clone, on a daemon with no common room
    let roster = a_roster_with_no_common_room();

    // When a delete asks whether its clones can go
    let answer = roster.refuse_an_unreachable_clone_host(SESSION_ID).await;

    // Then nothing stands in the way: it needs no room
    assert!(
        answer.is_ok(),
        "a session with no clone needs no room: {answer:?}"
    );
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn on(daemon: &str) -> &str {
    daemon
}

impl AgentRoster {
    fn has_a_clone_of(&self, session_id: &str, daemon: &str, clone_session_id: &str) {
        let (minted, created) = self
            .session_agent_clones
            .claim(session_id, daemon, || clone_session_id.to_string());
        assert!(
            created && minted == clone_session_id,
            "the clone is recorded"
        );
    }
}

fn a_roster_with_no_common_room() -> AgentRoster {
    let config = DaemonConfig::default();
    let coding_agents = Arc::new(tddy_coding_agent_specs::CodingAgentRegistry::empty());
    let clones = Arc::new(SessionAgentCloneStore::new());
    AgentRoster {
        config: config.clone(),
        tddy_data_dir: std::env::temp_dir(),
        user_resolver: Arc::new(|_| None),
        peer_routing: PeerRouting::local_only(&config),
        room_directory: Arc::new(NoRoomDirectory),
        session_rooms: Arc::new(NoSessionRooms),
        room_broadcast: Arc::new(NoRoomBroadcast),
        room_tokens: Arc::new(NoRoomTokens),
        room_connector: Arc::new(NoRoomTransport),
        session_agent_rosters: Arc::new(SessionAgentRosterStore::new(
            Arc::clone(&clones),
            Arc::new(SessionAgentActivityStore::new()),
            Arc::clone(&coding_agents),
        )),
        session_agent_clones: clones,
        hosted_agent_clones: Arc::new(HostedAgentClones::default()),
        roster_keepalive_interval: Duration::from_secs(30),
        session_admissions: Arc::new(NoRoomAdmission),
        model_registry: None,
        coding_agent_launcher: None,
        coding_agents,
        host: Arc::new(AHostAPreCheckNeverCalls),
    }
}

/// The host callbacks of a roster whose pre-check reads only its own stores and its peer routing:
/// any call into the host is a test failure.
struct AHostAPreCheckNeverCalls;

const NOT_CALLED: &str = "a delete's clone pre-check calls nothing on the host";

#[async_trait::async_trait]
impl AgentHostCallbacks for AHostAPreCheckNeverCalls {
    async fn worktree_snapshot(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<WorktreeSnapshot, Status> {
        panic!("{NOT_CALLED}")
    }

    async fn run_exec_tool_locally(
        &self,
        _: &ExecuteToolRequest,
        _: &Path,
        _: &Path,
    ) -> ExecuteToolResponse {
        panic!("{NOT_CALLED}")
    }

    fn hosted_clone_for(&self, _: &str) -> Option<Arc<HostedClone>> {
        panic!("{NOT_CALLED}")
    }

    async fn run_hosted_clone_tool(
        &self,
        _: &ExecuteToolRequest,
        _: &HostedClone,
    ) -> ExecuteToolResponse {
        panic!("{NOT_CALLED}")
    }

    async fn ensure_session_room(
        &self,
        _: &str,
        _: &Path,
        _: &Path,
    ) -> Result<Option<OpenedRoom>, Status> {
        panic!("{NOT_CALLED}")
    }
}
