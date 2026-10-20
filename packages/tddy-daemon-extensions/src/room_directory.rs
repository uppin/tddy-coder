//! Who is in a room, from the room server's own listing, as a port.

use tddy_rpc::Status;

/// The room server's view of who is where.
#[async_trait::async_trait]
pub trait RoomDirectory: Send + Sync {
    /// The identities in `room`, in the server's order. `NotFound` when the server has no such room:
    /// a room nobody has opened is not the same fact as a room nobody is in.
    async fn participants(&self, room: &str) -> Result<Vec<String>, Status>;
}

/// No room server: there is no listing to read.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRoomDirectory;

#[async_trait::async_trait]
impl RoomDirectory for NoRoomDirectory {
    async fn participants(&self, _room: &str) -> Result<Vec<String>, Status> {
        Err(Status::failed_precondition(
            "this daemon has no room server configured, so there is no room listing to read",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_nothing_wired_listing_a_room_is_a_failed_precondition() {
        let refused = NoRoomDirectory
            .participants("session-1780828020298-abc")
            .await
            .expect_err("there is no server to ask");

        assert_eq!(refused.code, tddy_rpc::Code::FailedPrecondition);
    }
}
