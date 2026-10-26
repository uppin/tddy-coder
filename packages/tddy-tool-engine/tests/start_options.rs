//! `DescribeSessionStartOptions` offers the stack parents and stack bases the New-session form
//! offers, and says what each of its arguments narrows (`#start-parity` 5/9).
//!
//! The stack pickers are held to the form's own filters through a fixture the web's
//! `stackParentsParity.cy.ts` runs them over.

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use tddy_tool_engine::start_options::{
    pr_stack_orchestrators, stack_base_session_candidates, start_options_tool_def, CandidateSession,
};

fn fixture() -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../tddy-web/cypress/support/fixtures/stack-parent-candidates.json"),
        )
        .expect("the stack fixture exists"),
    )
    .unwrap()
}

fn sessions() -> Vec<CandidateSession> {
    serde_json::from_value(fixture()["sessions"].clone()).unwrap()
}

fn ids(candidates: &[CandidateSession]) -> Vec<String> {
    candidates.iter().map(|c| c.session_id.clone()).collect()
}

fn expected(key: &str) -> Vec<String> {
    serde_json::from_value(fixture()[key].clone()).unwrap()
}

#[test]
fn the_stack_parents_offered_are_the_forms() {
    // When
    let parents = pr_stack_orchestrators(&sessions());

    // Then
    assert_eq!(ids(&parents), expected("stack_parents"));
}

#[test]
fn the_stack_bases_offered_are_the_forms_for_the_same_project_and_host() {
    // Given
    let scope = &fixture()["scope"];

    // When
    let bases = stack_base_session_candidates(
        &sessions(),
        scope["project_id"].as_str().unwrap(),
        scope["daemon_instance_id"].as_str().unwrap(),
    );

    // Then
    assert_eq!(ids(&bases), expected("pr_stack_base_candidates"));
}

#[test]
fn no_project_offers_no_stack_base() {
    // When
    let bases = stack_base_session_candidates(&sessions(), "", "host-a");

    // Then
    assert!(bases.is_empty());
}

#[test]
fn the_tool_describes_every_argument_and_requires_none() {
    // When
    let def = start_options_tool_def();
    let schema: serde_json::Value = serde_json::from_str(&def.input_schema_json).unwrap();

    // Then
    for argument in [
        "daemon_instance_id",
        "session_type",
        "agent",
        "provider_account_id",
    ] {
        assert!(
            !schema["properties"][argument]["description"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "{argument} is not described"
        );
    }
    assert!(schema["required"].as_array().is_none_or(|r| r.is_empty()));
}
