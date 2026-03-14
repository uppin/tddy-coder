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
        AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(&self.identity)
            .with_ttl(self.ttl)
            .with_grants(VideoGrants {
                room_join: true,
                room: self.room.clone(),
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                ..Default::default()
            })
            .to_jwt()
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
}
