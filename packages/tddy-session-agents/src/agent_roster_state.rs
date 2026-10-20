//! The host fields the agent roster, its clones and agent-def resolution read.
//!
//! The session host (`tddy-session-lifecycle`'s `DaemonSessionHost`) sits above this crate, so the
//! code that moved here cannot take the host itself. It takes this instead: a view the host lends
//! for the length of one call, borrowing each field rather than cloning it. Every shared field is
//! lent as the `Arc` the host holds, so moved code that hands a store to a task clones the same
//! handle the host would have.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_extensions::{
    RoomAdmission, RoomBroadcast, RoomDirectory, RoomTokenMinter, SessionRooms,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::peer_routing::PeerRouting;
use tddy_model_registry::ModelRegistryStore;
use tddy_room_client::RoomConnector;

use crate::session_agent_clone::{HostedAgentClones, SessionAgentCloneStore};
use crate::session_agent_roster::SessionAgentRosterStore;

/// The session host's roster fields, borrowed for one call.
#[derive(Clone, Copy)]
pub struct AgentRosterState<'a> {
    /// The daemon's configuration.
    pub config: &'a DaemonConfig,
    /// The daemon's data root — the parent of every user's sessions and projects.
    pub tddy_data_dir: &'a Path,
    /// Maps a caller's session token to their GitHub login.
    pub user_resolver: &'a SessionUserResolver,
    /// How the host routes an addressed request to a peer daemon.
    pub peer_routing: &'a PeerRouting,
    /// Who the room server reports in each room.
    pub room_directory: &'a Arc<dyn RoomDirectory>,
    /// The session rooms this daemon hosts.
    pub session_rooms: &'a Arc<dyn SessionRooms>,
    /// Broadcasts in those rooms.
    pub room_broadcast: &'a Arc<dyn RoomBroadcast>,
    /// Mints the token a clone self-joins its session room with, and the one an owning daemon is
    /// admitted with.
    pub room_tokens: &'a Arc<dyn RoomTokenMinter>,
    /// How a hosted clone joins the facilitating daemon's session room.
    pub room_connector: &'a Arc<dyn RoomConnector>,
    /// Every session's agent roster.
    pub session_agent_rosters: &'a Arc<SessionAgentRosterStore>,
    /// The clones peers hold for this daemon's sessions.
    pub session_agent_clones: &'a Arc<SessionAgentCloneStore>,
    /// The clones this daemon holds for other daemons' sessions.
    pub hosted_agent_clones: &'a Arc<HostedAgentClones>,
    /// How often a `StreamSessionAgents` subscription re-sends an unchanged roster.
    pub roster_keepalive_interval: Duration,
    /// The owning daemons this daemon, as the facilitating one, has admitted to its session rooms.
    pub session_admissions: &'a Arc<dyn RoomAdmission>,
    /// This daemon's model registry — the assistants an agent id may resolve to — when one is wired.
    pub model_registry: &'a Option<Arc<ModelRegistryStore>>,
}
