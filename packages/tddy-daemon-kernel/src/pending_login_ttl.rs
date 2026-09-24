//! `github.pending_login_ttl_seconds` — how long a sign-in's GitHub token may wait in memory for
//! its owner's credential vault to open.
//!
//! A sign-in over a closed vault (`LOCKED` after a restart, `UNINITIALIZED` before a first
//! passphrase) holds the token GitHub granted in memory, unsealed, until the passphrase opens the
//! vault. That waiting token is also what allows a first passphrase or a reset: only a fresh
//! sign-in proves the caller holds the GitHub account. Left to wait forever, a token nobody unlocks
//! would sit in memory until the daemon exits, and the permission to choose the vault's passphrase
//! would stand as long. So it expires:
//!
//! | Value | Meaning |
//! |---|---|
//! | absent | [`DEFAULT_PENDING_LOGIN_TTL_SECONDS`] — ten minutes |
//! | `0` | never: held until an unlock, a logout or a restart |
//! | `1`..=[`MAX_PENDING_LOGIN_TTL_SECONDS`] | that many seconds |
//!
//! Its own module so the daemon's config file, already far over its size budget, carries the field
//! and nothing more.

use std::fmt;
use std::time::Duration;

/// The lifetime when the config names none: long enough to find and type a passphrase.
pub const DEFAULT_PENDING_LOGIN_TTL_SECONDS: u64 = 600;

/// The longest lifetime a config may name — the seven-day session refresh window
/// (`tddy_github::REFRESH_TOKEN_TTL`). A longer one would outlive every session the sign-in could
/// have started.
pub const MAX_PENDING_LOGIN_TTL_SECONDS: u64 = tddy_github::REFRESH_TOKEN_TTL.as_secs();

/// How long a pending sign-in waits for its vault; `0` is never. Validated as it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct PendingLoginTtl(u64);

impl Default for PendingLoginTtl {
    fn default() -> Self {
        Self(DEFAULT_PENDING_LOGIN_TTL_SECONDS)
    }
}

impl PendingLoginTtl {
    /// `seconds` as a lifetime, or why a config may not name it.
    pub fn from_seconds(seconds: u64) -> Result<Self, String> {
        if seconds > MAX_PENDING_LOGIN_TTL_SECONDS {
            return Err(format!(
                "github.pending_login_ttl_seconds is at most {MAX_PENDING_LOGIN_TTL_SECONDS} \
                 (seven days); got {seconds}. 0 means a pending sign-in never expires"
            ));
        }
        Ok(Self(seconds))
    }

    /// How long a pending sign-in waits — `None` when it never expires.
    pub fn lifetime(self) -> Option<Duration> {
        (self.0 > 0).then(|| Duration::from_secs(self.0))
    }
}

impl fmt::Display for PendingLoginTtl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0 => f.write_str("never (0)"),
            seconds => write!(f, "{seconds} s"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for PendingLoginTtl {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let seconds = u64::deserialize(deserializer).map_err(|e| {
            serde::de::Error::custom(format!(
                "github.pending_login_ttl_seconds must be a whole number of seconds, 0 for never: \
                 {e}"
            ))
        })?;
        Self::from_seconds(seconds).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitHubConfig;

    /// The `github:` block `yaml` describes, as the daemon's config loader reads it.
    fn a_github_block(yaml: &str) -> Result<GitHubConfig, String> {
        serde_yaml::from_str(yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn a_github_block_naming_no_lifetime_gets_ten_minutes() {
        // Given
        let yaml = "client_id: \"the-app\"\n";

        // When
        let lifetime =
            a_github_block(yaml).map(|github| github.pending_login_ttl_seconds.lifetime());

        // Then
        assert_eq!(lifetime, Ok(Some(Duration::from_secs(600))));
    }

    #[test]
    fn a_lifetime_of_zero_means_a_pending_login_never_expires() {
        // Given
        let yaml = "client_id: \"the-app\"\npending_login_ttl_seconds: 0\n";

        // When
        let lifetime =
            a_github_block(yaml).map(|github| github.pending_login_ttl_seconds.lifetime());

        // Then
        assert_eq!(lifetime, Ok(None));
    }

    #[test]
    fn an_explicit_lifetime_is_taken_as_given() {
        // Given
        let yaml = "client_id: \"the-app\"\npending_login_ttl_seconds: 120\n";

        // When
        let lifetime =
            a_github_block(yaml).map(|github| github.pending_login_ttl_seconds.lifetime());

        // Then
        assert_eq!(lifetime, Ok(Some(Duration::from_secs(120))));
    }

    #[test]
    fn a_negative_lifetime_is_refused_naming_the_setting() {
        // Given
        let yaml = "client_id: \"the-app\"\npending_login_ttl_seconds: -5\n";

        // When
        let refused = a_github_block(yaml).err();

        // Then
        assert!(
            refused
                .as_deref()
                .is_some_and(|e| e.contains("pending_login_ttl_seconds")),
            "expected a refusal naming the setting, got {refused:?}"
        );
    }

    #[test]
    fn a_lifetime_that_is_not_a_number_is_refused_naming_the_setting() {
        // Given
        let yaml = "client_id: \"the-app\"\npending_login_ttl_seconds: \"ten minutes\"\n";

        // When
        let refused = a_github_block(yaml).err();

        // Then
        assert!(
            refused
                .as_deref()
                .is_some_and(|e| e.contains("pending_login_ttl_seconds")),
            "expected a refusal naming the setting, got {refused:?}"
        );
    }

    #[test]
    fn a_lifetime_longer_than_a_week_is_refused_naming_the_limit() {
        // Given
        let yaml = format!(
            "client_id: \"the-app\"\npending_login_ttl_seconds: {}\n",
            MAX_PENDING_LOGIN_TTL_SECONDS + 1
        );

        // When
        let refused = a_github_block(&yaml).err();

        // Then
        assert!(
            refused
                .as_deref()
                .is_some_and(|e| e.contains(&MAX_PENDING_LOGIN_TTL_SECONDS.to_string())),
            "expected a refusal naming the limit, got {refused:?}"
        );
    }
}
