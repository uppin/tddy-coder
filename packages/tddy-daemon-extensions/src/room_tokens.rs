//! Minting room access tokens, as a port.
//!
//! The daemon decides once — from its configuration, at startup — whether it can mint tokens for an
//! integration's rooms, and hands the answer to everything that needs one. **A minter authorizes
//! nothing**: it mints for whatever room and identity it is asked, so it is handed only to code
//! that has already decided its caller may have the token.

use std::time::Duration;

use tddy_rpc::Status;

/// Mints a token admitting `identity` to `room` for `ttl`.
pub trait RoomTokenMinter: Send + Sync {
    fn mint(&self, room: &str, identity: &str, ttl: Duration) -> Result<String, Status>;
}

/// No integration: every mint is a failed precondition.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRoomTokens;

impl RoomTokenMinter for NoRoomTokens {
    fn mint(&self, _room: &str, _identity: &str, _ttl: Duration) -> Result<String, Status> {
        todo!("NoRoomTokens::mint")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_wired_every_mint_is_a_failed_precondition() {
        let refused = NoRoomTokens
            .mint("session-abc", "bridge-1", Duration::from_secs(60))
            .expect_err("nothing can mint");

        assert_eq!(refused.code, tddy_rpc::Code::FailedPrecondition);
        assert_eq!(
            refused.message,
            "this daemon has no room integration configured, so it can mint no room token for session-abc"
        );
    }
}
