//! A claude-cli child's pending permission, as an orchestrator reads it: `#coagents`'
//! `PermissionRequestView` (`#home-inbox` 6/8).

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::pending_question::PendingQuestion;
use tddy_discovery::subagent::{PermissionChoice, PermissionRequestView, PermissionToolCall};
use tddy_session_activity::answers::permission_request_view;

fn a_permission(tool_name: &str, tool_input_json: &str) -> PendingQuestion {
    PendingQuestion {
        kind: "permission".to_string(),
        question: format!("Run {tool_name}"),
        request_id: "req-1".to_string(),
        tool_name: tool_name.to_string(),
        tool_input_json: tool_input_json.to_string(),
        ..Default::default()
    }
}

fn a_choice(option_id: &str, name: &str) -> PermissionChoice {
    PermissionChoice {
        option_id: option_id.to_string(),
        name: name.to_string(),
        kind: option_id.to_string(),
    }
}

#[test]
fn a_bash_permission_without_suggestions_offers_allow_once_and_reject_once() {
    // When
    let view = permission_request_view(
        &a_permission("Bash", r#"{"command":"cargo publish"}"#),
        false,
    );

    // Then
    assert_eq!(
        view,
        PermissionRequestView {
            request_id: "req-1".to_string(),
            tool_call: PermissionToolCall {
                title: "Bash".to_string(),
                kind: "execute".to_string(),
                raw_input: Some(serde_json::json!({ "command": "cargo publish" })),
            },
            options: vec![
                a_choice("allow_once", "Allow once"),
                a_choice("reject_once", "Reject")
            ],
        }
    );
}

#[test]
fn with_permission_suggestions_allow_always_is_offered_between_them() {
    // When
    let view = permission_request_view(&a_permission("Bash", r#"{"command":"ls"}"#), true);

    // Then
    assert_eq!(
        view.options,
        vec![
            a_choice("allow_once", "Allow once"),
            a_choice("allow_always", "Allow always"),
            a_choice("reject_once", "Reject"),
        ]
    );
}

#[rstest]
#[case::edit("Edit", "edit")]
#[case::write("Write", "edit")]
#[case::read("Read", "read")]
#[case::an_mcp_tool("mcp__github__create_issue", "other")]
fn the_tool_call_kind_is_acps_name_for_the_tool(#[case] tool_name: &str, #[case] kind: &str) {
    // When
    let view = permission_request_view(
        &a_permission(tool_name, r#"{"file_path":"src/lib.rs"}"#),
        false,
    );

    // Then
    assert_eq!(
        (view.tool_call.title.as_str(), view.tool_call.kind.as_str()),
        (tool_name, kind)
    );
}

#[test]
fn an_input_that_is_not_json_is_kept_as_its_text() {
    // When
    let view = permission_request_view(&a_permission("Bash", "not json"), false);

    // Then
    assert_eq!(
        view.tool_call.raw_input,
        Some(serde_json::json!("not json"))
    );
}
