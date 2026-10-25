//! The records a coding subagent's permission request leaves on its update stream: the request, pending while a
//! turn holds it for the user, and its answer, once one is given.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use tddy_agent_backend::{AcpToolStatus, AcpUpdate, AcpUpdateSink};
use tddy_discovery::subagent::{
    permission_tool_call_id, request_input_json, AnsweredBy, PermissionAnswer,
    PermissionRequestView,
};

use crate::parked_permissions::{Answered, ParkedPermissions};

/// The kind a request's record names. A permission is not a tool the agent ran, so it reads as `other`, as its
/// transcript entry does.
const PERMISSION_KIND: &str = "other";

/// The name the main agent's answers carry as `answeredBy`.
const MAIN_AGENT_NAME: &str = "main-agent";

/// The record of a request its turn holds for the user: pending, its input saying it was deferred to the user.
pub(crate) fn deferred_request_record(request: &PermissionRequestView) -> AcpUpdate {
    AcpUpdate::ToolCallRecord {
        id: permission_tool_call_id(&request.request_id),
        title: titled(request),
        kind: PERMISSION_KIND.to_string(),
        status: AcpToolStatus::Pending,
        raw_input: Some(request_input_json(request, true)),
        raw_output: None,
    }
}

/// The record of an answered request: completed, its raw output naming who answered it and with which option, or
/// that it was cancelled. The request's input is repeated, so the record reads the same request the pending one did.
pub(crate) fn answer_record(answered: &Answered) -> AcpUpdate {
    let answered_by = answered_by_name(&answered.by);
    let raw_output = match &answered.answer {
        PermissionAnswer::Selected { option_id, .. } => {
            serde_json::json!({ "answeredBy": answered_by, "optionId": option_id })
        }
        PermissionAnswer::Cancelled { .. } => {
            serde_json::json!({ "answeredBy": answered_by, "cancelled": true })
        }
    };
    AcpUpdate::ToolCallRecord {
        id: permission_tool_call_id(answered.answer.request_id()),
        title: titled(&answered.request),
        kind: PERMISSION_KIND.to_string(),
        status: AcpToolStatus::Completed,
        raw_input: Some(request_input_json(
            &answered.request,
            answered.deferred_to_user,
        )),
        raw_output: Some(raw_output),
    }
}

/// Send the record of `request_id`'s answer to `sink`, when the request has been answered.
pub(crate) fn record_answer(
    sink: &AcpUpdateSink,
    permissions: &ParkedPermissions,
    request_id: &str,
) {
    if let Some(answered) = permissions.answered(request_id) {
        sink(answer_record(&answered));
    }
}

/// Who answered, as the answer's record names them: a user by login, the main agent as `main-agent`.
fn answered_by_name(by: &AnsweredBy) -> String {
    match by {
        AnsweredBy::MainAgent => MAIN_AGENT_NAME.to_string(),
        AnsweredBy::User(login) => login.clone(),
    }
}

fn titled(request: &PermissionRequestView) -> String {
    format!("Permission: {}", request.tool_call.title)
}
