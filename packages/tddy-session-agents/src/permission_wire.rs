//! A coding agent's permission request on the conversation wire: the request the final frame of a
//! paused turn names, and the answer a resume carries back to the agent.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-permission-yield.md

use tddy_discovery::subagent::{PermissionAnswer, PermissionRequestView, TurnRequest};

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
