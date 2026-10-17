//! Publishing to everyone in a session's room on a topic, as a port.

use tddy_rpc::Status;

/// Broadcasts in the rooms this daemon hosts.
pub trait RoomBroadcast: Send + Sync {
    /// Publish `payload` on `topic` in `session_id`'s room. A session whose room is not hosted here
    /// has nobody to hear it: `Ok`, published nowhere.
    fn publish(&self, session_id: &str, topic: &str, payload: Vec<u8>) -> Result<(), Status>;
}

/// No rooms to broadcast in: every publish reaches nobody.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRoomBroadcast;

impl RoomBroadcast for NoRoomBroadcast {
    fn publish(&self, _session_id: &str, _topic: &str, _payload: Vec<u8>) -> Result<(), Status> {
        todo!("NoRoomBroadcast::publish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_wired_publishing_reaches_nobody_without_failing() {
        NoRoomBroadcast
            .publish("1780828020298-abc", "session.agents", b"roster".to_vec())
            .expect("a broadcast nobody hosts is not an error");
    }
}
