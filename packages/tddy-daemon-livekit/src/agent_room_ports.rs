//! The plugin's registries as the core's agent-roster ports: broadcasting in a hosted session room,
//! admitting owning daemons to it, and listing who is in a room.

use std::sync::Arc;

use tddy_daemon_extensions::{RoomAdmission, RoomBroadcast, RoomDirectory};
use tddy_rpc::Status;

use crate::livekit_rooms_stream::RoomRoster;
use crate::session_admission_service::SessionAdmissionRegistry;
use crate::session_room::SessionRoomRegistry;

impl RoomBroadcast for SessionRoomRegistry {
    fn publish(&self, session_id: &str, topic: &str, payload: Vec<u8>) -> Result<(), Status> {
        let _ = (session_id, topic, payload);
        todo!("SessionRoomRegistry as RoomBroadcast")
    }
}

impl RoomAdmission for SessionAdmissionRegistry {
    fn admit(&self, session_id: &str, owning_instance_id: &str) -> bool {
        let _ = (session_id, owning_instance_id);
        todo!("SessionAdmissionRegistry as RoomAdmission: admit")
    }
    fn revoke(&self, session_id: &str, owning_instance_id: &str) -> bool {
        let _ = (session_id, owning_instance_id);
        todo!("SessionAdmissionRegistry as RoomAdmission: revoke")
    }
    fn is_admitted(&self, session_id: &str, owning_instance_id: &str) -> bool {
        let _ = (session_id, owning_instance_id);
        todo!("SessionAdmissionRegistry as RoomAdmission: is_admitted")
    }
    fn revoke_all_for_session(&self, session_id: &str) -> usize {
        let _ = session_id;
        todo!("SessionAdmissionRegistry as RoomAdmission: revoke_all_for_session")
    }
}

/// [`RoomDirectory`] over the server's room listing.
pub struct RosterDirectory(pub Arc<dyn RoomRoster>);

#[async_trait::async_trait]
impl RoomDirectory for RosterDirectory {
    async fn participants(&self, room: &str) -> Result<Vec<String>, Status> {
        let _ = (room, &self.0);
        todo!("RosterDirectory::participants")
    }
}
