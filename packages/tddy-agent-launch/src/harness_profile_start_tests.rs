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
