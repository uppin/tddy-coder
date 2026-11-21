//! Claude Code's decision for a hook that waited for an orchestrator's answer: what the hook prints
//! as its only output, built from the `AwaitQuestionAnswer` response.
//!
//! It lives here, beside the answer handling, because two hook paths print it: the local
//! `tddy-tools session-hook --await-answer`, and a target session's hook-log follower, which writes
//! it into the answer file the hook on the target waits for (`#target-parity` 1/7). Only daemon-side
//! crates reach a target's shell, and `tddy-tools` already depends on this crate.

use tddy_service::proto::activity::AwaitQuestionAnswerResponse;

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
    // TODO(permission-answer): implement — move the body of
    // `tddy_tools::session_hook::permission_decision_json` here and have tddy-tools call it.
    let _ = (answer, permission_suggestions);
    todo!("permission_decision_json")
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
}
