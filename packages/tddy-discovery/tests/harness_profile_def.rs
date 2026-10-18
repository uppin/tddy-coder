//! Harness profiles in `<tddyhome>/agents/`: loaded beside the OpenAI-loop definitions, refused when
//! they also name an endpoint, and listed as agents.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-agent-def-route.md

use tddy_discovery::agent_def::load_agent_defs;
use tddy_discovery::harness_profile::{
    harness_profile_rows, load_harness_profiles, parse_harness_profile, HarnessProfile,
};

const A_PROFILE: &str = "name: claude-on-meta\nlabel: Claude Code · Meta Wonder\nharness: claude-cli\nprovider_account: meta-hackathon\nmodel: muse-wonder-1.0\n";
const AN_OPENAI_DEF: &str = "name: repo-explorer\nmodel: qwen3\nbase_url: http://127.0.0.1:11434\n";

fn the_profile() -> HarnessProfile {
    HarnessProfile {
        name: "claude-on-meta".to_string(),
        label: Some("Claude Code · Meta Wonder".to_string()),
        harness: "claude-cli".to_string(),
        provider_account: "meta-hackathon".to_string(),
        model: "muse-wonder-1.0".to_string(),
    }
}

#[test]
fn a_profile_parses_from_its_yaml() {
    // Then
    assert_eq!(parse_harness_profile(A_PROFILE), Ok(the_profile()));
}

#[test]
fn a_profile_with_a_base_url_is_rejected_at_load() {
    // When
    let refusal = parse_harness_profile(&format!("{A_PROFILE}base_url: http://evil\n"))
        .expect_err("a profile is not an endpoint");

    // Then
    assert!(refusal.contains("base_url"), "{refusal}");
}

#[test]
fn profiles_and_openai_definitions_load_side_by_side() {
    // Given
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("claude-on-meta.yaml"), A_PROFILE).unwrap();
    std::fs::write(dir.path().join("repo-explorer.yaml"), AN_OPENAI_DEF).unwrap();

    // When
    let profiles = load_harness_profiles(dir.path());
    let defs = load_agent_defs(dir.path());

    // Then: each loader takes its own file and leaves the other's alone
    assert_eq!(profiles, vec![the_profile()]);
    assert_eq!(
        defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["repo-explorer"]
    );
}

#[test]
fn profiles_appear_in_the_agent_list() {
    // Then
    assert_eq!(
        harness_profile_rows(&[the_profile()]),
        vec![(
            "claude-on-meta".to_string(),
            "Claude Code · Meta Wonder".to_string()
        )]
    );
}
