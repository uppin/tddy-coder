//! An orchestrator's answer to an `AskUserQuestion` reaches Claude through whichever hook holds it.
//!
//! When the waiting `PreToolUse` hook's hold ends unanswered, Claude fires `PermissionRequest` for the
//! same call, and that hook waits too — recorded, and held, as a question. On 2026-11-22 the answer
//! to that second hold was accepted and then mapped as a permission: it names no `allow_*` option,
//! so nothing was printed and Claude kept its own dialog open.
//!
//! Findings: docs/dev/todo/2026-11-22-coder-target-back-channel-and-activity-cursor.md § Gap 7.

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_service::proto::activity::{
    AwaitQuestionAnswerResponse, AwaitQuestionOutcome, QuestionAnswer,
};
use tddy_session_activity::hook_decision::question_decision_json_for;

/// The `tool_input` of the `AskUserQuestion` call.
fn the_call() -> Value {
    json!({
        "questions": [{
            "question": "Proceed with the fixes and the draft PR?",
            "header": "Scope",
            "multiSelect": false,
            "options": [
                { "label": "Fix + draft PR", "description": "" },
                { "label": "Red only", "description": "" }
            ]
        }]
    })
}

/// The orchestrator chose `label` for the first question.
fn answered_with(label: &str) -> AwaitQuestionAnswerResponse {
    AwaitQuestionAnswerResponse {
        outcome: AwaitQuestionOutcome::Answered as i32,
        answered_by: "01a16dc6-ab4c-7dc3-844e-baca4514f86e".to_string(),
        answers: vec![QuestionAnswer {
            question_index: 0,
            selected: vec![label.to_string()],
            other: None,
        }],
        ..Default::default()
    }
}

/// The call's input with the answer handed over, as Claude reads it.
fn the_call_answered_with(label: &str) -> Value {
    let mut input = the_call();
    input["answers"] = json!({ "Proceed with the fixes and the draft PR?": label });
    input
}

#[test]
fn a_question_held_by_its_permission_request_hook_is_answered_as_that_hooks_decision() {
    // When the orchestrator's answer reaches the PermissionRequest hook holding the question
    let decision = question_decision_json_for(
        "PermissionRequest",
        &answered_with("Fix + draft PR"),
        &the_call(),
    );

    // Then Claude is told to allow the call with the answers in its input — the PermissionRequest
    // decision, which runs the tool as if the person had answered
    let decision: Value =
        serde_json::from_str(&decision.expect("an answered question prints a decision")).unwrap();
    assert_eq!(
        decision,
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": {
                    "behavior": "allow",
                    "updatedInput": the_call_answered_with("Fix + draft PR"),
                }
            }
        })
    );
}

#[test]
fn a_question_held_by_its_pre_tool_use_hook_is_answered_as_before() {
    // When the answer reaches the PreToolUse hook holding the question
    let decision =
        question_decision_json_for("PreToolUse", &answered_with("Red only"), &the_call());

    // Then the decision is PreToolUse's
    let decision: Value = serde_json::from_str(&decision.expect("a decision")).unwrap();
    assert_eq!(
        decision,
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": the_call_answered_with("Red only"),
            }
        })
    );
}
