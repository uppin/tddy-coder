//! Unit tests: what a harness profile starts, and where it sits in the route's precedence.
//!
//! Changeset: docs/dev/1-WIP/2026-10-18-provider-route-agent-def-route.md

use tddy_core::provider_route::{choose_route, RouteChoice, RouteSources};
use tddy_discovery::harness_profile::HarnessProfile;

use crate::harness_profile_start::{profile_start, ProfileStart};

fn a_profile(harness: &str) -> HarnessProfile {
    HarnessProfile {
        name: "p".to_string(),
        label: None,
        harness: harness.to_string(),
        provider_account: "meta-hackathon".to_string(),
        model: "muse-wonder-1.0".to_string(),
    }
}

#[test]
fn a_claude_cli_profile_starts_claude_cli_on_its_account_and_model() {
    // Then
    assert_eq!(
        profile_start(&a_profile("claude-cli")),
        Ok(ProfileStart {
            session_type: Some("claude-cli".to_string()),
            agent: None,
            choice: RouteChoice::Account("meta-hackathon".to_string()),
            model: "muse-wonder-1.0".to_string(),
        })
    );
}

#[test]
fn a_codex_profile_starts_a_tool_session_with_the_codex_agent() {
    // When
    let start = profile_start(&a_profile("codex")).expect("codex is a harness");

    // Then
    assert_eq!(start.session_type, None);
    assert_eq!(start.agent.as_deref(), Some("codex"));
}

#[test]
fn a_profile_naming_no_known_harness_is_refused() {
    // Then
    assert!(profile_start(&a_profile("vim")).is_err());
}

#[test]
fn a_session_level_account_overrides_the_profiles() {
    // Given
    let start = profile_start(&a_profile("claude-cli")).expect("starts");
    let sources = RouteSources {
        session: Some(RouteChoice::Account("fireworks-main".to_string())),
        agent_definition: Some(start.choice),
        user_default: None,
    };

    // Then
    assert_eq!(choose_route(&sources).as_deref(), Some("fireworks-main"));
}

mod through_the_tool_session_route {
    //! A profile that names a tool-session agent starts that agent through the tool-session route
    //! (#provider-route 4/9): the route file it is handed is the profile's account's.

    use std::sync::Arc;

    use tddy_accounts::{endpoints_for, preset, ProviderAccount};
    use tddy_core::provider_route::RouteSources;
    use tddy_core::provider_route_file::route_from_json;
    use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};

    use super::a_profile;
    use crate::harness_profile_start::profile_start;
    use crate::session_acting_identity::SessionAccountAccess;
    use crate::tool_session_route::tool_session_route_json;

    #[test]
    fn a_codex_profile_hands_its_tool_session_the_profiles_account() {
        // Given Ada holds the Meta account the profile names
        let dir = tempfile::tempdir().expect("dir");
        let passphrase = SecretString::new("correct horse battery staple");
        let store = CredentialStore::create(
            &CredentialStore::path_in(dir.path(), "ada"),
            &passphrase,
            "ada",
        )
        .expect("vault");
        let meta = preset("meta").expect("meta");
        store
            .put(
                ProviderAccount {
                    account: AccountId::new("meta-hackathon"),
                    preset: meta,
                    label: "Meta".to_string(),
                    endpoints: endpoints_for(meta, None, None).expect("endpoints"),
                    api_key: SecretString::new("a-meta-key"),
                }
                .to_record(1_760_000_000),
            )
            .expect("stored");
        let vaults = Arc::new(SessionVaults::new(dir.path()));
        vaults.unlock("ada", &passphrase).expect("unlocked");
        let access = SessionAccountAccess::new(
            Some(vaults),
            Arc::new(|t: &str| (t == "tok").then(|| "ada".to_string())),
            "tok",
        );
        let start = profile_start(&a_profile("codex")).expect("starts");

        // When the profile's tool session is started with no session-level choice
        let json = tool_session_route_json(
            &access,
            start.agent.as_deref().expect("a tool-session agent"),
            &RouteSources {
                agent_definition: Some(start.choice),
                ..RouteSources::default()
            },
            &start.model,
        )
        .expect("resolves")
        .expect("a route");

        // Then the codex child runs on the profile's account and model
        let route = route_from_json(&json).expect("a route file");
        assert!(route
            .args
            .iter()
            .any(|a| a.contains("wire_api=\"responses\"")));
        assert!(route.args.iter().any(|a| a == "muse-wonder-1.0"));
        assert_eq!(route.secret_env[0].1.expose(), "a-meta-key");
    }
}
