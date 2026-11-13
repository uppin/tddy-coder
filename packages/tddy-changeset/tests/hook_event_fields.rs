//! The fields a hook event carries, as `tddy-tools session-hook` forwards them for the daemon to
//! match an agent spec's `conversation.rotation` against (`#orchestrators` 6/11).

use pretty_assertions::assert_eq;
use tddy_changeset::{hook_event_fields, parse_hook_event};

#[test]
fn a_session_start_after_a_clear_carries_its_source_and_new_conversation() {
    // Given
    let stdin =
        r#"{"hook_event_name":"SessionStart","session_id":"c-1","cwd":"/work","source":"clear"}"#;

    // When
    let fields = hook_event_fields(stdin);

    // Then
    assert_eq!(fields.get("source").map(String::as_str), Some("clear"));
    assert_eq!(fields.get("session_id").map(String::as_str), Some("c-1"));
    assert_eq!(
        fields.get("hook_event_name").map(String::as_str),
        Some("SessionStart")
    );
}

#[test]
fn fields_that_are_not_strings_are_left_out() {
    // Given a tool call whose input is an object and whose flag is a boolean
    let stdin = r#"{"hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"ls"},"stop_hook_active":true}"#;

    // When
    let fields = hook_event_fields(stdin);

    // Then
    assert_eq!(fields.get("tool_name").map(String::as_str), Some("Bash"));
    assert!(!fields.contains_key("tool_input"), "fields: {fields:?}");
    assert!(
        !fields.contains_key("stop_hook_active"),
        "fields: {fields:?}"
    );
}

#[test]
fn json_that_is_not_an_object_carries_no_fields() {
    // When
    let fields = hook_event_fields(r#"["SessionStart"]"#);

    // Then
    assert!(fields.is_empty());
}

#[test]
fn a_session_start_event_parses_its_source() {
    // When
    let event = parse_hook_event(r#"{"hook_event_name":"SessionStart","source":"clear"}"#)
        .expect("a SessionStart event parses");

    // Then
    assert_eq!(event.source.as_deref(), Some("clear"));
}
