//! The MCP turn outcome of a coding subagent's turn that paused on a permission request.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A coding subagent's permission request is the caller's to answer

use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    PermissionChoice, PermissionRequestView, PermissionToolCall, PromptOutcome, StopReason,
};
use tddy_discovery::subagent_runtime::prompt_outcome_json;

fn a_paused_turn() -> PromptOutcome {
    let mut outcome = PromptOutcome::new(
        StopReason::PermissionRequested,
        Vec::new(),
        TokenUsage::default(),
    );
    outcome.permission_request = Some(PermissionRequestView {
        request_id: "perm-1".to_string(),
        tool_call: PermissionToolCall {
            title: "Run cargo test".to_string(),
            kind: "execute".to_string(),
            raw_input: Some(serde_json::json!({ "command": "cargo test" })),
        },
        options: vec![PermissionChoice {
            option_id: "allow-once".to_string(),
            name: "Allow once".to_string(),
            kind: "allow_once".to_string(),
        }],
    });
    outcome
}

#[test]
fn a_paused_turn_names_its_stop_reason_and_its_permission_request() {
    // When
    let json: serde_json::Value =
        serde_json::from_str(&prompt_outcome_json(a_paused_turn())).expect("outcome JSON");

    // Then
    assert_eq!(json["stopReason"], "permission_requested");
    assert_eq!(
        json["permissionRequest"],
        serde_json::json!({
            "requestId": "perm-1",
            "toolCall": {
                "title": "Run cargo test",
                "kind": "execute",
                "rawInput": { "command": "cargo test" },
            },
            "options": [{ "optionId": "allow-once", "name": "Allow once", "kind": "allow_once" }],
        })
    );
}

#[test]
fn an_ordinary_turn_carries_no_permission_request() {
    // When
    let json: serde_json::Value = serde_json::from_str(&prompt_outcome_json(PromptOutcome::new(
        StopReason::EndTurn,
        Vec::new(),
        TokenUsage::default(),
    )))
    .expect("outcome JSON");

    // Then
    assert!(json.get("permissionRequest").is_none(), "{json}");
}
