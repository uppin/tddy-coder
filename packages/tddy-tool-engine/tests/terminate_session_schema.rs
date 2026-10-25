//! `TerminateSession` tells the calling agent what terminating does, and does not do
//! (`#start-parity` 7/9).

use tddy_tool_engine::project_home_terminate::{terminate_session_tool_def, TERMINATE_SESSION};

#[test]
fn the_description_says_sigterm_and_that_the_worktree_and_branch_are_kept() {
    // When
    let def = terminate_session_tool_def().expect("TerminateSession is defined");

    // Then
    assert_eq!(def.name, TERMINATE_SESSION);
    let description = def.description.to_lowercase();
    for said in ["sigterm", "worktree", "branch", "kept", "resume"] {
        assert!(
            description.contains(said),
            "the description does not say {said:?}: {description}"
        );
    }
}

#[test]
fn it_takes_the_session_and_optionally_its_host() {
    // When
    let def = terminate_session_tool_def().expect("TerminateSession is defined");
    let schema: serde_json::Value = serde_json::from_str(&def.input_schema_json).unwrap();

    // Then
    assert_eq!(schema["required"], serde_json::json!(["session_id"]));
    assert!(!schema["properties"]["daemon_instance_id"]["description"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
}
