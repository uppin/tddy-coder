//! `github.open_vault_idle_ttl_seconds` — how long an open credential vault may go unused before the
//! daemon closes it and drops its data key.
//!
//! An unlocked vault keeps its data key in memory so that PR-status reads can use the stored GitHub
//! token. Session tokens are stateless, so the daemon learns of no session ending except a logout,
//! and a lineage that simply stops coming back would keep its vault open until the daemon exits.
//! So an open vault is closed once nothing has used it — a login sealing into it, an unlock, a
//! session refresh reopening or rotating through it, or a credential read — for this long:
//!
//! | Value | Meaning |
//! |---|---|
//! | absent | [`DEFAULT_OPEN_VAULT_IDLE_TTL_SECONDS`] — the refresh-token lifetime, seven days |
//! | `0` | never: held until its last lineage logs out, or the daemon restarts |
//! | `1`..=[`MAX_OPEN_VAULT_IDLE_TTL_SECONDS`] | that many seconds |
//!
//! Closing is not locking anyone out: the next refresh presenting an unlock key reopens the vault,
//! exactly as after a restart. Its own module so the daemon's config file, already far over its
//! size budget, carries the field and nothing more.

use std::fmt;
use std::time::Duration;

/// The refresh-token lifetime, in seconds: past it no lineage that could have used the vault can
/// refresh any more, so a vault unused this long has nobody left to use it.
const REFRESH_TOKEN_LIFETIME_SECONDS: u64 = tddy_github::REFRESH_TOKEN_TTL.as_secs();

/// The idle lifetime when the config names none: the refresh-token lifetime.
pub const DEFAULT_OPEN_VAULT_IDLE_TTL_SECONDS: u64 = REFRESH_TOKEN_LIFETIME_SECONDS;

/// The longest idle lifetime a config may name — the refresh-token lifetime again. A longer one
/// would keep a vault open for a session that can no longer refresh; `0` is the way to say "never".
pub const MAX_OPEN_VAULT_IDLE_TTL_SECONDS: u64 = REFRESH_TOKEN_LIFETIME_SECONDS;

/// How long an open vault may go unused; `0` is never. Validated as it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct OpenVaultIdleTtl(u64);

impl Default for OpenVaultIdleTtl {
    fn default() -> Self {
        Self(DEFAULT_OPEN_VAULT_IDLE_TTL_SECONDS)
    }
}

impl OpenVaultIdleTtl {
    /// `seconds` as an idle lifetime, or why a config may not name it.
    pub fn from_seconds(seconds: u64) -> Result<Self, String> {
        if seconds > MAX_OPEN_VAULT_IDLE_TTL_SECONDS {
            return Err(format!(
                "github.open_vault_idle_ttl_seconds is at most {MAX_OPEN_VAULT_IDLE_TTL_SECONDS} \
                 (the seven-day refresh-token lifetime); got {seconds}. 0 means an open vault is \
                 held until the daemon restarts"
            ));
        }
        Ok(Self(seconds))
    }

    /// How long an open vault may go unused — `None` when it is never closed for idleness.
    pub fn lifetime(self) -> Option<Duration> {
        (self.0 > 0).then(|| Duration::from_secs(self.0))
    }
}

impl fmt::Display for OpenVaultIdleTtl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0 => f.write_str("never (0)"),
            seconds => write!(f, "{seconds} s"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for OpenVaultIdleTtl {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let seconds = u64::deserialize(deserializer).map_err(|e| {
            serde::de::Error::custom(format!(
                "github.open_vault_idle_ttl_seconds must be a whole number of seconds, 0 for \
                 never: {e}"
            ))
        })?;
        Self::from_seconds(seconds).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitHubConfig;

    const SEVEN_DAYS: Duration = Duration::from_secs(7 * 24 * 60 * 60);

    /// The `github:` block `yaml` describes, as the daemon's config loader reads it.
    fn a_github_block(yaml: &str) -> Result<GitHubConfig, String> {
        serde_yaml::from_str(yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn a_github_block_naming_no_idle_lifetime_gets_the_seven_day_refresh_token_lifetime() {
        // Given
        let yaml = "client_id: \"the-app\"\n";

        // When
        let lifetime =
            a_github_block(yaml).map(|github| github.open_vault_idle_ttl_seconds.lifetime());

        // Then
        assert_eq!(lifetime, Ok(Some(SEVEN_DAYS)));
    }

    #[test]
    fn an_idle_lifetime_of_zero_means_an_open_vault_is_never_closed_for_idleness() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: 0\n";

        // When
        let lifetime =
            a_github_block(yaml).map(|github| github.open_vault_idle_ttl_seconds.lifetime());

        // Then
        assert_eq!(lifetime, Ok(None));
    }

    #[test]
    fn an_explicit_idle_lifetime_is_taken_as_given() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: 3600\n";

        // When
        let lifetime =
            a_github_block(yaml).map(|github| github.open_vault_idle_ttl_seconds.lifetime());

        // Then
        assert_eq!(lifetime, Ok(Some(Duration::from_secs(3600))));
    }

    #[test]
    fn an_idle_lifetime_that_is_not_a_number_is_refused_naming_the_setting() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: \"a week\"\n";

        // When
        let refused = a_github_block(yaml).err();

        // Then
        assert!(
            refused
                .as_deref()
                .is_some_and(|e| e.contains("open_vault_idle_ttl_seconds")),
            "expected a refusal naming the setting, got {refused:?}"
        );
    }

    #[test]
    fn a_negative_idle_lifetime_is_refused_naming_the_setting() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: -1\n";

        // When
        let refused = a_github_block(yaml).err();

        // Then
        assert!(
            refused
                .as_deref()
                .is_some_and(|e| e.contains("open_vault_idle_ttl_seconds")),
            "expected a refusal naming the setting, got {refused:?}"
        );
    }

    #[test]
    fn an_idle_lifetime_longer_than_the_refresh_token_lifetime_is_refused_naming_the_limit() {
        // Given
        let yaml = format!(
            "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: {}\n",
            SEVEN_DAYS.as_secs() + 1
        );

        // When
        let refused = a_github_block(&yaml).err();

        // Then
        assert!(
            refused
                .as_deref()
                .is_some_and(|e| e.contains(&SEVEN_DAYS.as_secs().to_string())),
            "expected a refusal naming the limit, got {refused:?}"
        );
    }
}
