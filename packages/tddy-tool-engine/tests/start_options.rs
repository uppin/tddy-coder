//! `DescribeSessionStartOptions` says what each of its arguments narrows (`#start-parity` 5/9).

use pretty_assertions::assert_eq;
use tddy_tool_engine::start_options::start_options_tool_def;

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

// ---------------------------------------------------------------------------
// `#remote-shell` 11/11 — the derived hosts a host reaches
// ---------------------------------------------------------------------------

#[test]
fn a_derived_host_choice_carries_what_a_start_needs_and_what_an_agent_reads() {
    // Given a stopped Coder workspace as the options would list it
    let choice = tddy_tool_engine::start_options::DerivedHostChoice {
        id: "host-a/coder/coder-3f1c@coder.example.com/ada/devbox".to_string(),
        label: "devbox (Work)".to_string(),
        kind: "coder".to_string(),
        parent_daemon_instance_id: "host-a".to_string(),
        usable: false,
        unusable_reason: "the workspace is stopped".to_string(),
    };

    // When it is serialised for the agent
    let json = serde_json::to_value(&choice).unwrap();

    // Then every field an agent reads is there under its name, and the round trip is exact
    assert_eq!(
        json,
        serde_json::json!({
            "id": "host-a/coder/coder-3f1c@coder.example.com/ada/devbox",
            "label": "devbox (Work)",
            "kind": "coder",
            "parent_daemon_instance_id": "host-a",
            "usable": false,
            "unusable_reason": "the workspace is stopped"
        })
    );
    assert_eq!(
        serde_json::from_value::<tddy_tool_engine::start_options::DerivedHostChoice>(json).unwrap(),
        choice
    );
}

#[test]
fn the_description_says_usable_means_listed_as_startable_not_probed() {
    // When
    let description = start_options_tool_def().description;

    // Then
    assert!(
        description.contains("listed as startable, not probed"),
        "{description}"
    );
}
