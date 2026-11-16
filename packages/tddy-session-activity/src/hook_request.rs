//! The payload-to-request mapping of an agent's lifecycle hook: what a hook's stdin JSON
//! becomes as the two hook reports (`ReportSessionStatus`, `ReportAgentActivity`) the daemon's
//! activity handling serves.
//!
//! It lives here, beside that handling, so every hook path maps a payload the same way: the
//! local `tddy-tools session-hook` (which reports over HTTP or the agent-tool socket) and a
//! target session's hook-log follower in `tddy-agent-launch` (which delivers in-process). There
//! is one mapping, not a copy per path.

use serde::Deserialize;
use tddy_core::{
    activity_status_from_hook_with_tool, hook_event_asks_a_question, hook_event_fields,
    parse_hook_event, SessionActivityStatus, ASK_USER_QUESTION_TOOL, REQUEST_USER_INPUT_TOOL,
};
use tddy_service::proto::activity::{ReportAgentActivityRequest, ReportSessionStatusRequest};

/// The `ReportSessionStatus` request the hook event in `stdin_json` becomes, or `None` for an
/// event that maps to no status. `event` is the hook command's own event name, used when the
/// payload names none (Claude Code hooks bake it in; Cursor's rely on the payload alone). A
/// `WaitingForInput` report carries the event's own `message` — the question the agent is
/// waiting on — which the daemon records as the session's pending question.
pub fn session_status_request(
    session_id: &str,
    hook_token: &str,
    os_user: &str,
    event: &str,
    stdin_json: &str,
) -> Option<ReportSessionStatusRequest> {
    let parsed = parse_hook_event(stdin_json).ok();
    let notification_type = parsed.as_ref().and_then(|ev| ev.notification_type.clone());
    let tool_name = parsed.as_ref().and_then(|ev| ev.tool_name.clone());
    let hook_event_name = parsed
        .as_ref()
        .map(|ev| ev.hook_event_name.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(event);
    let status = activity_status_from_hook_with_tool(
        hook_event_name,
        notification_type.as_deref(),
        tool_name.as_deref(),
    )?;

    // Which conversation reports, and every top-level string field of the event: the daemon matches
    // the session's agent spec's `conversation.rotation` against them.
    let agent_conversation_id = parsed
        .as_ref()
        .and_then(|ev| ev.session_id.clone())
        .unwrap_or_default();
    let hook_fields = hook_event_fields(stdin_json);

    // A permission or a question carries what is asked — the event, the tool and that tool's input —
    // and none of the agent's own words: the `Notification` hook that follows the same prompt says
    // those.
    if hook_event_asks_a_question(hook_event_name, tool_name.as_deref()) {
        let tool_input_json = parsed
            .as_ref()
            .and_then(|ev| ev.tool_input.as_ref())
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        return Some(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: hook_token.to_string(),
            os_user: os_user.to_string(),
            status: status.as_wire().to_string(),
            message: String::new(),
            hook_event_name: hook_event_name.to_string(),
            tool_name: tool_name.unwrap_or_default(),
            tool_input_json,
            agent_conversation_id,
            hook_fields,
        });
    }

    let message = match status {
        SessionActivityStatus::WaitingForInput => parsed.as_ref().and_then(|ev| ev.message.clone()),
        _ => None,
    }
    .unwrap_or_default();
    Some(ReportSessionStatusRequest {
        session_id: session_id.to_string(),
        hook_token: hook_token.to_string(),
        os_user: os_user.to_string(),
        status: status.as_wire().to_string(),
        message,
        hook_event_name: hook_event_name.to_string(),
        tool_name: String::new(),
        tool_input_json: String::new(),
        agent_conversation_id,
        hook_fields,
    })
}

/// Tool-payload subset of a hook's stdin JSON.
///
/// Parsed separately from [`tddy_core::HookEvent`] on purpose: `HookEvent` derives `Eq`, which
/// `serde_json::Value` does not implement, so these dynamic fields cannot live there. Every field
/// is optional — the event type determines which are present (`tool_response` only on
/// `PostToolUse`).
#[derive(Debug, Default, Deserialize)]
struct HookToolPayload {
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
    #[serde(default)]
    tool_response: Option<serde_json::Value>,
}

/// The `ReportAgentActivity` request a tool-use hook's payload becomes.
///
/// Returns `None` for any event other than `PreToolUse` / `PostToolUse` (they carry no tool
/// payload, so nothing is reported). `input_json` is the `tool_input` object serialized to a JSON
/// string; `result_json` is the `tool_response` serialized (empty on `PreToolUse`, which has no
/// response); `is_error` / `error_message` are derived from the `tool_response`.
pub fn agent_activity_request(
    hook_event_name: &str,
    stdin_json: &str,
    session_id: &str,
    hook_token: &str,
    os_user: &str,
) -> Option<ReportAgentActivityRequest> {
    if hook_event_name != "PreToolUse" && hook_event_name != "PostToolUse" {
        return None;
    }

    // A malformed payload degrades to empty fields rather than failing the hook.
    let payload: HookToolPayload = serde_json::from_str(stdin_json).unwrap_or_default();

    // A question tool (`AskUserQuestion`, `request_user_input`) is a question, not a tool
    // call, so neither of its hooks is reported as one: the question goes to the daemon by
    // `ReportSessionStatus` instead. Its `PreToolUse` would leave a pending call that a declined
    // question never completes, and so its `PostToolUse` would have nothing to pair with and
    // write an unpaired `completed` row under a fresh id. Nothing reads such a row.
    if payload.tool_name.as_deref() == Some(ASK_USER_QUESTION_TOOL)
        || payload.tool_name.as_deref() == Some(REQUEST_USER_INPUT_TOOL)
    {
        return None;
    }

    let (is_error, error_message) = tool_response_error(payload.tool_response.as_ref());
    let input_json = payload
        .tool_input
        .map(|v| v.to_string())
        .unwrap_or_default();
    let result_json = payload
        .tool_response
        .map(|v| v.to_string())
        .unwrap_or_default();

    Some(ReportAgentActivityRequest {
        session_id: session_id.to_string(),
        hook_token: hook_token.to_string(),
        os_user: os_user.to_string(),
        event: hook_event_name.to_string(),
        tool_name: payload.tool_name.unwrap_or_default(),
        input_json,
        result_json,
        is_error,
        error_message,
    })
}

/// Derive `(is_error, error_message)` from a Claude Code `tool_response`.
///
/// Claude Code has no single standardized error schema; the common convention is a response object
/// carrying a non-empty `error` string. Anything else is treated as a success.
fn tool_response_error(tool_response: Option<&serde_json::Value>) -> (bool, String) {
    match tool_response
        .and_then(|v| v.get("error"))
        .and_then(|e| e.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(err) => (true, err.to_string()),
        None => (false, String::new()),
    }
}
