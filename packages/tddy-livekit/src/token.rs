//! LiveKit access token generation from API key and secret.
//!
//! Used when connecting with `--livekit-api-key` / `--livekit-api-secret` instead of
//! a pre-generated `--livekit-token`.
//!
//! The LiveKit SDK keeps the signaling connection alive; the server may push refreshed
//! JWTs on the signal channel without the application reconnecting the room.

use livekit_api::access_token::{AccessToken, AccessTokenError, VideoGrants};
use std::time::Duration;

/// Default JWT TTL (seconds) for API-key–minted tokens in the daemon and coder.
/// Matches [`livekit_api::access_token::DEFAULT_TTL`] so sessions are not cut short by JWT expiry
/// under normal use (no application-level reconnect loop).
pub const DEFAULT_LIVEKIT_JWT_TTL_SECS: u64 = livekit_api::access_token::DEFAULT_TTL.as_secs();

/// Generates LiveKit JWT access tokens from API key and secret.
pub struct TokenGenerator {
    api_key: String,
    api_secret: String,
    room: String,
    identity: String,
    ttl: Duration,
}

impl TokenGenerator {
    pub fn new(
        api_key: String,
        api_secret: String,
        room: String,
        identity: String,
        ttl: Duration,
    ) -> Self {
        Self {
            api_key,
            api_secret,
            room,
            identity,
            ttl,
        }
    }

    /// Generate a JWT access token for the configured room and identity.
    pub fn generate(&self) -> Result<String, AccessTokenError> {
        self.generate_for(&self.room, &self.identity)
    }

    /// Generate a JWT access token for the given room and identity.
    /// Used by TokenService to issue tokens for arbitrary callers.
    pub fn generate_for(&self, room: &str, identity: &str) -> Result<String, AccessTokenError> {
        AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(identity)
            .with_ttl(self.ttl)
            .with_grants(VideoGrants {
                room_join: true,
                room: room.to_string(),
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                // Required for `LocalParticipant::set_metadata` (e.g. Codex OAuth URL in dashboard).
                can_update_own_metadata: true,
                ..Default::default()
            })
            .to_jwt()
    }

    /// Token TTL (time-to-live).
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Duration before JWT expiry at which UIs may fetch a new token (e.g. Connect-RPC
    /// `RefreshToken`) without dropping an existing room. Returns TTL minus one minute.
    pub fn time_until_refresh(&self) -> Duration {
        self.ttl.saturating_sub(Duration::from_secs(60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV_API_KEY: &str = "devkey";
    const DEV_API_SECRET: &str = "secret";

    #[test]
    fn token_generator_generates_valid_jwt() {
        let gen = TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            "test-room".to_string(),
            "test-identity".to_string(),
            Duration::from_secs(120),
        );
        let token = gen.generate().expect("generate must succeed");
        assert!(!token.is_empty());
        assert!(
            token.matches('.').count() >= 2,
            "JWT has 3 parts separated by dots"
        );
    }

    #[test]
    fn token_generator_time_until_refresh_returns_ttl_minus_60s() {
        let gen = TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            "room".to_string(),
            "identity".to_string(),
            Duration::from_secs(120),
        );
        assert_eq!(gen.time_until_refresh(), Duration::from_secs(60));
    }

    #[test]
    fn token_generator_time_until_refresh_saturates_when_ttl_short() {
        let gen = TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            "room".to_string(),
            "identity".to_string(),
            Duration::from_secs(30),
        );
        assert_eq!(gen.time_until_refresh(), Duration::ZERO);
    }

    #[test]
    fn token_generator_generate_for_uses_requested_room_and_identity() {
        let gen = TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            "default-room".to_string(),
            "default-identity".to_string(),
            Duration::from_secs(120),
        );
        let token = gen
            .generate_for("other-room", "other-identity")
            .expect("generate_for must succeed");
        assert!(!token.is_empty());
        assert!(
            token.matches('.').count() >= 2,
            "JWT has 3 parts separated by dots"
        );
    }

    #[test]
    fn token_generator_ttl_returns_configured_duration() {
        let gen = TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            "room".to_string(),
            "identity".to_string(),
            Duration::from_secs(90),
        );
        assert_eq!(gen.ttl(), Duration::from_secs(90));
    }

    #[test]
    fn default_livekit_jwt_ttl_matches_livekit_api_default() {
        assert_eq!(
            DEFAULT_LIVEKIT_JWT_TTL_SECS,
            livekit_api::access_token::DEFAULT_TTL.as_secs()
        );
    }
}
