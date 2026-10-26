//! Every project-home tool that acts on a target session can name the host that session runs on,
//! and says so in its schema (`#start-parity` 1/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde_json::Value;
use tddy_tool_engine::project_home_git_tools::project_home_git_tool_defs;
use tddy_tool_engine::project_home_tools::{
    project_home_tool_defs, target_daemon_instance_id, DIFF_SESSION_WORKTREE,
    READ_SESSION_WORKTREE, TARGET_DAEMON_INSTANCE_ID,
};
use tddy_tool_engine::project_home_worktree_tools::{GET_WORKTREE, LIST_WORKTREES};
use tddy_tool_engine::start_options::DESCRIBE_SESSION_START_OPTIONS;

/// The tools that act on a target session: read, diff, the two worktree tools, the seven git tools,
/// and the start-options describe, which names the host it describes.
fn target_tool_schemas() -> Vec<(String, Value)> {
    project_home_tool_defs()
        .into_iter()
        .filter(|tool| {
            [
                READ_SESSION_WORKTREE,
                DIFF_SESSION_WORKTREE,
                LIST_WORKTREES,
                GET_WORKTREE,
                DESCRIBE_SESSION_START_OPTIONS,
            ]
            .contains(&tool.name.as_str())
        })
        .chain(project_home_git_tool_defs())
        .map(|tool| {
            let schema = serde_json::from_str(&tool.input_schema_json).expect("the schema is JSON");
            (tool.name, schema)
        })
        .collect()
}

#[test]
fn every_target_tool_offers_the_targets_host_as_an_optional_described_string() {
    // Given
    let schemas = target_tool_schemas();
    assert_eq!(
        schemas.len(),
        12,
        "read, diff, the two worktree tools, the seven git tools and the start-options describe"
    );

    for (name, schema) in schemas {
        // When
        let property = &schema["properties"][TARGET_DAEMON_INSTANCE_ID];

        // Then
        assert_eq!(
            property["type"], "string",
            "{name} offers no {TARGET_DAEMON_INSTANCE_ID}"
        );
        assert!(
            !property["description"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "{name}'s {TARGET_DAEMON_INSTANCE_ID} is not described"
        );
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert!(
            !required.contains(&TARGET_DAEMON_INSTANCE_ID),
            "{name} must not require a host: empty is this host"
        );
    }
}

#[test]
fn a_named_host_is_read_from_the_arguments() {
    // When
    let host = target_daemon_instance_id(r#"{"session_id":"s1","daemon_instance_id":"host-b"}"#);

    // Then
    assert_eq!(host.as_deref(), Some("host-b"));
}

#[test]
fn an_empty_or_absent_host_is_this_host() {
    // When
    let absent = target_daemon_instance_id(r#"{"session_id":"s1"}"#);
    let empty = target_daemon_instance_id(r#"{"session_id":"s1","daemon_instance_id":""}"#);
    let blank = target_daemon_instance_id(r#"{"session_id":"s1","daemon_instance_id":"  "}"#);

    // Then
    assert_eq!((absent, empty, blank), (None, None, None));
}
