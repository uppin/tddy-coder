//! A coding agent's permission request on the conversation wire: the request the final frame of a
//! paused turn names, and the answer a resume carries back to the agent.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A coding subagent's permission request is the caller's to answer

use tddy_discovery::subagent::{PermissionAnswer, PermissionRequestView, TurnRequest};

/// Why a permission answer the main agent sent is refused, when the answer was delivered while a turn
/// holds the conversation: the answer reached its request, and the turn holding it carries on, with its
/// outcome going to the call that deferred the request. Not a failure of the answer, so it is said as one.
/// The wording is the shared one, so the daemon and the coding agent say the same thing.
pub(crate) fn answer_delivered_while_held(request_id: &str) -> String {
    tddy_discovery::subagent::answer_delivered_while_held(request_id)
}

/// The request as the final frame carries it: JSON, or empty when the turn did not pause on one.
#[must_use]
pub fn permission_request_json(request: Option<&PermissionRequestView>) -> String {
    request
        .map(|request| {
            serde_json::to_string(request)
                .expect("a permission request is plain data, so it always serializes")
        })
        .unwrap_or_default()
}

/// Apply the answer a resume carries, where it carries one. An answer this build cannot parse is an
/// error naming the field, not a resume that runs with the request left unanswered: the caller asked
/// for its answer to reach the agent.
pub fn with_permission_answer(
    requested: TurnRequest,
    answer_json: &str,
) -> Result<TurnRequest, String> {
    if answer_json.is_empty() {
        return Ok(requested);
    }
    let answer = serde_json::from_str::<PermissionAnswer>(answer_json).map_err(|e| {
        format!("ResumeAgentConversation permission_answer_json is not a permission answer: {e}")
    })?;
    Ok(requested.with_permission_answer(answer))
}

/// Hand the pending permission request `defer_permission_to_user` names to the user, where the
/// resume names one. Empty is no request. A resume that both hands a request to the user and
/// answers one is refused: a request is either answered by the caller or handed to the user.
pub fn with_permission_deferral(
    requested: TurnRequest,
    defer_permission_to_user: Option<&str>,
    answer_json: &str,
) -> Result<TurnRequest, String> {
    match defer_permission_to_user.filter(|request_id| !request_id.is_empty()) {
        None => Ok(requested),
        Some(_) if !answer_json.is_empty() => Err(
            "ResumeAgentConversation defer_permission_to_user and permission_answer_json cannot both be set: \
             a permission request is either answered by the caller or handed to the user"
                .to_string(),
        ),
        Some(request_id) => Ok(requested.deferring_permission_to_user(request_id)),
    }
}
