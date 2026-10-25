//! `subagent_resume`'s `deferPermissionToUser` argument: hand a coding subagent's pending
//! permission request to the user instead of answering it.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use crate::permission_answer_arg::PERMISSION_ARG;

/// The argument's name on `subagent_resume`.
pub const DEFER_ARG: &str = "deferPermissionToUser";

/// The `deferPermissionToUser` property of `subagent_resume`'s input schema.
pub fn defer_property() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "description": "Hand the pending permission request with this `requestId` to the user: \
                        it shows in the subagent's conversation view with its options, and the \
                        user's choice is delivered to the subagent. The call returns {responseId, \
                        pending: true, awaitingUser: true}; collect the rest of the turn with \
                        subagent_await. The request stays answerable by you until the user answers: \
                        an answer sent with `permission` meanwhile is delivered at once, and its own \
                        call is refused, saying it was delivered, while the turn carries on under \
                        this call. The first answer wins; a later one is refused, naming who \
                        answered. Cannot be combined with `permission` in one call."
    })
}

/// The request id `args` defers, `None` when it defers nothing; refused when it also answers
/// (`permission`), naming both.
pub fn parse_permission_deferral(args: &serde_json::Value) -> Result<Option<String>, String> {
    let Some(deferred) = args.get(DEFER_ARG) else {
        return Ok(None);
    };
    if args.get(PERMISSION_ARG).is_some() {
        return Err(format!(
            "{DEFER_ARG} and {PERMISSION_ARG} cannot both be given: a permission request is either \
             answered by the caller or handed to the user, not both"
        ));
    }
    match deferred.as_str() {
        Some(request_id) if !request_id.is_empty() => Ok(Some(request_id.to_string())),
        _ => Err(format!(
            "{DEFER_ARG} must name the pending request: the permissionRequest.requestId the turn yielded with"
        )),
    }
}
