//! `subagent_resume`'s `permission` argument: the caller's answer to a coding subagent's pending
//! permission request.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A coding subagent's permission request is the caller's to answer

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
    args: &serde_json::Value,
) -> Result<Option<PermissionAnswer>, String> {
    let Some(permission) = args.get(PERMISSION_ARG) else {
        return Ok(None);
    };
    let Some(object) = permission.as_object() else {
        return Err(format!(
            "{PERMISSION_ARG} must be an object naming the request (requestId) and either its optionId or cancelled: true"
        ));
    };
    let request_id = match object.get("requestId").and_then(serde_json::Value::as_str) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            return Err(format!(
                "{PERMISSION_ARG}.requestId must name the pending request: the permissionRequest.requestId the turn yielded with"
            ))
        }
    };
    answer_of(request_id, object.get("optionId"), object.get("cancelled")).map(Some)
}

/// The answer an `optionId` or a `cancelled` flag gives `request_id`: exactly one of the two.
fn answer_of(
    request_id: String,
    option: Option<&serde_json::Value>,
    cancelled: Option<&serde_json::Value>,
) -> Result<PermissionAnswer, String> {
    match (option, cancelled) {
        (Some(_), Some(_)) => Err(format!(
            "{PERMISSION_ARG} names both optionId and cancelled; an answer is one or the other"
        )),
        (Some(option), None) => match option.as_str() {
            Some(option_id) if !option_id.is_empty() => Ok(PermissionAnswer::Selected {
                request_id,
                option_id: option_id.to_string(),
            }),
            _ => Err(format!(
                "{PERMISSION_ARG}.optionId must name one of the request's options[].optionId"
            )),
        },
        (None, Some(cancelled)) => match cancelled.as_bool() {
            Some(true) => Ok(PermissionAnswer::Cancelled { request_id }),
            _ => Err(format!(
                "{PERMISSION_ARG}.cancelled must be true to answer the request as cancelled; to choose an option, give its optionId"
            )),
        },
        (None, None) => Err(format!(
            "{PERMISSION_ARG} must name an optionId, or cancelled: true"
        )),
    }
}
