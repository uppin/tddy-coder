//! The token service's [`tddy_service::TokenProvider`], answered by the daemon's room-token minter.

/// How long a minted room token stays valid: six hours, mirroring `tddy_livekit::DEFAULT_LIVEKIT_JWT_TTL_SECS`
/// (LiveKit's own default). Spelled here because this crate names no LiveKit crate;
/// `tddy-session-sync` spells the same default the same way for the same reason.
pub(crate) const ROOM_TOKEN_TTL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// `tddy_service::TokenProvider` over the daemon's [`tddy_daemon_extensions::RoomTokenMinter`]: the
/// token service's own port, answered by the one minter the daemon built from its configuration.
pub struct MinterTokenProvider {
    minter: std::sync::Arc<dyn tddy_daemon_extensions::RoomTokenMinter>,
    ttl: std::time::Duration,
}

impl MinterTokenProvider {
    /// Tokens minted through `minter`, each valid for `ttl`.
    pub fn new(
        minter: std::sync::Arc<dyn tddy_daemon_extensions::RoomTokenMinter>,
        ttl: std::time::Duration,
    ) -> Self {
        Self { minter, ttl }
    }
}

impl tddy_service::TokenProvider for MinterTokenProvider {
    fn generate_token(&self, room: &str, identity: &str) -> Result<String, String> {
        self.minter
            .mint(room, identity, self.ttl)
            .map_err(|status| status.message)
    }

    fn ttl_seconds(&self) -> u64 {
        self.ttl.as_secs()
    }
}
