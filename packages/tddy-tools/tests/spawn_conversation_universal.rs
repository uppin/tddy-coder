//! `spawn_conversation` is the universal start: it takes every argument `StartSession` takes, and
//! the start it begins can be followed and stopped (`#start-parity` 9/9).

use std::collections::BTreeSet;

use tddy_tool_engine::start_session_args::{offered_start_fields, HostChoice, StartChoices};
use tddy_tools::server::PermissionServer;

fn advertised_schema(name: &str) -> serde_json::Value {
    let def = PermissionServer::advertised_tool_defs()
        .into_iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("{name} is advertised over MCP"));
    serde_json::from_str(&def.input_schema_json).expect("the schema is JSON")
}

#[test]
fn spawn_conversation_accepts_every_start_session_argument() {
    // Given what StartSession offers
    let start_session: BTreeSet<String> = offered_start_fields(&StartChoices {
        hosts: vec![HostChoice {
            daemon_instance_id: "host-b".to_string(),
            label: "host-b".to_string(),
        }],
        recipes: vec!["tdd".to_string()],
    })
    .into_iter()
    .collect();
    assert!(
        !start_session.is_empty(),
        "StartSession offers its arguments"
    );

    // When
    let schema = advertised_schema("spawn_conversation");
    let accepted: BTreeSet<String> = schema["properties"]
        .as_object()
        .expect("spawn_conversation's properties")
        .keys()
        .cloned()
        .collect();

    // Then
    let missing: Vec<&String> = start_session.difference(&accepted).collect();
    assert!(
        missing.is_empty(),
        "StartSession arguments spawn_conversation does not take: {missing:?}"
    );
}

#[test]
fn a_spawned_conversations_start_can_be_followed_and_cancelled() {
    // When
    let status = advertised_schema("spawn_conversation_status");
    let cancel = advertised_schema("spawn_conversation_cancel");

    // Then both take the handle spawn_conversation returned
    assert_eq!(status["required"], serde_json::json!(["start_id"]));
    assert_eq!(cancel["required"], serde_json::json!(["start_id"]));
}
