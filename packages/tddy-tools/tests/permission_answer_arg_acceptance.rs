//! `subagent_resume`'s `permission` argument — the main agent's answer to a coding subagent's
//! pending permission request.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A coding subagent's permission request is the caller's to answer

use tddy_discovery::subagent::PermissionAnswer;
use tddy_tools::permission_answer_arg::parse_permission_answer;

#[test]
fn an_option_is_a_selected_answer() {
    // When
    let answer = parse_permission_answer(&serde_json::json!({
        "sessionId": "c-1",
        "permission": { "requestId": "perm-1", "optionId": "allow-once" },
    }));

    // Then
    assert_eq!(
        answer,
        Ok(Some(PermissionAnswer::Selected {
            request_id: "perm-1".to_string(),
            option_id: "allow-once".to_string(),
        }))
    );
}

#[test]
fn cancelled_is_a_cancelled_answer() {
    // When
    let answer = parse_permission_answer(&serde_json::json!({
        "permission": { "requestId": "perm-1", "cancelled": true },
    }));

    // Then
    assert_eq!(
        answer,
        Ok(Some(PermissionAnswer::Cancelled {
            request_id: "perm-1".to_string(),
        }))
    );
}

#[test]
fn no_permission_is_no_answer() {
    // When / Then
    assert_eq!(
        parse_permission_answer(&serde_json::json!({ "sessionId": "c-1" })),
        Ok(None)
    );
}

#[test]
fn both_an_option_and_cancelled_is_refused_naming_both() {
    // When
    let refusal = parse_permission_answer(&serde_json::json!({
        "permission": { "requestId": "perm-1", "optionId": "allow-once", "cancelled": true },
    }))
    .expect_err("an answer is one or the other");

    // Then
    assert!(
        refusal.contains("optionId") && refusal.contains("cancelled"),
        "{refusal}"
    );
}

#[test]
fn an_answer_without_a_request_id_is_refused_naming_it() {
    // When
    let refusal = parse_permission_answer(&serde_json::json!({
        "permission": { "optionId": "allow-once" },
    }))
    .expect_err("the request must be named");

    // Then
    assert!(refusal.contains("requestId"), "{refusal}");
}
