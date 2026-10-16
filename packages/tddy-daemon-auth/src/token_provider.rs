//! TokenProvider that delegates to TokenGenerator for LiveKit token generation.

/// TokenProvider that delegates to TokenGenerator. Used when the daemon has LiveKit API key/secret.
pub struct LiveKitTokenProvider(pub std::sync::Arc<tddy_livekit::TokenGenerator>);

impl tddy_service::TokenProvider for LiveKitTokenProvider {
    fn generate_token(&self, room: &str, identity: &str) -> Result<String, String> {
        self.0
            .generate_for(room, identity)
            .map_err(|e| e.to_string())
    }
    fn ttl_seconds(&self) -> u64 {
        self.0.ttl().as_secs()
    }
}

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
        let _ = (room, identity, &self.minter);
        todo!("MinterTokenProvider::generate_token")
    }

    fn ttl_seconds(&self) -> u64 {
        todo!("MinterTokenProvider::ttl_seconds")
    }
}
