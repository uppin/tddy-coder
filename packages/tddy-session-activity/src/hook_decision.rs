//! Claude Code's decision for a hook that waited for an orchestrator's answer: what the hook prints
//! as its only output, built from the `AwaitQuestionAnswer` response.
//!
//! It lives here, beside the answer handling, because two hook paths print it: the local
//! `tddy-tools session-hook --await-answer`, and a target session's hook-log follower, which writes
//! it into the answer file the hook on the target waits for (`#target-parity` 1/7). Only daemon-side
//! crates reach a target's shell, and `tddy-tools` already depends on this crate.

use tddy_service::proto::activity::{AwaitQuestionAnswerResponse, AwaitQuestionOutcome};

/// Claude Code's `PermissionRequest` decision for `answer`
/// (<https://code.claude.com/docs/en/hooks> § PermissionRequest decision control):
///
/// - `allow_once` → `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`;
/// - `allow_always` → the same, with `"updatedPermissions"` set to Claude's `permission_suggestions`;
/// - a `reject_*` option → `"behavior":"deny"` with `"message"`;
/// - `cancelled`, `TIMED_OUT`, `NO_ORCHESTRATOR`, an unknown option → `None`: nothing is printed,
///   and Claude's own prompt follows.
#[must_use]
pub fn permission_decision_json(
    answer: &AwaitQuestionAnswerResponse,
    permission_suggestions: Option<&serde_json::Value>,
) -> Option<String> {
    if AwaitQuestionOutcome::try_from(answer.outcome) != Ok(AwaitQuestionOutcome::Answered)
        || answer.cancelled
    {
        return None;
    }
    let decision = match answer.option_id.as_str() {
        "allow_once" => serde_json::json!({ "behavior": "allow" }),
        "allow_always" => match permission_suggestions {
            Some(suggestions) => {
                serde_json::json!({ "behavior": "allow", "updatedPermissions": suggestions })
            }
            None => serde_json::json!({ "behavior": "allow" }),
        },
        option if option.starts_with("reject_") => {
            serde_json::json!({ "behavior": "deny", "message": answer.message })
        }
        _ => return None,
    };
    Some(
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": decision,
            }
        })
        .to_string(),
    )
}

/// Claude Code's `PreToolUse` decision handing the orchestrator's answers to an `AskUserQuestion`
/// over: `permissionDecision: "allow"` with `updatedInput` — the call's own `tool_input` with an
/// `answers` map (question text → the chosen labels joined with `", "`, then `other`). `None` when
/// the wait was not answered or nothing can be handed over; Claude then shows its own dialog.
#[must_use]
pub fn question_decision_json(
    answer: &AwaitQuestionAnswerResponse,
    tool_input: &serde_json::Value,
) -> Option<String> {
    // TODO(question-answer): implement — move `tddy_tools::session_hook::question_decision_json`
    // and `encode_question_decision` here and have tddy-tools call them.
    let _ = (answer, tool_input);
    todo!("question_decision_json")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_service::proto::activity::AwaitQuestionOutcome;

    use super::*;

    fn answered(option_id: &str, message: &str) -> AwaitQuestionAnswerResponse {
        AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::Answered as i32,
            option_id: option_id.to_string(),
            message: message.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn an_allow_once_answer_is_claudes_allow_decision() {
        // When
        let decision = permission_decision_json(&answered("allow_once", ""), None);

        // Then
        assert_eq!(
            decision.map(|json| serde_json::from_str::<serde_json::Value>(&json).unwrap()),
            Some(serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {"behavior": "allow"}
                }
            }))
        );
    }

    #[test]
    fn a_reject_answer_is_claudes_deny_decision_with_its_message() {
        // When
        let decision = permission_decision_json(&answered("reject_once", "not on prod"), None);

        // Then
        assert_eq!(
            decision.map(|json| serde_json::from_str::<serde_json::Value>(&json).unwrap()),
            Some(serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {"behavior": "deny", "message": "not on prod"}
                }
            }))
        );
    }

    #[test]
    fn no_orchestrator_is_no_decision() {
        // Given
        let answer = AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::NoOrchestrator as i32,
            ..Default::default()
        };

        // When / Then
        assert_eq!(permission_decision_json(&answer, None), None);
    }

    #[test]
    fn an_answered_question_is_handed_to_claude_in_its_own_input() {
        // Given the orchestrator chose "Rust" for the call's one question
        let tool_input = serde_json::json!({"questions": [{
            "question": "Which language?", "header": "Language", "multiSelect": false,
            "options": [{"label": "Rust"}, {"label": "Go"}]
        }]});
        let answer = AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::Answered as i32,
            answers: vec![tddy_service::proto::activity::QuestionAnswer {
                question_index: 0,
                selected: vec!["Rust".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };

        // When
        let decision = question_decision_json(&answer, &tool_input);

        // Then
        let mut updated = tool_input.clone();
        updated["answers"] = serde_json::json!({"Which language?": "Rust"});
        assert_eq!(
            decision.map(|json| serde_json::from_str::<serde_json::Value>(&json).unwrap()),
            Some(serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "updatedInput": updated
                }
            }))
        );
    }
}
