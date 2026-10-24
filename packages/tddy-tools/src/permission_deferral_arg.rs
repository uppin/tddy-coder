//! `subagent_resume`'s `deferPermissionToUser` argument: hand a coding subagent's pending
//! permission request to the user instead of answering it.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

/// The argument's name on `subagent_resume`.
pub const DEFER_ARG: &str = "deferPermissionToUser";

/// The `deferPermissionToUser` property of `subagent_resume`'s input schema.
pub fn defer_property() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "description": "Hand the pending permission request with this `requestId` to the user: \
                        it shows in the subagent's conversation view with its options, and the \
                        user's choice is delivered to the subagent. Returns {responseId, pending, \
                        awaitingUser}; collect the rest of the turn with subagent_await. You may \
                        still answer it yourself with `permission` — the first answer wins."
    })
}

/// The request id `args` defers, `None` when it defers nothing; refused when it also answers
/// (`permission`), naming both.
pub fn parse_permission_deferral(_args: &serde_json::Value) -> Result<Option<String>, String> {
    // TODO(web-approvals): implement
    Err("TODO(web-approvals): parse_permission_deferral is not implemented".to_string())
}
