//! Shared test helpers for tddy-livekit tests.

#[cfg(test)]
pub mod test_util {
    use std::time::Duration;

    use crate::token::TokenGenerator;

    pub fn a_token_generator() -> TokenGenerator {
        TokenGenerator::new(
            "devkey".into(),
            "secret".into(),
            "test-room".into(),
            "test-identity".into(),
            Duration::from_secs(120),
        )
    }

    pub fn a_token_generator_with_ttl(ttl: Duration) -> TokenGenerator {
        TokenGenerator::new(
            "devkey".into(),
            "secret".into(),
            "test-room".into(),
            "test-identity".into(),
            ttl,
        )
    }
}
