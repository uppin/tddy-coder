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
/// `PostToolUse`, Cursor's `tool_output` string and `tool_use_id` only on its tool hooks).
#[derive(Debug, Default, Deserialize)]
struct HookToolPayload {
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
    #[serde(default)]
    tool_response: Option<serde_json::Value>,
    /// Cursor's tool result, a JSON *string* on `postToolUse` (kept a [`serde_json::Value`]
    /// so a non-string one still parses, then handled by shape).
    #[serde(default)]
    tool_output: Option<serde_json::Value>,
    /// Cursor's id for the tool call, pairing its pre and post reports.
    #[serde(default)]
    tool_use_id: Option<String>,
    /// Cursor's `postToolUseFailure` message, and what kind of failure it was.
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    failure_type: Option<String>,
}

/// The `ReportAgentActivity` request a tool-use hook's payload becomes.
///
/// Accepts Claude Code's `PreToolUse` / `PostToolUse` and Cursor's `preToolUse` /
/// `postToolUse` / `postToolUseFailure`, and reports every other event as nothing (they
/// carry no tool payload, so nothing is reported). The wire `event` is always the PascalCase
/// pair — that is the daemon's contract. `input_json` is the `tool_input` object serialized
/// to a JSON string; `result_json` is the `tool_response` serialized — or Cursor's
/// `tool_output` string parsed and re-serialized (empty on a pre, which has no result);
/// `is_error` / `error_message` are derived from the response; `tool_use_id` is the agent's
/// id for the call when it sends one.
pub fn agent_activity_request(
    hook_event_name: &str,
    stdin_json: &str,
    session_id: &str,
    hook_token: &str,
    os_user: &str,
) -> Option<ReportAgentActivityRequest> {
    // Cursor's failure hook fires *instead of* its post hook, so it closes the call's row.
    let event = match hook_event_name {
        "PreToolUse" | "preToolUse" => "PreToolUse",
        "PostToolUse" | "postToolUse" | "postToolUseFailure" => "PostToolUse",
        _ => return None,
    };

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

    // Cursor's result is a JSON string; unparseable text degrades to itself, as on the daemon.
    let tool_result = match payload.tool_output {
        Some(serde_json::Value::String(text)) => {
            Some(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
        }
        output => output,
    };
    let result = payload.tool_response.as_ref().or(tool_result.as_ref());
    let (is_error, error_message) = if hook_event_name == "postToolUseFailure" {
        (
            true,
            payload
                .error_message
                .or(payload.failure_type)
                .unwrap_or_default(),
        )
    } else if payload.tool_response.is_some() {
        tool_response_error(payload.tool_response.as_ref())
    } else {
        cursor_tool_output_error(tool_result.as_ref())
    };
    let input_json = payload
        .tool_input
        .map(|v| v.to_string())
        .unwrap_or_default();
    let result_json = result.map(|v| v.to_string()).unwrap_or_default();

    Some(ReportAgentActivityRequest {
        session_id: session_id.to_string(),
        hook_token: hook_token.to_string(),
        os_user: os_user.to_string(),
        event: event.to_string(),
        tool_name: payload.tool_name.unwrap_or_default(),
        input_json,
        result_json,
        is_error,
        error_message,
        tool_use_id: payload.tool_use_id.unwrap_or_default(),
    })
}

/// Derive `(is_error, error_message)` from Cursor's parsed `tool_output`: a non-empty `error`
/// string, as on a Claude Code response — or the `isError` flag, which Cursor's redacted MCP
/// outputs carry with no message at all.
fn cursor_tool_output_error(tool_output: Option<&serde_json::Value>) -> (bool, String) {
    match tool_output
        .and_then(|v| v.get("error"))
        .and_then(|e| e.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(err) => (true, err.to_string()),
        None => match tool_output
            .and_then(|v| v.get("isError"))
            .and_then(|v| v.as_bool())
        {
            Some(true) => (true, String::new()),
            _ => (false, String::new()),
        },
    }
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const SESSION_ID: &str = "cursor-session-1";
    const HOOK_TOKEN: &str = "tok-cursor-1";
    const OS_USER: &str = "dev";

    /// Map a hook stdin JSON to an agent-activity request with the standard test identity.
    fn map(hook_event_name: &str, stdin_json: &str) -> Option<ReportAgentActivityRequest> {
        agent_activity_request(hook_event_name, stdin_json, SESSION_ID, HOOK_TOKEN, OS_USER)
    }

    /// Cursor's `preToolUse` carries `tool_name` + `tool_input` and the call's `tool_use_id`,
    /// so it maps to a `PreToolUse` report naming that id, with no result yet.
    #[test]
    fn cursor_pre_tool_use_maps_to_a_pre_report_naming_the_tools_id() {
        // Given
        let stdin_json = r#"{"hook_event_name":"preToolUse","tool_name":"Shell","tool_input":{"command":"ls"},"tool_use_id":"tool-1","cwd":"/repo"}"#;

        // When
        let req = map("preToolUse", stdin_json).expect("preToolUse must produce a request");

        // Then
        assert_eq!(req.event, "PreToolUse");
        assert_eq!(req.tool_name, "Shell");
        assert_eq!(req.input_json, r#"{"command":"ls"}"#);
        assert_eq!(req.result_json, "");
        assert_eq!(req.tool_use_id, "tool-1");
        assert!(!req.is_error);
        assert_eq!(req.session_id, SESSION_ID);
        assert_eq!(req.hook_token, HOOK_TOKEN);
        assert_eq!(req.os_user, OS_USER);
    }

    /// Cursor's `postToolUse` carries the result as `tool_output`, a JSON *string*: it is
    /// parsed into the report's `result_json`, and the call's `tool_use_id` pairs it with
    /// its pre.
    #[test]
    fn cursor_post_tool_use_parses_the_string_tool_output_into_the_result() {
        // Given — the payload shape of Cursor's docs and of a captured `ls` run
        let stdin_json = r#"{"hook_event_name":"postToolUse","tool_name":"Shell","tool_input":{"command":"ls"},"tool_output":"{\"output\":\"src\\n\",\"exitCode\":0}","tool_use_id":"tool-1","duration":869.285}"#;

        // When
        let req = map("postToolUse", stdin_json).expect("postToolUse must produce a request");

        // Then
        assert_eq!(req.event, "PostToolUse");
        assert_eq!(req.tool_name, "Shell");
        assert_eq!(req.result_json, r#"{"exitCode":0,"output":"src\n"}"#);
        assert_eq!(req.tool_use_id, "tool-1");
        assert!(!req.is_error);
        assert_eq!(req.error_message, "");
    }

    /// Cursor's `postToolUseFailure` fires *instead of* `postToolUse` when the tool fails,
    /// times out or is denied: it maps to an error `PostToolUse` report for the same
    /// `tool_use_id`, carrying the failure's message.
    #[test]
    fn cursor_post_tool_use_failure_maps_to_an_error_post_for_the_tools_id() {
        // Given
        let stdin_json = r#"{"hook_event_name":"postToolUseFailure","tool_name":"Shell","tool_input":{"command":"npm test"},"tool_use_id":"tool-9","error_message":"Command timed out after 30s","failure_type":"timeout","duration":5000,"is_interrupt":false}"#;

        // When
        let req = map("postToolUseFailure", stdin_json)
            .expect("postToolUseFailure must produce a request");

        // Then
        assert_eq!(req.event, "PostToolUse");
        assert_eq!(req.tool_name, "Shell");
        assert_eq!(req.input_json, r#"{"command":"npm test"}"#);
        assert_eq!(req.tool_use_id, "tool-9");
        assert!(req.is_error);
        assert_eq!(req.error_message, "Command timed out after 30s");
    }

    /// Cursor's shell, MCP, file-edit and response hooks fire *alongside* `preToolUse` /
    /// `postToolUse` for the same tool call (or carry no tool payload at all), so none is
    /// reported as a tool call: each would duplicate — or invent — a row.
    #[rstest::rstest]
    #[case::before_shell_execution(
        "beforeShellExecution",
        r#"{"hook_event_name":"beforeShellExecution","command":"ls","cwd":"/repo"}"#
    )]
    #[case::after_shell_execution(
        "afterShellExecution",
        r#"{"hook_event_name":"afterShellExecution","command":"ls","output":"src","duration":12}"#
    )]
    #[case::before_mcp_execution(
        "beforeMCPExecution",
        r#"{"hook_event_name":"beforeMCPExecution","tool_name":"read","tool_input":"{}","mcp_server_name":"docs"}"#
    )]
    #[case::after_mcp_execution(
        "afterMCPExecution",
        r#"{"hook_event_name":"afterMCPExecution","tool_name":"read","tool_input":"{}","mcp_server_name":"docs","result_json":"{}","duration":12}"#
    )]
    #[case::after_file_edit(
        "afterFileEdit",
        r#"{"hook_event_name":"afterFileEdit","file_path":"/repo/src/main.rs","edits":[]}"#
    )]
    #[case::after_agent_response(
        "afterAgentResponse",
        r#"{"hook_event_name":"afterAgentResponse","text":"done"}"#
    )]
    fn cursor_execution_and_response_hooks_are_not_reported_as_tool_calls(
        #[case] event: &str,
        #[case] stdin_json: &str,
    ) {
        // When
        let request = map(event, stdin_json);

        // Then
        assert_eq!(request, None, "{event} must not be reported as a tool call");
    }

    /// A `tool_output` that is not JSON degrades to its text, as an unparseable Claude
    /// response degrades on the daemon: the row keeps what the tool said.
    #[test]
    fn an_unparseable_tool_output_degrades_to_its_text() {
        // Given
        let stdin_json = r#"{"hook_event_name":"postToolUse","tool_name":"Shell","tool_input":{"command":"ls"},"tool_output":"not json{{","tool_use_id":"tool-2"}"#;

        // When
        let req = map("postToolUse", stdin_json).expect("postToolUse must produce a request");

        // Then
        assert_eq!(req.result_json, r#""not json{{""#);
        assert_eq!(req.tool_use_id, "tool-2");
        assert!(!req.is_error);
    }

    /// A `tool_output` flagging `isError` is reported as an error, even with no message —
    /// Cursor's redacted MCP outputs carry only the flag.
    #[test]
    fn a_tool_output_flagging_is_error_is_reported_as_an_error() {
        // Given
        let stdin_json = r#"{"hook_event_name":"postToolUse","tool_name":"MCP:read","tool_input":{},"tool_output":"{\"redacted\":true,\"isError\":true}","tool_use_id":"tool-3"}"#;

        // When
        let req = map("postToolUse", stdin_json).expect("postToolUse must produce a request");

        // Then
        assert!(req.is_error);
        assert_eq!(req.error_message, "");
        assert_eq!(req.tool_use_id, "tool-3");
    }

    /// A malformed payload degrades to empty fields rather than failing the hook, as a
    /// Claude Code one does.
    #[test]
    fn a_malformed_cursor_payload_degrades_to_empty_fields() {
        // When
        let req = map("preToolUse", "{broken").expect("a malformed payload still maps");

        // Then
        assert_eq!(req.event, "PreToolUse");
        assert_eq!(req.tool_name, "");
        assert_eq!(req.input_json, "");
        assert_eq!(req.tool_use_id, "");
    }
}
