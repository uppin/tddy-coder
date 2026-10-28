//! What `session-hook --await-answer` prints for each way an orchestrator's answer can arrive: Claude
//! Code's `PermissionRequest` decision JSON, or nothing (`#home-inbox` 6/8).
//!
//! The shape is Claude Code's contract (https://code.claude.com/docs/en/hooks § PermissionRequest
//! decision control): `hookSpecificOutput.decision.behavior` is `allow` or `deny`;
//! `updatedPermissions` applies to `allow`, `message` to `deny`.

use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::{json, Value};
use tddy_service::proto::activity::{AwaitQuestionAnswerResponse, AwaitQuestionOutcome};
use tddy_tools::session_hook::permission_decision_json;

/// The orchestrator chose `option_id`, saying `message`.
fn answered(option_id: &str, message: &str) -> AwaitQuestionAnswerResponse {
    AwaitQuestionAnswerResponse {
        outcome: AwaitQuestionOutcome::Answered as i32,
        option_id: option_id.to_string(),
        cancelled: false,
        message: message.to_string(),
        answered_by: "home-session-1".to_string(),
        ..Default::default()
    }
}

/// The rule Claude suggested for `cargo publish`.
fn the_suggestions() -> Value {
    json!([{
        "type": "addRules",
        "rules": [{ "toolName": "Bash", "ruleContent": "cargo publish" }],
        "behavior": "allow",
        "destination": "localSettings"
    }])
}

/// What Claude reads `decision` as.
fn decision(behaviour: Value) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": behaviour
        }
    })
}

fn printed(answer: &AwaitQuestionAnswerResponse, suggestions: Option<&Value>) -> Option<Value> {
    permission_decision_json(answer, suggestions)
        .map(|stdout| serde_json::from_str(&stdout).expect("the decision is JSON"))
}

#[test]
fn allow_once_prints_a_plain_allow() {
    // When
    let printed = printed(&answered("allow_once", ""), Some(&the_suggestions()));

    // Then the suggestions are not applied: once is once
    assert_eq!(printed, Some(decision(json!({ "behavior": "allow" }))));
}

#[test]
fn allow_always_prints_an_allow_that_applies_claudes_suggestions() {
    // When
    let printed = printed(&answered("allow_always", ""), Some(&the_suggestions()));

    // Then
    assert_eq!(
        printed,
        Some(decision(json!({
            "behavior": "allow",
            "updatedPermissions": the_suggestions()
        })))
    );
}

#[test]
fn reject_once_prints_a_deny_with_the_orchestrators_message() {
    // When
    let printed = printed(
        &answered("reject_once", "publish from the release branch"),
        None,
    );

    // Then
    assert_eq!(
        printed,
        Some(decision(json!({
            "behavior": "deny",
            "message": "publish from the release branch"
        })))
    );
}

#[rstest]
#[case::timed_out(AwaitQuestionAnswerResponse {
    outcome: AwaitQuestionOutcome::TimedOut as i32,
    ..Default::default()
})]
#[case::no_orchestrator(AwaitQuestionAnswerResponse {
    outcome: AwaitQuestionOutcome::NoOrchestrator as i32,
    ..Default::default()
})]
#[case::cancelled(AwaitQuestionAnswerResponse {
    outcome: AwaitQuestionOutcome::Answered as i32,
    cancelled: true,
    answered_by: "home-session-1".to_string(),
    ..Default::default()
})]
#[case::unspecified(AwaitQuestionAnswerResponse::default())]
fn an_answer_that_decides_nothing_prints_nothing(#[case] answer: AwaitQuestionAnswerResponse) {
    // When / Then Claude shows its own prompt
    assert_eq!(
        permission_decision_json(&answer, Some(&the_suggestions())),
        None
    );
}

#[test]
fn the_allow_decision_is_one_line_so_claude_reads_exactly_one_json_document() {
    // When
    let stdout =
        permission_decision_json(&answered("allow_once", ""), None).expect("allow_once decides");

    // Then
    assert_eq!(stdout.lines().count(), 1, "the decision {stdout:?}");
}
