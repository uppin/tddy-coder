//! `github.open_vault_idle_ttl_seconds` — read here, meant elsewhere.
//!
//! How long an open credential vault may go unused — no sign-in, unlock, refresh or credential
//! read — before the daemon closes it and drops its data key. The daemon's config reads it as a
//! plain, optional whole number of seconds (`GitHubConfig.open_vault_idle_ttl_seconds:
//! Option<u64>`): a negative or non-numeric value fails the config load, naming the setting.
//!
//! **What the number means is not decided here.** The default and ceiling (the refresh-token
//! lifetime, seven days) and `0` = never are applied by `tddy-daemon-auth`'s `vault_lifetimes`,
//! where the credential vaults are built — it depends on `tddy-github`, whose `REFRESH_TOKEN_TTL`
//! that is, and this crate, which seventeen crates depend on, must not. A value past the ceiling
//! stops the daemon there, at startup.
//!
//! Its own module so the daemon's config file, already far over its size budget, carries the field
//! and nothing more; what is left here is the parsing tests.

#[cfg(test)]
mod tests {
    use crate::config::GitHubConfig;

    /// The `github:` block `yaml` describes, as the daemon's config loader reads it.
    fn a_github_block(yaml: &str) -> Result<GitHubConfig, String> {
        serde_yaml::from_str(yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn a_github_block_naming_no_idle_lifetime_leaves_it_unset() {
        // Given
        let yaml = "client_id: \"the-app\"\n";

        // When
        let seconds = a_github_block(yaml).map(|github| github.open_vault_idle_ttl_seconds);

        // Then
        assert_eq!(seconds, Ok(None));
    }

    #[test]
    fn zero_is_read_as_zero_for_the_vaults_to_take_as_never() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: 0\n";

        // When
        let seconds = a_github_block(yaml).map(|github| github.open_vault_idle_ttl_seconds);

        // Then
        assert_eq!(seconds, Ok(Some(0)));
    }

    #[test]
    fn an_explicit_idle_lifetime_is_read_as_given_even_past_the_limit_the_vaults_apply() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: 604801\n";

        // When
        let seconds = a_github_block(yaml).map(|github| github.open_vault_idle_ttl_seconds);

        // Then
        assert_eq!(seconds, Ok(Some(604_801)));
    }

    #[test]
    fn a_negative_idle_lifetime_is_refused_naming_the_setting() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: -5\n";

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
    fn a_idle_lifetime_that_is_not_a_whole_number_is_refused_naming_the_setting() {
        // Given
        let yaml = "client_id: \"the-app\"\nopen_vault_idle_ttl_seconds: \"ten minutes\"\n";

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
}
