use crate::connection_service::DaemonSessionHost;
use std::sync::Arc;
use tddy_daemon_extensions::{RoomAdmission, RoomBroadcast, RoomDirectory, RoomTokenMinter};
use tddy_session_agents::agent_host_callbacks::RoomConnector;

impl DaemonSessionHost {
    /// Install which owning daemons are admitted to the rooms this daemon hosts (builder). Without
    /// it no admission is ever recorded.
    #[must_use]
    pub fn with_room_admission(mut self, admission: Arc<dyn RoomAdmission>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_room_admission");
        self.session_admissions = admission;
        self
    }

    /// Install the reader of who is in a room (builder). Without it, reading a room's participants
    /// is a failed precondition.
    #[must_use]
    pub fn with_room_directory(mut self, directory: Arc<dyn RoomDirectory>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_room_directory");
        self.room_directory = directory;
        self
    }

    /// Install what publishes to everyone in a session's room (builder). Without it a broadcast
    /// reaches nobody.
    #[must_use]
    pub fn with_room_broadcast(mut self, broadcast: Arc<dyn RoomBroadcast>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_room_broadcast");
        self.room_broadcast = broadcast;
        self
    }

    /// Install what mints a room access token (builder). Without it every mint is a failed
    /// precondition.
    #[must_use]
    pub fn with_room_tokens(mut self, tokens: Arc<dyn RoomTokenMinter>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_room_tokens");
        self.room_tokens = tokens;
        self
    }

    /// Install how this daemon joins a room as a client (builder). Without it every join is
    /// refused.
    #[must_use]
    pub fn with_room_connector(mut self, connector: Arc<dyn RoomConnector>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_room_connector");
        self.room_connector = connector;
        self
    }
}
