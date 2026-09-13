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
