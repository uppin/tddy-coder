//! What `StartProjectSession`'s advertised schema tells the calling agent: which session types it
//! can start, what every argument means, and which session types cannot start without a model.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde_json::Value;
use tddy_core::cli_harness::cli_harnesses;
use tddy_tool_engine::project_home_tools::{project_home_tool_defs, START_PROJECT_SESSION};

fn start_project_session_schema() -> Value {
    let def = project_home_tool_defs()
        .into_iter()
        .find(|tool| tool.name == START_PROJECT_SESSION)
        .expect("StartProjectSession is a project-home tool");
    serde_json::from_str(&def.input_schema_json).expect("the schema is JSON")
}

fn property(name: &str) -> Value {
    start_project_session_schema()["properties"][name].clone()
}

#[test]
fn session_type_offers_tool_workspace_and_every_registered_cli_harness() {
    // Given
    let mut expected = vec!["tool".to_string(), "workspace".to_string()];
    expected.extend(
        cli_harnesses()
            .iter()
            .map(|harness| harness.session_type.to_string()),
    );

    // When
    let offered: Vec<String> = serde_json::from_value(property("session_type")["enum"].clone())
        .expect("session_type carries an enum");

    // Then
    assert_eq!(offered, expected);
}

#[test]
fn every_argument_is_described() {
    // When
    let schema = start_project_session_schema();
    let properties = schema["properties"].as_object().expect("properties");

    // Then
    for (name, property) in properties {
        let description = property["description"].as_str().unwrap_or_default();
        assert!(!description.trim().is_empty(), "{name} has no description");
    }
}

#[test]
fn model_says_which_session_types_need_one() {
    // When
    let description = property("model")["description"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Then
    assert!(description.contains("claude-cli"), "{description}");
    assert!(description.contains("cursor-cli"), "{description}");
    assert!(!description.contains("opencode-cli"), "{description}");
}
