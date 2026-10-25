//! `StartSession` offers every `StartSessionRequest` field the caller may choose, and nothing the
//! proto gains can slip past it: each field is offered, filled by the caller's context, or withheld
//! with a reason (`#start-parity` 2/9).
//!
//! The field list is read from the proto descriptor compiled into `tddy-service`, not written here.
//! The web half of the parity — what the New-session form's request builder sends — is
//! `packages/tddy-web/cypress/component/createSessionRequestParity.cy.ts`, against the fixture
//! [`the_web_fixture_lists_exactly_the_offered_and_context_filled_fields`] keeps current.

use std::collections::BTreeSet;
use std::path::PathBuf;

use prost::Message;
use prost_types::FileDescriptorSet;
use tddy_tool_engine::start_session_args::{
    offered_start_fields, HostChoice, StartChoices, CONTEXT_FILLED_FIELDS, INTERNAL_START_FIELDS,
};

fn start_session_request_fields() -> BTreeSet<String> {
    let set = FileDescriptorSet::decode(tddy_service::SERVICE_DESCRIPTOR_BYTES)
        .expect("the compiled-in descriptor set decodes");
    let message = set
        .file
        .iter()
        .filter(|file| file.package() == "session")
        .flat_map(|file| file.message_type.iter())
        .find(|message| message.name() == "StartSessionRequest")
        .expect("session.StartSessionRequest is in the descriptor set");
    message
        .field
        .iter()
        .map(|field| field.name().to_string())
        .collect()
}

fn any_choices() -> StartChoices {
    StartChoices {
        hosts: vec![HostChoice {
            daemon_instance_id: "host-b".to_string(),
            label: "host-b".to_string(),
        }],
        recipes: vec!["tdd".to_string()],
    }
}

fn names(table: &[(&str, &str)]) -> BTreeSet<String> {
    table.iter().map(|(name, _)| name.to_string()).collect()
}

fn offered() -> BTreeSet<String> {
    offered_start_fields(&any_choices()).into_iter().collect()
}

#[test]
fn every_start_session_request_field_is_offered_context_filled_or_withheld() {
    // Given
    let proto_fields = start_session_request_fields();

    // When
    let accounted: BTreeSet<String> = offered()
        .union(&names(CONTEXT_FILLED_FIELDS))
        .cloned()
        .collect::<BTreeSet<_>>()
        .union(&names(INTERNAL_START_FIELDS))
        .cloned()
        .collect();

    // Then
    let unaccounted: Vec<&String> = proto_fields.difference(&accounted).collect();
    assert!(
        unaccounted.is_empty(),
        "StartSessionRequest fields StartSession neither offers, fills from context, nor withholds \
         with a reason: {unaccounted:?}"
    );
}

#[test]
fn the_tool_offers_no_argument_the_proto_does_not_have() {
    // Given
    let proto_fields = start_session_request_fields();

    // When
    let offered = offered();

    // Then
    let invented: Vec<&String> = offered.difference(&proto_fields).collect();
    assert!(
        invented.is_empty(),
        "offered but not in StartSessionRequest: {invented:?}"
    );
    assert!(
        !offered.is_empty(),
        "StartSession offers no argument at all"
    );
}

#[test]
fn the_context_and_withheld_tables_name_only_real_fields_and_never_an_offered_one() {
    // Given
    let proto_fields = start_session_request_fields();
    let offered = offered();

    for (name, reason) in CONTEXT_FILLED_FIELDS.iter().chain(INTERNAL_START_FIELDS) {
        // Then
        assert!(
            proto_fields.contains(*name),
            "{name} is not a StartSessionRequest field"
        );
        assert!(
            !offered.contains(*name),
            "{name} is both offered and filled or withheld"
        );
        assert!(!reason.trim().is_empty(), "{name} carries no reason");
    }
}

fn web_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tddy-web/cypress/support/fixtures/start-session-tool-fields.json")
}

#[test]
fn the_web_fixture_lists_exactly_the_offered_and_context_filled_fields() {
    // Given what the tool offers today
    let expected = serde_json::json!({
        "offered": offered().into_iter().collect::<Vec<_>>(),
        "context_filled": names(CONTEXT_FILLED_FIELDS).into_iter().collect::<Vec<_>>(),
    });

    // When the committed fixture the web parity spec reads is loaded
    let committed: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(web_fixture_path()).expect("the web fixture exists"),
    )
    .expect("the web fixture is JSON");

    // Then it is current
    assert_eq!(
        committed,
        expected,
        "{} is stale: write the `expected` value above into it",
        web_fixture_path().display()
    );
}
