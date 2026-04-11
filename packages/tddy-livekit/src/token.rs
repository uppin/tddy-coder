//! LiveKit access token generation from API key and secret.
//!
//! Used when connecting with `--livekit-api-key` / `--livekit-api-secret` instead of
//! a pre-generated `--livekit-token`. Supports automatic refresh by reconnecting
//! 1 minute before expiry.

use livekit_api::access_token::{AccessToken, AccessTokenError, VideoGrants};
use std::time::Duration;

/// Generates LiveKit JWT access tokens from API key and secret.
/// Used for token refresh: generate a new token and reconnect before expiry.
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

    /// Duration after which a new token should be generated and the connection refreshed.
    /// Returns TTL minus 1 minute.
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
    fn livekit_coder_daemon_jwt_defaults_should_not_force_minute_scale_room_reconnects() {
        const JWT_TTL_SECS_USED_BY_CODER_AND_DAEMON: u64 = 120;
        const MIN_SECONDS_BETWEEN_TOKEN_REFRESH_RECONNECTS: u64 = 5 * 60;

        let gen = TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            "room".to_string(),
            "identity".to_string(),
            Duration::from_secs(JWT_TTL_SECS_USED_BY_CODER_AND_DAEMON),
        );
        assert!(
            gen.time_until_refresh() >= Duration::from_secs(MIN_SECONDS_BETWEEN_TOKEN_REFRESH_RECONNECTS),
            "JWT TTL {}s with a {}s pre-expiry slack reconnects every {}s (see TokenGenerator::time_until_refresh and LiveKitParticipant::run_with_reconnect); raise TTL or slack so dashboards do not show ~1/min participant churn",
            JWT_TTL_SECS_USED_BY_CODER_AND_DAEMON,
            60u64,
            gen.time_until_refresh().as_secs()
        );
    }
}
