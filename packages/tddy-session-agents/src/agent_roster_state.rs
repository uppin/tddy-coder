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

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_daemon_livekit::livekit_rooms_stream::RoomRoster;
use tddy_daemon_livekit::peer_routing::PeerRouting;
use tddy_daemon_livekit::session_room::SessionRoomRegistry;

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
    /// Who the LiveKit server reports in each room.
    pub room_roster: &'a Arc<dyn RoomRoster>,
    /// The session rooms this daemon hosts.
    pub session_rooms: &'a Arc<SessionRoomRegistry>,
    /// Every session's agent roster.
    pub session_agent_rosters: &'a Arc<SessionAgentRosterStore>,
    /// The clones peers hold for this daemon's sessions.
    pub session_agent_clones: &'a Arc<SessionAgentCloneStore>,
    /// The clones this daemon holds for other daemons' sessions.
    pub hosted_agent_clones: &'a Arc<HostedAgentClones>,
    /// How often a `StreamSessionAgents` subscription re-sends an unchanged roster.
    pub roster_keepalive_interval: Duration,
}
