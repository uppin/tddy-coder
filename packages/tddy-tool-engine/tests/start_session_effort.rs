//! `StartSession` takes the effort a session starts at, as the New-session form sends it, and the
//! request it becomes carries it (`#orchestrators` 3/11). Levels are per agent, so the argument is a
//! string the schema points at `DescribeSessionStartOptions` for, never an enum.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-start-effort.md`.

use pretty_assertions::assert_eq;
use serde_json::Value;
use tddy_tool_engine::start_session_args::{
    start_session_schema, StartChoices, StartContext, StartSessionArgs,
};

fn no_choices() -> StartChoices {
    StartChoices {
        hosts: Vec::new(),
        recipes: Vec::new(),
        agents: Vec::new(),
        session_types: Vec::new(),
    }
}

fn a_context() -> StartContext {
    StartContext {
        session_token: "tok".to_string(),
        project_id: "proj-1".to_string(),
        orchestrator_session_id: "orch-1".to_string(),
        orchestrator_daemon_instance_id: String::new(),
    }
}

/// The `effort` property's type, whether it is an enum, and whether its description sends the
/// caller to the start options for the levels.
fn shape_of(property: &Value) -> (Option<&str>, bool, bool) {
    (
        property["type"].as_str(),
        property.get("enum").is_some(),
        property["description"]
            .as_str()
            .is_some_and(|description| description.contains("DescribeSessionStartOptions")),
    )
}

#[test]
fn start_session_offers_effort_and_passes_it_through() {
    // Given
    let schema = start_session_schema(&no_choices());
    let args: StartSessionArgs = serde_json::from_value(serde_json::json!({
        "session_type": "claude-cli",
        "model": "opus",
        "effort": "high",
    }))
    .expect("effort is a StartSession argument");

    // When
    let request = args.into_request(a_context());

    // Then — a free string pointing at the start options, and carried into the request
    assert_eq!(
        (shape_of(&schema["properties"]["effort"]), request.effort),
        ((Some("string"), false, true), "high".to_string())
    );
}
