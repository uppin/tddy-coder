//! Every project-home tool that acts on a target session can name the host that session runs on,
//! and says so in its schema (`#start-parity` 1/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde_json::Value;
use tddy_tool_engine::project_home_disk_refresh::REFRESH_WORKTREE_DISK_USAGE;
use tddy_tool_engine::project_home_git_tools::project_home_git_tool_defs;
use tddy_tool_engine::project_home_terminal_tools::{
    CLAIM_SESSION_TERMINAL, READ_SESSION_TERMINAL, RELEASE_SESSION_TERMINAL, SEND_SESSION_KEYS,
};
use tddy_tool_engine::project_home_terminate::TERMINATE_SESSION;
use tddy_tool_engine::project_home_tools::{
    is_worktree_target_tool, project_home_tool_defs, retarget_args, target_daemon_instance_id,
    DIFF_SESSION_WORKTREE, READ_SESSION_WORKTREE, TARGET_DAEMON_INSTANCE_ID,
};
use tddy_tool_engine::project_home_worktree_tools::{GET_WORKTREE, LIST_WORKTREES};
use tddy_tool_engine::session_follow::{READ_SESSION_ACTIVITY, READ_SESSION_TRANSCRIPT};
use tddy_tool_engine::start_options::DESCRIBE_SESSION_START_OPTIONS;

/// The tools that act on a target session: read, diff, the two worktree tools, the two follow
/// tools, the four terminal tools, terminate, refresh, the seven git tools, and the start-options describe,
/// which names the host it describes.
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
                READ_SESSION_ACTIVITY,
                READ_SESSION_TRANSCRIPT,
                READ_SESSION_TERMINAL,
                CLAIM_SESSION_TERMINAL,
                SEND_SESSION_KEYS,
                RELEASE_SESSION_TERMINAL,
                TERMINATE_SESSION,
                REFRESH_WORKTREE_DISK_USAGE,
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
        20,
        "read, diff, the two worktree tools, the two follow tools, the four terminal tools, terminate, refresh, the seven git tools and the start-options describe"
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

#[test]
fn worktree_tools_are_the_ones_that_need_a_checkout() {
    // Given every tool that reads, diffs, restores or runs git in a checkout, and the tools that act
    // on a session's agent instead
    let mut need_a_checkout: Vec<String> = [READ_SESSION_WORKTREE, DIFF_SESSION_WORKTREE]
        .map(str::to_string)
        .to_vec();
    need_a_checkout
        .push(tddy_tool_engine::project_home_lifecycle_tools::RESTORE_WORKTREE.to_string());
    need_a_checkout.extend(
        project_home_git_tool_defs()
            .into_iter()
            .map(|tool| tool.name),
    );
    let act_on_the_agent = [
        READ_SESSION_TERMINAL,
        CLAIM_SESSION_TERMINAL,
        SEND_SESSION_KEYS,
        RELEASE_SESSION_TERMINAL,
        READ_SESSION_ACTIVITY,
        READ_SESSION_TRANSCRIPT,
        TERMINATE_SESSION,
        tddy_tool_engine::project_home_lifecycle_tools::DELETE_SESSION,
        tddy_tool_engine::project_home_lifecycle_tools::SET_SESSION_MARK,
        tddy_tool_engine::project_home_turn_tools::SEND_SESSION_MESSAGE,
        tddy_tool_engine::project_home_turn_tools::RESUME_SESSION,
    ];

    // When each is classified
    let misclassified: Vec<String> = need_a_checkout
        .iter()
        .filter(|name| !is_worktree_target_tool(name))
        .cloned()
        .chain(
            act_on_the_agent
                .iter()
                .filter(|name| is_worktree_target_tool(name))
                .map(|name| name.to_string()),
        )
        .collect();

    // Then the checkout tools are worktree tools and the agent tools are not
    assert_eq!(misclassified, Vec::<String>::new());
}

#[test]
fn retargeting_names_the_codebase_host_and_session_and_keeps_the_rest() {
    // Given a diff aimed at a split session's agent half, and that session's codebase half
    let args = r#"{"session_id":"agent-half","path":"src/lib.rs"}"#;
    let codebase = tddy_worktree_service::session_target::CodebaseHalf {
        daemon_instance_id: "udoo".to_string(),
        session_id: "codebase-half".to_string(),
    };

    // When the arguments are retargeted at the codebase half
    let retargeted: Value =
        serde_json::from_str(&retarget_args(args, &codebase)).expect("retargeted args are JSON");

    // Then they name the codebase session on its host, and keep every other argument
    assert_eq!(
        retargeted,
        serde_json::json!({
            "session_id": "codebase-half",
            "daemon_instance_id": "udoo",
            "path": "src/lib.rs"
        })
    );
}
