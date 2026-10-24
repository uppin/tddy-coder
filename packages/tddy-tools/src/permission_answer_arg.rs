//! `subagent_resume`'s `permission` argument: the caller's answer to a coding subagent's pending
//! permission request.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-permission-yield.md

use tddy_discovery::subagent::PermissionAnswer;

/// The argument's name on `subagent_resume`.
pub const PERMISSION_ARG: &str = "permission";

/// The `permission` property of `subagent_resume`'s input schema.
pub fn permission_property() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["requestId"],
        "properties": {
            "requestId": {
                "type": "string",
                "description": "The `permissionRequest.requestId` the turn yielded with."
            },
            "optionId": {
                "type": "string",
                "description": "One of the request's `options[].optionId` — the answer the \
                                subagent receives."
            },
            "cancelled": {
                "type": "boolean",
                "description": "Answer the request as cancelled instead of choosing an option."
            }
        },
        "description": "Answer the coding subagent's pending permission request (the turn ended \
                        with stopReason \"permission_requested\"). The decision is yours: you may \
                        decide on your own, or ask the user first and answer with what they chose. \
                        The subagent's paused turn continues with the answer, and this call returns \
                        the rest of that turn."
    })
}

/// The answer `args` carries, `None` when it carries no `permission`. Exactly one of `optionId`
/// or `cancelled: true` is required; anything else is refused naming what is wrong.
pub fn parse_permission_answer(
    _args: &serde_json::Value,
) -> Result<Option<PermissionAnswer>, String> {
    // TODO(permission-yield): implement
    Err("TODO(permission-yield): parse_permission_answer is not implemented".to_string())
}
